# Protocol wire facts

Connection, request ids, cancels and end markers. Format: [README](README.md).

### Duplicate client id (326)

*Live 2026-10-26, paper Gateway, server 226; #1091.*

- Connecting with a client id already in use: after `start_api`, TWS sends exactly one frame, a
  protobuf Error (msg id 4, request id -1, code 326, "Unable to connect as the client id is
  already in use. Retry with a unique client id."). No NextValidId or ManagedAccounts. Socket
  closes ~0.7s later.
- A clean drop of the holder frees the id immediately.
- Half-open socket (simulated with a TCP proxy that closed the client side and held the TWS
  side 10s): each reconnect attempt got 326 until TWS closed its side, then the next attempt
  reconnected. So 326 is transient on reconnect (retry) and fatal on the initial connect.

### Request id range

*Live 2026-09-11, paper Gateway, server 220; #789, #912 (`REQUEST_ID_FLOOR` /
`REQUEST_ID_CEILING` in `src/client/ids.rs`).*

- Data frames (ContractData / End) echo every id tried: 0, 9000, 1<<24, 1<<30, i32::MAX-1,
  i32::MAX, -2, -1000, i32::MIN+1, i32::MIN.
- Error frames (code 200) echo all of those **except i32::MAX and i32::MIN**: field 1 arrives
  absent, decodes as -1, and the error cannot be routed. An allocator must stop at i32::MAX-1.
- `-1` is IB's NO_VALID_ID sentinel; the C# client does no sign validation.

### Cancel after a request-bound terminal error

*Live 2026-11-24 off-hours, paper TWS, server 223, sync and async; first sweep 2026-10-26, paper
Gateway; #1094, #1097, #1096.*

TWS reply to a cancel sent after the request already failed. Observed while the type still sent
the cancel; types whose code is verified here now skip it, so the reply no longer occurs for them.

| Request | Terminal error | Cancel reply |
| --- | --- | --- |
| market_data (stream and snapshot) | 200, 354, 10168 | 300 |
| realtime_bars | 200, 354, 420 | 300 |
| tick_by_tick | 200, 10189 | 300 |
| market_depth | 200, 354 | 310 |
| market_depth | 10092 (no depth for the security type and exchange) | none |
| historical_data (stream / fetch) | 200, 162, 10314 | 366 "No historical data query found" |
| historical_data (subscribe_with_updates) | 162, 321 | 366 |
| pnl | 321 | 10185 |
| pnl_single | 321 | 10186 |
| scanner | 162 (disabled scan code) | 365 |
| contract_details, historical_ticks | 200 | none |
| historical_ticks | 10299 | none |
| account_summary | 321 | none |

- 354, 10168 and 10189 came from a CME future and a US stock without real-time permission. The
  same ES market data request drew 10168 in one run and 354 in the next, minutes apart.
- 420 on realtime bars (*live 2026-11-24 17:47-17:48 UTC, US market open, paper TWS, server
  223, market data types real-time and delayed*): IBM `TRADES` ("Invalid Real-time Query:No
  market data permissions for NYSE STK ..."), ESZ6 `TRADES` ("... for CME FUT") and EUR.USD
  `TRADES` ("... No historical market data for EUR/CASH@FXSUBPIP Last 0"). No bar followed for
  that request id, and the cancel the client sent drew 300 "Can't find EId with
  tickerId:1500000001" 1 to 2 ms later, under both types. The same session's EUR.USD `MIDPOINT`
  bars streamed. With the delayed type, ESZ6 drew 354 instead and MSFT drew 10089 under both
  types (420 at 16:42 UTC). Market data, tick-by-tick and keep-up-to-date requests on these
  contracts drew 10089, 354, 162 or 321, never 420.
- The reply has no recipient; the transport logs `no recipient for error` at INFO. It does not
  reach `notice_stream()`. Cost: one wasted write and one log line.
- A cancel that draws no reply proves nothing either way, so those types keep cancelling.
- `positions_multi` / `account_updates_multi`: TWS accepts a made-up account (DU0000000) with
  no error, so no terminal error could be provoked.
- 10314 came from a 1975 end date (IB rejects that old a date), not a format problem.
- Skipping the cancel is opt-in per verified code (`StreamDecoder::rejection_ends_request`). A
  global skip was rejected: a code we treat as terminal may not end the request at TWS (10299
  below), and skipping would leak TWS resources. 10197 does not end a market data request at
  all; see [Streams across an internet outage](market-data.md#streams-across-an-internet-outage).
- **Not probed:** 354 and 10168 on market data during RTH; the cancel reply after 10197 and after
  10182; market depth 354 on a stock.

### Cancel after a rejected keepUpToDate historical request

*Live 2026-11-25 09:57-10:01 UTC off-hours, paper TWS, server 223, sync.*

- Raw `reqHistoricalData` with `keepUpToDate` true, then `cancelHistoricalData` 1.5 s after the
  error, under the same request id. Every rejection below drew 366 "No historical data query
  found for ticker id:<id>" within 1 ms of the cancel, so TWS had ended the request:
  - 200 "No security definition has been found for the request" (ZZZZQX stock, and 005930 on
    KSE).
  - 162 "Historical Market Data Service error message:No market data permissions for IBIS STK"
    (SAP on IBIS, 1 h `TRADES` bars, 1 day).
  - 162 "Historical Market Data Service error message:invalid step: 1" (EUR.USD, 1 s
    `MIDPOINT` bars, 2 days).
  - 321 "Error validating request.-'D' : cause - End date not supported with live updates"
    (EUR.USD, end date set).
  - 321 "Error validating request.-'D' : cause - Source price not supported with live
    updates" (AAPL `ADJUSTED_LAST`; BTC `AGGTRADES`).
- The same requests with `keepUpToDate` false drew the same 200 and 162 errors, and the same
  366 to a cancel; with an end date or `ADJUSTED_LAST` / `AGGTRADES` they were served.
- Requests that streamed answered the cancel with 162 "API historical data query cancelled:
  <id>" instead: EUR.USD `MIDPOINT` (updates every ~5 s), and ESZ6 (CME) and 7203 (TSE)
  `TRADES`, whose bars came without a permission error and then no update or error for 12 to
  45 s.
- `HistoricalDataFrame::rejection_ends_request` matches 200, 162 and 321 by code under `keep_up_to_date`.
- **Not probed:** 354 on a historical request (no contract tried drew it); the cancel reply
  after 10182, which only the internet outage runs drew
  ([Streams across an internet outage](market-data.md#streams-across-an-internet-outage));
  pacing-violation 162, and RTH.

### Cancel after a request TWS finished

*Live 2026-11-24 off-hours, paper TWS, server 223.*

- Snapshot market data (EUR.USD, BTC) ends with TickSnapshotEnd; no cancel is sent after it.
- historical_data `subscribe()` (EUR.USD, ES) ends with HistoricalDataEnd; no cancel.
- historical_ticks (EUR.USD midpoint) ends on the batch flagged done; no cancel.
- contract_details (AAPL) ends with ContractDataEnd; no cancel.
- Still-live requests answer the cancel without error: streaming market data, tick-by-tick and
  market depth on EUR.USD, account_summary after its End (the stream stays open), pnl. A
  `stream()` historical request answers it with 162 "API historical data query cancelled".

### 10089 ends the request without an end marker

*Live 2026-11-24 off-hours, paper TWS, server 223, market data type real-time.*

- AAPL snapshot, AAPL streaming market data and AAPL realtime bars each drew 10089 "Requested
  market data requires additional subscription for API ... Delayed market data is available",
  then nothing for 12-15 s: no ticks, no TickSnapshotEnd. A cancel afterwards drew 300 every
  time, so TWS had already ended the request.
- The client ends the stream on 10089 and sends no cancel; the full sweep is in
  [market data type and permission notices](market-data.md#market-data-type-and-permission-notices)
  (#1297).
- With market data type delayed (3) or delayed-frozen (4), the same requests drew 10167 and
  delayed ticks; frozen (2) drew 10168.
- **Not probed:** RTH, and an account with delayed data enabled for the API.

### 10299 does not end a historical request

*Live 2026-11-24 off-hours, paper TWS, server 223.*

- historical_data TRADES on BTC (PAXOS crypto) drew 10299 "Expected what to show is AGGTRADES,
  please use that instead of TRADES", then TWS sent the bars and HistoricalDataEnd for the same
  request id anyway, about 180 ms later.
- With `keepUpToDate` (`stream()`), TWS sent the bars after the 10299 and kept sending updates
  every ~5 s. A cancel sent right after the 10299 drew 366 "No historical data query found",
  and the updates kept coming for the 37 s left in the session.
- Clients before #1298 ended the stream on the 10299, so those frames reached no recipient. Bar
  requests now yield it as a notice and read on
  ([10299 substitution](market-data.md#10299-whattoshow-substitution)); historical ticks still
  end on it.
- historical_ticks TRADES on BTC drew the same 10299 and no ticks afterwards.
- **Not probed:** whether a later cancel stops the updates.

### Cancel of a running historical bar request (162)

*Live 2026-11-05 off-hours, paper, server 226; #1167.*

- `historical_data(..).between(..).subscribe()` (keepUpToDate false, 300 days of hourly SPY
  bars, end date set), then `cancelHistoricalData` right after the request: within ~50 ms TWS
  sends Error 162 for the request id, "Historical Market Data Service error message:API
  historical data query cancelled: <id>". No bars and no HistoricalDataEnd. 3/3 runs, sync
  and async.
- So TWS confirms the cancel, and `cancel_and_drain` returns `Drained::Rejected` (162): the
  request is over at TWS. A cancel after the request already ended draws 366 instead (see
  [Cancel after a request-bound terminal error](#cancel-after-a-request-bound-terminal-error)).
- **Not probed:** a cancel that crosses bars already in flight, and RTH. Whether the 162 still
  follows when the bars were sent first is unverified.

### One-shot requests answer once

*Live 2026-10-26, paper Gateway, off-hours; #1107.*

- `calculate_option_price`, `calculate_implied_volatility`, `head_timestamp`,
  `histogram_data`, historical schedule: exactly one frame each, then nothing for 15s. Option
  computations do not stream.
- A cancel sent after the answer draws no error (option price, head timestamp, histogram):
  harmless, unneeded. The answer acts as a snapshot end.
- **Not probed:** WSH metadata (paper account gets 10276 "News feed is not allowed").

### End markers come after the initial dump only

*Live 2026-10-05, paper Gateway, single account, group "All"; PR #957 review. Diagnostics:
`integration/async/tests/end_markers.rs` (`#[ignore]`).*

- account_summary, account_updates, account_updates_multi, positions, positions_multi send End
  after the initial dump only. No End after fill-triggered pushes (~21s, ~61s) or the 3-minute
  periodic push (~241s). Never assume one End per push.
- **account_summary sends End twice** (group "All", 3/3 reproductions; recorder showed 1
  request, 2 AccountSummaryEnd frames on the wire, so not a routing bug):
  - regular tags only: rows, End, End
  - `$LEDGER:ALL` only: ledger block, End, the same ledger block, End
  - both: rows, ledger, End, ledger, End
  - One earlier 30s run showed 1 End, so not fully deterministic.
- account_summary pushes: 111 rows within one second; pushes are ≥40s apart. Pushes include
  unchanged values.
- **Open:** after `cancel().await` (cancel frame on the wire with the same id, as in C#), a
  third account_summary request on the same connection still got 322 (max 2) 10s later. A fresh
  connection per request works. Unknown whether TWS keeps the slot or the cancel is
  ineffective.

### Data-farm notices

*Live 2026-11-13 (session not recorded), paper TWS, server 223; live 2026-11-14 weekend, paper Gateway,
server 226, connect burst plus a 30-min session (Sat 15:24–15:54 ET) with a forex quote and
connectivity flaps; #1209 (`FARM_CODES`, `Notice::data_farm` in `src/messages/notice.rs`).*

- Every farm notice is global (request id -1) and carries the farm name in its text. All but
  the inactive notice put it after a `:` with no space:
  - connect burst, TWS server 223: `2104 Market data farm connection is OK:usopt`,
    `2104 ...OK:usfarm`, `2106 HMDS data farm connection is OK:ushmds`,
    `2158 Sec-def data farm connection is OK:secdefnj`.
  - connect burst, Gateway server 226: `2104 ...OK:usfuture` (earlier run only),
    `2104 ...OK:usfarm`, `2106 ...OK:ushmds`, `2158 ...OK:secdefnj`.
  - connecting, Gateway 226, on the first EUR.USD quote: `2119 Market data farm is
    connecting:cashfarm`, then `2104 ...OK:cashfarm` in the same second.
  - broken, Gateway 226, during weekend flaps: `2103 Market data farm connection is
    broken:usfarm`, `2105 HMDS data farm connection is broken:ushmds`, `2157 Sec-def data farm
    connection is broken:secdefnj`. Each flap ran broken → 1100 → OK codes → `1102 ...restored
    - data maintained. All data farms are connected: usfarm; ushmds; secdefnj.`; 1100 can come
    before or after the broken codes.
- Inactive uses a `.` instead: `2108 Market data farm connection is inactive but should be
  available upon demand.cashfarm`, about 2.5 min after the cashfarm quote was cancelled. It
  arrived as two identical frames (both in the raw capture, so TWS sends it twice).
  `Notice::data_farm` parsed every notice above to the right kind, status and name.
- TWS server 223 also sends the broken codes 2103, 2105 and 2157, seen in the 2026-11-25 outage
  runs (see [Internet outage: notices and timing](#internet-outage-notices-and-timing)).
- **Not probed:** 2107 (HMDS inactive): `ushmds` stayed up through 30 min idle after one
  head-timestamp request. Farms other than those named above.

### A TWS stall leaves historical, contract-detail and completed-order requests unanswered

*Live 2026-11-24 23:06 to 2026-11-25 06:19 UTC, paper TWS, server 223; ended by a TWS restart.*

| Request | During the stall | After the restart (06:21) |
| ------- | ---------------- | ------------------------- |
| `contract_details` OPT 700 SEHK | 23:06: no reply, no error in 170 s | 139 rows in 1 s |
| `completed_orders` (`api_only` true and false) | 23:09-23:10: nothing in 10 s, three times | 8 orders at once |
| `historical_data`, keep-up-to-date or not (700 SEHK, HSI future, EUR.USD) | from 00:55: no batch, no update, no error, on new connections too, checked 10 times until 06:19 | batch and updates within 1 s |
| `reqMktData`, `realtime_bars`, orders and cancels | answered throughout | answered |

- The N225 historical request at 23:30 still answered, so the historical service stopped
  between 23:30 and 00:55. At 00:56:40-00:57:51 the farm notices reported `usfuture`, `ushmds`,
  `cashhmds`, `usfarm`, `secdefnj`, `cashfarm`, `usopt` and `uscrypto` broken and then OK again;
  no notice marked the stall itself or its end.
- New connections took the handshake as usual, except some that timed out after 30 s ("no
  startup reply from TWS") when made seconds after another connection closed (22:04 twice,
  01:36, 04:05).
- A subscription opened during the stall is created normally and then stays silent, which on a
  keep-up-to-date stream looks like a session break, and a one-shot request without a timeout
  waits for good. Only a watchdog on the caller's side detects it.
- Whether long-held keep-up-to-date streams caused it is not settled: the probe's own streams
  (up to four at once, held 1.5 h, 21:41-23:20) had all ended by 00:40, before the historical
  service stopped, but other clients on the same TWS ran their own requests.
- **Not probed:** the cause; IB Gateway; whether a reconnect of every client, short of a restart,
  clears it.

### Internet outage: notices and timing

*Live 2026-11-25 12:40-13:08 UTC (US pre-market, FX open), paper TWS, server 223; Wi-Fi off for
15 s, 2 min and 6 min, four API clients connected; #1300.*

- The API socket stayed up in all three runs: no transport reconnect, no connection-lost error.
- Loss: 1100 and broken-farm notices (2103, 2105, 2157) for every connected farm; no 2110. 1100
  came 17.5 s (as Wi-Fi returned), 36.5 s and 131.5 s after the cut; the first broken notice
  17.5 s, 50.3 s and 44.3 s. In the 6-minute run broken notices came 87 s before the 1100.
- With the 1100, TWS sent 1104 "Pending to create 1 orders: <order id>" to the client that had
  placed an order during the outage.
- Some farms went to 2108 instead of, or after, broken. jfarm went broken, then 2108, and got no
  2104.
- Restore order, every run: in the 2- and 6-minute runs a second 1100 23.7 s and 31.2 s after the
  network returned; then OKs for every farm within 1.3 to 2.1 s, including farms never reported
  broken; then 1102, 0.7 to 2.1 s after the first OK and 6.4 s, 28.5 s and 35.4 s after the
  network. Both HMDS OKs (2106 `ushmds`, `cashhmds`) came before the 1102.
- The 1102 farm list named 11, 8 and 6 farms while 11, 12 and 12 sent an OK. Do not parse it.
- Farm notices arrived only with request id -1.
- Streaming requests sent during the outage got no frame and no error, for up to 354 s.
- `Client::resubscribing` waits for the HMDS OKs after a 10182 (`src/transport/farm_watch.rs`).
- `Client::resubscribing` reopens a stream on 1101, cancel first, on IB's documented rule alone.
- **Not probed:** 1101, so also whether TWS answers a cancel for a request opened before a 1101
  (with 300, or at all) and whether such a request resumes on its own; an outage that drops the
  API socket; a router uplink cut; a TWS daily restart; IB Gateway.

### Requests held during an internet outage

*Live 2026-11-25, paper TWS, server 223; same runs.*

- Historical bar requests sent during the outage were answered 0.4 to 0.5 s after the 1102
  (waits up to 146 s). Each reply ended at the minute the request was sent.
- `reqOpenOrders` sent 65 s in (after the 1100) was answered 82.6 s later. Before TWS noticed the
  outage it was answered at once.
- `reqPositions` sent 65 s in drew 2151 "Positions info is not available yet." with request id
  -1 and then nothing, also after the restore (open 323 s). Sent 0.1 s after the network
  returned, it got 4 positions and `PositionEnd` where other requests got 11 to 14.
- **Not probed:** contract details, completed orders.
