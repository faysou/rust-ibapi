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

*Live 2026-10-26, paper Gateway; #1094, #1097, #1096.*

TWS reply to a cancel sent after the request already failed. Observed while every type still
sent the safety-net cancel; types whose code is verified here now skip it, so the reply no
longer occurs for them.

| Request | Terminal error | Cancel reply |
|---|---|---|
| market_data, realtime_bars, tick_by_tick | 200 | 300 |
| market_depth | 200 | 310 |
| historical_data (stream / fetch) | 200, 162, 10314 | 366 "No historical data query found" |
| pnl | 321 | 10185 |
| pnl_single | 321 | 10186 |
| scanner | 162 (disabled scan code) | 365 |
| contract_details, historical_ticks | 200 | none |
| account_summary | 321 | none |

- The reply has no recipient; the transport logs `no recipient for error` at INFO. It does not
  reach `notice_stream()`. Cost: one wasted write and one log line.
- `positions_multi` / `account_updates_multi`: TWS accepts a made-up account (DU0000000) with
  no error, so no terminal error could be provoked.
- 10314 came from a 1975 end date (IB rejects that old a date), not a format problem.
- Skipping the cancel is opt-in per verified code (`StreamDecoder::rejection_ends_request`). A
  global skip was rejected: a code we treat as terminal may not end the request at TWS, and
  skipping would leak TWS resources. 10197 does not end a market data request at all; see
  [Streams across an internet outage](market-data.md#streams-across-an-internet-outage).
- **Not probed:** the cancel reply after 10197 and after 10182.

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
- **Not probed:** 1101; an outage that drops the API socket; a router uplink cut; a TWS daily
  restart; IB Gateway.

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
