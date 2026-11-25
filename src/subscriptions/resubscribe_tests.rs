use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use crate::client::ids::RequestId;
use crate::common::test_utils::helpers::{binary_proto, error_frame, handshake_frames, proto_error_response, proto_response, TEST_REQ_ID_FIRST};
use crate::contracts::Contract;
use crate::market_data::historical::{BarSize, ToDuration};
use crate::messages::{IncomingMessages, TRANSPORT_RECONNECT_CODE};
use crate::server_versions;
use crate::stubs::MessageBusStub;
use crate::subscriptions::SubscriptionItem;
use crate::testdata::builders::market_data::{historical_data_bar, realtime_bar_tick, HistoricalDataResponse};
use crate::testdata::builders::ResponseProtoEncoder;
use crate::Error;

/// `open` call index, and the error it fails with before sending.
type OpenFailure = (usize, fn() -> Error);

struct Case {
    name: &'static str,
    /// Requests whose subscription gets a `ConnectionReset` instead of bars.
    resets: usize,
    open_fails: &'static [OpenFailure],
    reconnect_fails: bool,
    /// TWS error code sent on the first request after its bars.
    stream_error: Option<i32>,
    expected: &'static [&'static str],
    open_calls: usize,
}

const CASES: &[Case] = &[
    Case {
        name: "streams items, no gap notice on the first open",
        resets: 0,
        open_fails: &[],
        reconnect_fails: false,
        stream_error: None,
        expected: &["bar", "bar"],
        open_calls: 1,
    },
    Case {
        name: "resubscribes after a reset, gap notice first",
        resets: 1,
        open_fails: &[],
        reconnect_fails: false,
        stream_error: None,
        expected: &["gap", "bar", "bar"],
        open_calls: 2,
    },
    Case {
        name: "lost again while reopening, still one gap notice",
        resets: 1,
        open_fails: &[(1, || Error::ConnectionReset)],
        reconnect_fails: false,
        stream_error: None,
        expected: &["gap", "bar", "bar"],
        open_calls: 3,
    },
    Case {
        name: "first open lost, retried without a gap notice",
        resets: 0,
        open_fails: &[(0, || Error::ConnectionReset)],
        reconnect_fails: false,
        stream_error: None,
        expected: &["bar", "bar"],
        open_calls: 2,
    },
    Case {
        name: "other open error ends the stream",
        resets: 0,
        open_fails: &[(0, || Error::AlreadySubscribed)],
        reconnect_fails: false,
        stream_error: None,
        expected: &["AlreadySubscribed"],
        open_calls: 1,
    },
    Case {
        name: "reconnect gave up: its error ends the stream",
        resets: 1,
        open_fails: &[],
        reconnect_fails: true,
        stream_error: None,
        expected: &["ConnectionFailed"],
        open_calls: 1,
    },
    Case {
        name: "reconnect gave up while reopening: its error ends the stream",
        resets: 0,
        open_fails: &[(0, || Error::ConnectionReset)],
        reconnect_fails: true,
        stream_error: None,
        expected: &["ConnectionFailed"],
        open_calls: 1,
    },
    Case {
        name: "other stream error ends the stream, no resubscribe",
        resets: 0,
        open_fails: &[],
        reconnect_fails: false,
        stream_error: Some(420),
        expected: &["bar", "bar", "error 420"],
        open_calls: 1,
    },
];

fn bus(case: &Case) -> Arc<MessageBusStub> {
    let bar = |time| proto_response(IncomingMessages::RealTimeBars, realtime_bar_tick().time(time).encode_proto());
    let mut responses = vec![bar(1678323335), bar(1678323340)];
    if let Some(code) = case.stream_error {
        responses.push(proto_error_response(TEST_REQ_ID_FIRST, code, "Invalid Real-time Query"));
    }
    let mut stub = MessageBusStub::with_ordered_responses(responses).with_connection_resets(case.resets);
    if case.reconnect_fails {
        stub = stub.with_failed_reconnect();
    }
    Arc::new(stub)
}

/// The scripted failure for `open` call `call`, if any.
fn open_failure(case: &Case, call: usize) -> Option<Error> {
    case.open_fails.iter().find(|(i, _)| *i == call).map(|(_, error)| error())
}

fn label<T>(item: Result<SubscriptionItem<T>, Error>) -> String {
    match item {
        Ok(SubscriptionItem::Data(_)) => "bar".into(),
        Ok(SubscriptionItem::Notice(n)) if n.code == TRANSPORT_RECONNECT_CODE => "gap".into(),
        Ok(SubscriptionItem::Notice(n)) => format!("notice {n}"),
        Err(Error::Notice(n)) => format!("error {}", n.code),
        Err(e) => format!("{e:?}"),
    }
}

#[cfg(feature = "sync")]
#[test]
fn resubscribing_sync() {
    use crate::client::sync::Client;

    for case in CASES {
        let bus = bus(case);
        let client = Client::stubbed(bus.clone(), server_versions::SIZE_RULES);
        let contract = Contract::stock("AAPL").build();
        let calls = Cell::new(0);

        let open = || {
            let call = calls.replace(calls.get() + 1);
            match open_failure(case, call) {
                Some(error) => Err(error),
                None => client.realtime_bars(&contract).subscribe(),
            }
        };
        let labels: Vec<String> = client.resubscribing(open).map(label).collect();

        assert_eq!(labels, case.expected, "{}", case.name);
        assert_eq!(calls.get(), case.open_calls, "{}: open calls", case.name);
    }
}

#[cfg(feature = "async")]
#[tokio::test]
async fn resubscribing_async() {
    use crate::client::r#async::Client;
    use futures::StreamExt;

    for case in CASES {
        let bus = bus(case);
        let client = Client::stubbed(bus.clone(), server_versions::SIZE_RULES);
        let contract = Contract::stock("AAPL").build();
        let calls = Cell::new(0);

        let open = || {
            let call = calls.replace(calls.get() + 1);
            let failure = open_failure(case, call);
            let (client, contract) = (&client, &contract);
            async move {
                match failure {
                    Some(error) => Err(error),
                    None => client.realtime_bars(contract).subscribe().await,
                }
            }
        };
        let labels: Vec<String> = client.resubscribing(open).map(label).collect().await;

        assert_eq!(labels, case.expected, "{}", case.name);
        assert_eq!(calls.get(), case.open_calls, "{}: open calls", case.name);
    }
}

// A keep-up-to-date bar request across an internet outage, on a real bus over
// a `MemoryStream`. The notice sequences are those of the paper TWS runs
// (server 223) in `docs/wire-facts/market-data.md#streams-across-an-internet-outage`.

/// One step of an outage script.
enum Step {
    /// TWS sends this frame.
    Push(Vec<u8>),
    /// The transport drops; TWS answers the reconnect's handshake.
    TransportLost,
    /// The wrapper yields an item with this label.
    Item(&'static str),
    /// `open` has been called exactly this often, also a moment later.
    Opens(usize),
}

const LOST: (i32, &str) = (1100, "Connectivity between IBKR and Trader Workstation has been lost.");
const RESTORED: (i32, &str) = (
    1102,
    "Connectivity between IBKR and Trader Workstation has been restored - data maintained. All data farms are connected: cashfarm; usfarm; usopt; cashhmds; ushmds; secdefnj.",
);
const USHMDS_BROKEN: (i32, &str) = (2105, "HMDS data farm connection is broken:ushmds");
const CASHHMDS_BROKEN: (i32, &str) = (2105, "HMDS data farm connection is broken:cashhmds");
const USHMDS_OK: (i32, &str) = (2106, "HMDS data farm connection is OK:ushmds");
const CASHHMDS_OK: (i32, &str) = (2106, "HMDS data farm connection is OK:cashhmds");
const JFARM_BROKEN: (i32, &str) = (2103, "Market data farm connection is broken:jfarm");
const JFARM_INACTIVE: (i32, &str) = (2108, "Market data farm connection is inactive but should be available upon demand.jfarm");

/// A global notice (request id -1).
fn notice(&(code, message): &(i32, &str)) -> Step {
    Step::Push(error_frame(-1, code, message))
}

/// The 10182 ending the request `open` sent on call `call`.
fn disconnected(call: i32) -> Step {
    Step::Push(error_frame(
        RequestId::nth(call).raw(),
        10182,
        "Failed to request live updates (disconnected).",
    ))
}

/// The initial bars of the request `open` sent on call `call`.
fn bars(call: i32) -> Step {
    let response = HistoricalDataResponse {
        request_id: RequestId::nth(call).raw(),
        bars: vec![historical_data_bar(1_764_074_520 + 60 * i64::from(call))],
    };
    Step::Push(binary_proto(IncomingMessages::HistoricalData as i32, &response.to_proto()))
}

/// The request is open and has delivered its bars; then the HMDS farms break
/// with the 10182, as at every cut.
fn broken_with_10182(farms: &[(i32, &'static str)]) -> Vec<Step> {
    let mut steps = vec![Step::Opens(1), bars(0), Step::Item("bar")];
    steps.extend(farms.iter().map(notice));
    steps.extend([
        disconnected(0),
        Step::Item("notice [10182] Failed to request live updates (disconnected)."),
    ]);
    steps
}

/// 2- and 6-minute runs: no reopen until the last HMDS farm is back, exactly
/// one after it, and its bars are yielded.
fn reopens_after_last_hmds_ok() -> Vec<Step> {
    let mut steps = broken_with_10182(&[USHMDS_BROKEN, CASHHMDS_BROKEN]);
    steps.extend([
        notice(&LOST),
        notice(&USHMDS_OK),
        Step::Opens(1),
        notice(&CASHHMDS_OK),
        Step::Opens(2),
        bars(1),
        Step::Item("bar"),
        notice(&RESTORED),
        Step::Opens(2),
    ]);
    steps
}

/// 15-second run: jfarm broke, went inactive and never reported OK; it does
/// not hold the reopen back.
fn market_data_farm_without_ok_does_not_block() -> Vec<Step> {
    let mut steps = broken_with_10182(&[USHMDS_BROKEN, JFARM_BROKEN, JFARM_INACTIVE, CASHHMDS_BROKEN]);
    steps.extend([
        notice(&USHMDS_OK),
        Step::Opens(1),
        notice(&CASHHMDS_OK),
        Step::Opens(2),
        bars(1),
        Step::Item("bar"),
    ]);
    steps
}

/// The reopened request draws 10182 again: the wrapper waits again.
fn repeated_10182_waits_again() -> Vec<Step> {
    let mut steps = broken_with_10182(&[CASHHMDS_BROKEN]);
    steps.extend([
        notice(&CASHHMDS_OK),
        Step::Opens(2),
        notice(&CASHHMDS_BROKEN),
        disconnected(1),
        Step::Item("notice [10182] Failed to request live updates (disconnected)."),
        Step::Opens(2),
        notice(&CASHHMDS_OK),
        Step::Opens(3),
        bars(2),
        Step::Item("bar"),
    ]);
    steps
}

/// The transport drops during the wait: the reconnect path reopens, behind
/// the gap notice.
fn transport_loss_during_wait_reconnects() -> Vec<Step> {
    let mut steps = broken_with_10182(&[CASHHMDS_BROKEN]);
    steps.extend([
        Step::Opens(1),
        Step::TransportLost,
        Step::Item("gap"),
        Step::Opens(2),
        bars(1),
        Step::Item("bar"),
    ]);
    steps
}

type Script = fn() -> Vec<Step>;

const SCRIPTS: &[(&str, Script)] = &[
    ("reopens after the last HMDS OK", reopens_after_last_hmds_ok),
    ("market data farm without OK does not block", market_data_farm_without_ok_does_not_block),
    ("repeated 10182 waits again", repeated_10182_waits_again),
    ("transport loss during the wait reconnects", transport_loss_during_wait_reconnects),
];

/// How long a step waits for the wrapper, and how long `Opens` watches for an
/// extra call.
const STEP_TIMEOUT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(100);

/// Disconnects the client when dropped.
#[cfg(feature = "sync")]
struct Disconnect<'a>(&'a crate::client::sync::Client);

#[cfg(feature = "sync")]
impl Drop for Disconnect<'_> {
    fn drop(&mut self) {
        self.0.disconnect();
    }
}

#[cfg(feature = "sync")]
#[test]
fn resubscribing_across_farm_outage_sync() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use crate::client::sync::Client;
    use crate::common::test_utils::helpers::wait_for;
    use crate::connection::sync::Connection;
    use crate::transport::sync::{MemoryStream, TcpMessageBus};

    for (name, script) in SCRIPTS {
        let stream = MemoryStream::default();
        let connection = Connection::stubbed(stream.clone(), 28);
        connection.set_server_version_for_test(server_versions::PROTOBUF_REST_MESSAGES_3);
        let bus = Arc::new(TcpMessageBus::new(connection));
        bus.process_messages().unwrap();
        let client = Client::stubbed(bus.clone(), server_versions::PROTOBUF_REST_MESSAGES_3);
        let contract = Contract::forex("EUR", "USD").build();
        let opens = AtomicUsize::new(0);
        let (items_send, items) = mpsc::channel();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                let open = || {
                    opens.fetch_add(1, Ordering::SeqCst);
                    client.historical_data(&contract, BarSize::Min).duration(1.days()).stream()
                };
                for item in client.resubscribing(open) {
                    if items_send.send(label(item)).is_err() {
                        break;
                    }
                }
            });

            // Ends the wrapper, also when a step panics, so the scope can join it.
            let _disconnect = Disconnect(&client);
            for step in script() {
                match step {
                    Step::Push(frame) => stream.push_inbound(frame),
                    Step::TransportLost => {
                        stream.push_hang_up();
                        for frame in handshake_frames(server_versions::PROTOBUF_REST_MESSAGES_3, "EST", 5000) {
                            stream.push_inbound(frame);
                        }
                    }
                    Step::Item(expected) => {
                        let item = items
                            .recv_timeout(STEP_TIMEOUT)
                            .unwrap_or_else(|_| panic!("{name}: no item, expected {expected}"));
                        assert_eq!(item, expected, "{name}");
                    }
                    Step::Opens(expected) => {
                        wait_for("open calls", || opens.load(Ordering::SeqCst) >= expected);
                        std::thread::sleep(SETTLE);
                        assert_eq!(opens.load(Ordering::SeqCst), expected, "{name}: open calls");
                    }
                }
            }
        });
        assert_eq!(items.try_iter().last().as_deref(), Some("Shutdown"), "{name}: ends on the disconnect");
    }
}

#[cfg(feature = "async")]
#[tokio::test]
async fn resubscribing_across_farm_outage_async() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::StreamExt;

    use crate::client::r#async::Client;
    use crate::connection::r#async::AsyncConnection;
    use crate::transport::r#async::{AsyncTcpMessageBus, MemoryStream};

    for (name, script) in SCRIPTS {
        let stream = MemoryStream::default();
        let connection = AsyncConnection::stubbed(stream.clone(), 28);
        connection.set_server_version_for_test(server_versions::PROTOBUF_REST_MESSAGES_3);
        let bus = Arc::new(AsyncTcpMessageBus::new(connection));
        bus.clone().process_messages().unwrap();
        let client = Client::stubbed(bus.clone(), server_versions::PROTOBUF_REST_MESSAGES_3);
        let contract = Contract::forex("EUR", "USD").build();
        let opens = AtomicUsize::new(0);

        let open = || {
            opens.fetch_add(1, Ordering::SeqCst);
            client.historical_data(&contract, BarSize::Min).duration(1.days()).stream()
        };
        let mut items = std::pin::pin!(client.resubscribing(open));

        for step in script() {
            match step {
                Step::Push(frame) => stream.push_inbound(frame),
                Step::TransportLost => {
                    stream.push_hang_up();
                    for frame in handshake_frames(server_versions::PROTOBUF_REST_MESSAGES_3, "EST", 5000) {
                        stream.push_inbound(frame);
                    }
                }
                Step::Item(expected) => {
                    let item = tokio::time::timeout(STEP_TIMEOUT, items.next())
                        .await
                        .unwrap_or_else(|_| panic!("{name}: no item, expected {expected}"))
                        .map(label);
                    assert_eq!(item.as_deref(), Some(expected), "{name}");
                }
                Step::Opens(expected) => {
                    // Polls the wrapper meanwhile, so it can call `open`.
                    let deadline = tokio::time::Instant::now() + STEP_TIMEOUT;
                    while opens.load(Ordering::SeqCst) < expected {
                        assert!(tokio::time::Instant::now() < deadline, "{name}: timed out waiting for open calls");
                        if let Ok(item) = tokio::time::timeout(Duration::from_millis(5), items.next()).await {
                            panic!("{name}: unexpected item while waiting for open: {:?}", item.map(label));
                        }
                    }
                    if let Ok(item) = tokio::time::timeout(SETTLE, items.next()).await {
                        panic!("{name}: unexpected item: {:?}", item.map(label));
                    }
                    assert_eq!(opens.load(Ordering::SeqCst), expected, "{name}: open calls");
                }
            }
        }
    }
}

/// Dropping the wrapper while it waits for the farms leaves no waiter behind.
#[cfg(feature = "async")]
#[tokio::test]
async fn resubscribing_dropped_during_farm_wait_async() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::StreamExt;

    use crate::client::r#async::Client;
    use crate::connection::r#async::AsyncConnection;
    use crate::transport::r#async::{AsyncMessageBus, AsyncTcpMessageBus, MemoryStream};

    let stream = MemoryStream::default();
    let connection = AsyncConnection::stubbed(stream.clone(), 28);
    connection.set_server_version_for_test(server_versions::PROTOBUF_REST_MESSAGES_3);
    let bus = Arc::new(AsyncTcpMessageBus::new(connection));
    bus.clone().process_messages().unwrap();
    let client = Client::stubbed(bus.clone(), server_versions::PROTOBUF_REST_MESSAGES_3);
    let contract = Contract::forex("EUR", "USD").build();
    let opens = AtomicUsize::new(0);

    let open = || {
        opens.fetch_add(1, Ordering::SeqCst);
        client.historical_data(&contract, BarSize::Min).duration(1.days()).stream()
    };
    let mut items = Box::pin(client.resubscribing(open));
    for step in broken_with_10182(&[CASHHMDS_BROKEN]) {
        match step {
            Step::Push(frame) => stream.push_inbound(frame),
            Step::Item(expected) => assert_eq!(items.next().await.map(label).as_deref(), Some(expected)),
            Step::Opens(_) | Step::TransportLost => {}
        }
    }
    assert!(tokio::time::timeout(SETTLE, items.next()).await.is_err(), "the wrapper should be waiting");
    assert_eq!(bus.farm_watch().waiters(), 1);

    drop(items);
    assert_eq!(bus.farm_watch().waiters(), 0);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}
