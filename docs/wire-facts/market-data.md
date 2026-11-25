# Market data wire facts

Generic ticks, option computations, tick types and historical bars. Format: [README](README.md).

### ETF NAV generic ticks

*Live 2026-10-04 overnight, paper Gateway, server 220, SPY/QQQ; #946, #952. RTH re-run
2026-10-27, server 226.*

- `576` and `578` → 321 "Incorrect generic tick list". TWS's legal list for STK includes
  `577` (EtfNavLast), `614` (EtfNavMisc high/low), `623` (EtfFrozenNavLast), not 576/578.
- `577` → tick 96 `EtfNavLast` in RTH (SPY 769.72 vs last 769.75; QQQ 17 ticks in 45s, SPY
  1–4 per 20–60s). Nothing outside RTH.
- `614` → accepted, but no tick 98/99 in 60s SPY / 45s QQQ during RTH, no error.
- `623` → nothing in RTH (expected: frozen value, after close).
- **Not probed:** 623 → tick 97 after close with Frozen; 614 over a longer window (#946).

### Option computation volatility is a fraction

*Live 2026-10-27 RTH, paper Gateway, server 226, SPY 20261030 C770 @768.*

- `calculate_option_price`: 0.4 → $10.72, 0.1 → $2.12, 40.0 → $720 (4000% vol). So 0.4 means
  40%. Rustdoc previously said "percentage (e.g. 20.0)"; corrected.
- Same concept, different unit from VOL orders: [orders](orders.md#vol-order-volatility-is-percent).

### Option snapshot computations

*Live 2026-10-29 RTH, paper Gateway, server 226, AAPL 20261113 350C; #1121.*

- A snapshot with no generic ticks delivers all four computations: Bid, Ask, Last, Model.
  Model's `present_value_dividend` is None.
- STK snapshots fill every `Quote` field (12 ticks; bid/ask/last arrive as PriceSize), RTH and
  the overnight session alike (2026-10-29 23:55 ET: AAPL/SPY/IBKR, real bid/ask). No -1 seen.
- **Not probed:** a weekend (true no-quote) snapshot.

### No-quote sides

*Live 2026-11-23 Mon 18:47-19:00 ET (US equities and options closed, FX and CME Globex open,
CBOT grains closed), paper TWS, server 223. Real-time data only for EUR.USD and XAUUSD; US
stocks drew 10089 for every market data type, CME and CBOT 354 / 10167 (delayed only).*

`reqMktData`, streaming and snapshot. A side with no quote arrives as a `TickPrice` with a size
of `"0"`:

| Contract                               | Data type               | Empty side (price, size) |
| -------------------------------------- | ----------------------- | ------------------------ |
| AAPL options, far strikes              | delayed                 | `-1`, `0`                |
| SPX                                    | delayed                 | `-1`, `0` (bid and ask)  |
| CBOT corn options (OZC, magnifier 100) | delayed                 | `-100`, `0`              |
| AAPL put vertical (BAG)                | delayed, delayed-frozen | `0`, `0`                 |

- The last price uses the same placeholder: `-1` / `-100` with size `0` where nothing traded.
  EUR.USD sends a real last price with size `0`, so a zero size alone does not mean "no last".
- Delayed-frozen on the AAPL 110 put: first `-1` / `0` for bid and ask, then
  `MarketDataType::DelayedFrozen` and a real ask (`0.01` x `5`) with no new bid tick, so the bid
  stays empty. The 425 put got a real frozen bid and ask.
- Negative prices with sizes are real: an ES Dec/Mar calendar (BAG, delayed) quoted `-71` x `1` /
  `-68.75` x `2`, and the reverse spread `68.75` / `71`.
- Every bid, ask and last price tick carried its size. No price tick arrived without one.
- Historical bid/ask ticks: an AAPL 110 put (2026-11-23 20:55-21:00 UTC) sent all 1009 ticks with
  bid `0` x `0` and a real ask. AAPL (2166) and a corn option (1001) had no zero or negative
  value; no tick carried `-1`.
- Tick-by-tick bid/ask on EUR.USD (11 frames): real prices and sizes. AAPL drew 10089, ES 10189;
  EUR.USD `last` drew 10189 ("No historical market data ... Last").
- Price tick attributes: none of 170 delayed price ticks (ES, ZC, OZC, SPX, US stocks and options,
  BAGs) set any flag, `can_auto_execute` included. Real-time EUR.USD bid and ask set
  `can_auto_execute` (10 of 12 ticks; XAUUSD did not); `pre_open` was never set.
- Settles that the size decides: every placeholder price came with size `0`, and every real
  quote, negative ones included, with a positive size.
- **Not probed:** `pre_open` during a real-time pre-open or halt (no such data on this account;
  delayed ticks carry no attributes); empty sides in tick-by-tick bid/ask or last (no real-time
  data on an illiquid contract); RTH equity and option quotes.

### TickEFP is never sent

*Live 2026-05-27, paper Gateway, server 220+; removed in v3.0 (#651).*

- AAPL, SPY, IBKR, SAP@IBIS, SIE@IBIS for ~5s each: 251 frames, all proto-framed, zero msg id
  47. The Eurex single-stock futures underlyings, the likeliest EFP triggers, returned other
  ticks normally.
- C# has no `TickEFPEventProtoBuf` handler, no `TickEFP.proto`, no version constant: EFP is
  text-only by design. Reintroduce only if those appear upstream.

### Historical schedule honors useRTH

*Live 2026-11-03 off-hours, paper, server 226; #1147.*

- `reqHistoricalData` with `whatToShow=SCHEDULE`, 5 days, `useRTH=1` vs `useRTH=0`:
  - AAPL (STK): RTH 09:30–16:00 ET; extended 04:00–20:00 ET. Five sessions each.
  - ES (FUT, CME, next quarter): RTH 08:30–16:00 CT; extended 17:00 (prior day) – 16:00 CT.
- Extended futures sessions are keyed by trade date: the session dated 11-02 starts Sunday
  11-01 17:00. The window also shifts forward one trade date (the session underway counts),
  so `reference` differs from the start date and the RTH and extended lists are not
  index-aligned.
- Settles that the flag changes the result; the C# client passes it through unchanged.
- **Not probed:** holidays / early closes, non-US exchanges, `TradingHours` on other
  security types (options, FX, crypto).

### CFD reroute ends the request on the TWS side

*Live 2026-11-10 RTH, paper Gateway, server 226, SMART CFDs AAPL/BMW/IBUS500/IBDE40; #1197.*

- Stock CFDs (AAPL con 120549942, BMW con 102284037): `reqMktData` and `reqMktDepth` (5
  rows) each draw exactly one frame, `RerouteMktDataReq` / `RerouteMktDepthReq`, naming the
  underlying's stock contract (AAPL 265598, BMW 14094) and exchange `SMART`. After it
  comes nothing: no ticks, no error, for 15s.
- TWS has already ended the CFD request. `cancelMktData` on the rerouted id draws 300
  "Can't find EId with tickerId". `cancelMktDepth` draws nothing, so the depth side was
  checked by slots instead: with six rerouted depth requests left uncancelled, a SMART-depth
  stock request is still accepted. The control, real depth requests left uncancelled,
  draws 309 "Max number (3) of market depth requests" on the third.
- Index CFDs (IBUS500, IBDE40) are not rerouted: `reqMktData` draws 354 (not subscribed)
  and `reqMktDepth` draws 10092 (deep data not supported for the security type/exchange).
- **Not probed:** delayed market data (type 3/4) on a CFD; snapshot requests; FX or
  commodity CFDs; a window longer than 15s; the async client.

### Bar volume and WAP on non-TRADES bars

*Live 2026-11-23 Mon 18:55-19:05 ET (FX, crypto and CME Globex open, US equities after hours),
paper TWS, server 223; raw capture.*

- `reqRealTimeBars` MIDPOINT, BID and ASK, EUR.USD, XAUUSD and BTC (PAXOS): `volume` (7) and
  `WAP` (8) **sent** as `"-1"` and `count` as `-1` in every frame. TRADES and AGGTRADES on BTC
  send real values, and a quiet bar sends `"0"` volume with count `0`.
- `reqHistoricalData` MIDPOINT, BID, ASK and BID_ASK, 1-hour bars (AAPL, ES, EUR.USD, an AAPL
  option, a CBOT corn option, the IBUS500 CFD, XAUUSD, BTC) and daily bars (AAPL, ES,
  EUR.USD): `volume` and `WAP` sent as `"-1"` and `barCount` as `-1`. TRADES and AGGTRADES
  send real values. An SPX TRADES bar sends `volume` and `WAP` as `"0"` with a real `barCount`.
- The corn option (OZC, price magnifier 100) sends the non-TRADES `WAP` as `"-100"`, `-1` times
  the magnifier. `volume` and `barCount` stay `-1`.
- `keepUpToDate` updates (`HistoricalDataUpdate`), MIDPOINT and ASK on EUR.USD, MIDPOINT on
  BTC, ASK on XAUUSD: `volume` and `WAP` sent as `"-1"` and `barCount` as `-1`, as in the
  initial bars.
- Earlier result (2026-11-15, paper Gateway, server 226): realtime MIDPOINT bars on EUR.USD and
  ES left `volume` and `WAP` **out** and sent `count` as `-1`; historical bars sent `-1`. So a
  realtime bar can carry either form.
- Settles that a non-TRADES bar's volume, WAP and count are a `-1` sentinel or absent on both bar
  types, and that the WAP sentinel scales with the price magnifier.
- **Not probed:** TRADES `keepUpToDate` updates (no real-time data on this account for stocks or
  CME; with `keepUpToDate`, BTC draws 10299 for TRADES and 321 "Source price not supported with
  live updates" for AGGTRADES); realtime bars on a
  magnified contract; a quiet historical TRADES bar.

### One-second bars reach back at most 2000 S

*Live 2026-11-23 21:21-21:32Z (after the US close; the ES runs during the 21:15-21:30Z CME halt
and after it), paper TWS, server 223. TRADES, regular hours.*

- AAPL, ending 2026-11-23 21:00 UTC: 1-second bars over 1800 S return 1800 bars and over 2000 S
  2000 bars (20:26:40 to 20:59:59 UTC). 2001, 2100, 2200, 2300, 2399, 2400 and 3600 S each draw
  162 "Historical Market Data Service error message:invalid step: 1", about 0.2 s after the
  request.
- 5-second bars: 3600 S returns 720 bars (0.4 s), 14400 S 2880 (0.6 s), 28800 S 5760 (11.4 s).
  Durations count trading seconds: the 28800 S request starts at 2026-11-20 19:30 UTC, the
  Friday session, while its reported window start (08:00 ET on the 23rd) is wall clock.
- ESZ6 (CME, no end date), halted and open alike: 1-second bars over 1800 and 2000 S succeed,
  2400 and 3600 S draw the same 162; 5-second bars succeed at 3600, 14400 and 28800 S. Bar
  counts fall short of the duration (1192 bars over 1800 S with CME open); the probe did not
  establish why. 28800 S took 54 s on both runs, and its reported window ends about 11 s
  after the previous request finished, so it was sent late or re-sent; not diagnosed.
- Settles the boundary: 2000 S passes and 2001 S fails for 1-second bars (bisected on AAPL;
  ESZ6 agrees at 2000 and 2400 S). Inferred: the limit is on the step count (duration over
  bar size), not on bars returned, since 5760 five-second bars passed.
- **Not probed:** 5-second bars above 28800 S; other bar sizes' limits; ES in the
  22:00-23:00Z closure; the Gateway.

### Weekly and monthly bars are dated at the end of the period

*Live 2026-11-23 21:20Z (Monday, after the US close), paper TWS, server 223; AAPL TRADES, 3 M
ending now, regular hours; wire dates read from a recording.*

- Each bar carries a plain `YYYYMMDD` (decoded as `BarTimestamp::Date`).
- Weekly (14 bars): `20260828`, `20260904`, ... `20261120`, every complete week dated its
  Friday, then `20261123` for the current week.
- Monthly (4 bars): `20260831` (Mon), `20260930` (Wed), `20261030` (Fri; 31 Oct 2026 is a
  Saturday), then `20261123` for the current month.
- The current, incomplete week and month are dated today, the last trading day so far. The
  first weekly bar is cut by the window start (Sun 23 Aug) and still dated its end, 28 Aug.
- Intraday bars are stamped with their start (1-second bars over 20:30:00-21:00:00 UTC run
  from 20:30:00 to 20:59:59; [above](#one-second-bars-reach-back-at-most-2000-s)), and the
  `between` rustdoc says bars are stamped with their start time. Weekly and monthly bars are
  stamped with the end of the period instead.
- **Not probed:** daily bars in this run; a holiday on a period's last weekday, so the last
  trading day is not separated from the last weekday; other instruments; the Gateway.

### Historical bar durations start at 30 S

*Live 2026-11-23 22:21-22:23Z (Monday, after the US close), paper TWS, server 223. AAPL TRADES,
regular hours.*

- 1-second bars ending 21:00 and 19:00 UTC: 1, 10 and 29 S each draw 321 "Error validating
  request.-'D' : cause - Historical data requested duration is invalid". 30, 59 and 60 S return
  30, 59 and 60 bars, the last one stamped one second before the end date.
- The same 321 for 29 S of 5-second, 1-minute and 1-hour bars. 30 S returns 6 five-second bars
  and one 1-minute bar.
- Settles that the minimum is 30 S for every bar size probed, so `between` asks for at least
  30 S.
- **Not probed:** 30 S of 1-hour bars; a minimum for `D`, `W`, `M` or `Y`; other instruments;
  the Gateway.

### Historical bar requests count back across closed hours

*Live 2026-11-23 22:22Z (Monday, after the US close), paper TWS, server 223. AAPL 1-second
TRADES, 2000 S.*

- Regular hours, ending after the close (23:00 UTC): 2000 bars, 20:26:40 to 20:59:59 UTC that
  day. Ending at Monday's open (14:30 UTC): Friday 2026-11-20 20:26:40 to 20:59:59. Ending at
  14:40 UTC: Friday 20:36:40 to Monday 14:39:59, 1400 seconds of Friday and 600 of Monday.
- A request ending at the earliest bar of the 14:40 reply (Friday 20:36:40) returns 20:03:20
  to 20:36:39: the end second is excluded and the start second included, so the two replies
  meet without a repeated or missing bar.
- Extended hours, ending Monday 06:00 and 09:00 UTC (in the overnight closure and at the 04:00 ET
  open): both return Saturday 00:26:40 to 00:59:59 UTC, the end of Friday's 20:00 ET session.
- The reported window is wall clock (08:56:40 to 09:30 ET for the request ending at the open),
  so it does not say where the bars start.
- Eight requests chained this way, sent back to back from 21:00 UTC, took 3.1 s (six within
  2.1 s); each returned 2000 bars with no pacing error.
- Settles how `fetch` splits a 1-second `between` range: each request ends at the earliest bar
  of the reply before, and the replies tile the range. Ends computed in wall-clock time would
  repeat bars across a closure. The 2000 S limit itself is in "One-second bars reach back at
  most 2000 S" (#1264).
- **Not probed:** a reply with no bars inside a session (an illiquid contract); futures
  sessions; holidays; more than eight requests in a row; the Gateway.

### Second-sized bar limits from 5 to 30 seconds

*Live 2026-11-23 22:23-22:27Z (Monday, after the US close), paper TWS, server 223. AAPL TRADES,
regular hours, ending 21:00 UTC.*

- 86400 S: 5-second bars 17280 (27.7 s), 10-second 8640 (14.7 s), 15-second 5760 (14.7 s),
  30-second 2880 (0.6 s). Each count is 86400 S over the bar size, so the duration counts
  trading seconds and reaches back to 2026-11-18 16:30 UTC.
- 2 D: 5-second bars 9360 (26.3 s), 10-second 4680, 15-second 3120, 30-second 1560, each from
  Friday's open to Monday's close.
- 10 D of 5-second bars returned nothing within 120 s; the probe cancelled it.
- Settles that these sizes take any `S` duration `between` asks for (up to 86400 S, a range of
  one day), so only 1-second bars are split.
- **Not probed:** where these sizes' limits lie, including the 3 D and longer that `between`
  asks for over a range of more than one day; whether the 10 D request ends in data or an
  error; other instruments; the Gateway.

### Trade-price `whatToShow` by security type

*Live 2026-11-23 Mon 18:55-19:05 ET (FX, crypto and CME Globex open, US equities after hours),
paper TWS, server 223. Contracts: AAPL, ES Dec 26, EUR.USD, SPX, an AAPL option, a CBOT corn
option (OZC), the IBUS500 index CFD, XAUUSD, BTC on PAXOS.*

Accepted trade-price `whatToShow`, or the error a rejected one drew:

| Security type      | Historical bars                      | Realtime bars       | Historical ticks   |
| ------------------ | ------------------------------------ | ------------------- | ------------------ |
| STK, FUT, OPT, FOP | TRADES                               | not probed          | TRADES             |
| IND (SPX)          | TRADES (volume `0`)                  | not probed          | TRADES             |
| CASH (EUR.USD)     | none: TRADES 162                     | none: TRADES 420    | none: see below    |
| CMDTY (XAUUSD)     | none: TRADES 162                     | none: TRADES 420    | none: no answer    |
| CFD (IBUS500)      | none: TRADES 162                     | not probed          | none: no answer    |
| CRYPTO (BTC)       | AGGTRADES; TRADES after notice 10299 | AGGTRADES or TRADES | none: TRADES 10299 |

- AGGTRADES drew 10299 on historical bars for every type except crypto, and 420 on realtime bars
  for EUR.USD and XAUUSD. After that 10299, AAPL and ES historical bars arrive anyway as the
  TRADES series; EUR.USD draws 162 (2026-11-24, #1298). "Not probed" realtime cells drew 10089, 354 or 420 for missing
  real-time permission. SPX drew 10187 for BID_ASK and MIDPOINT historical ticks.
- MIDPOINT, BID and ASK bars worked for every type except IND (162). ADJUSTED_LAST follows TRADES.
  HISTORICAL_VOLATILITY and OPTION_IMPLIED_VOLATILITY drew 162 except OPTION_IMPLIED_VOLATILITY
  on AAPL and SPX.
- 10299 names the expected value: "Expected what to show is AGGTRADES, please use that instead
  of TRADES" for BTC, and the reverse for the other types. 162 reads "No historical market data
  for EUR/CASH@FXSUBPIP Last 3600" (XAUUSD: `CMDTYBBO`, IBUS500: `UKBBO`).
- Realtime TRADES and AGGTRADES on BTC sent identical bars for five 5-second intervals in a row,
  so the realtime 10299 for crypto TRADES that the `AggTrades` rustdoc stated is not what this
  server does.
- 10299 on crypto TRADES bars is a substitution notice, not a rejection: TWS then sends the bars
  and `HistoricalDataEnd`, identical to an AGGTRADES request (BTC and ETH, 2026-11-24). It is a
  rejection only for historical ticks, where nothing follows it. Clients before #1298 ended the
  request on the notice and dropped the bars, which is what the first run here recorded.
- With `keepUpToDate`, BTC and ETH TRADES draw 10299, then the bars, `HistoricalDataEnd` and live
  updates; a later cancel draws 162. AGGTRADES draws 321 "Source price not supported with live
  updates" and nothing else, so TRADES is the only way to stream crypto trade bars. AAPL, ES and
  EUR.USD AGGTRADES with `keepUpToDate` draw 10299, then 321 (2026-11-24, #1298).
- Historical ticks TRADES on EUR.USD: TWS answers with one `HistoricalTicks` frame (midpoint
  ticks, size `"0"`, `done`), not `HistoricalTicksLast`. `historical_ticks(..).last()` does not
  read it and waits until its deadline. XAUUSD and IBUS500 drew no ticks and no error in 30 s, and were not
  captured raw.
- A stock CFD (AAPL, rerouted on `reqMktData`) drew no answer to TRADES or MIDPOINT historical
  bars within 40 s; realtime bars drew notice 2127 and no bars.
- The account has crypto market data (BTC on PAXOS) though it has no crypto trading permission.
- **Streaming requests during US hours** (*live 2026-11-24 16:41-16:43 UTC, paper TWS, server
  223; BTC on PAXOS and MSFT*): BTC realtime bars stream for `TRADES`, `AGGTRADES` and
  `MIDPOINT`; keep-up-to-date `TRADES` draws 10299, then the batch, `HistoricalDataEnd` and an
  update every 5 seconds, and `AGGTRADES` draws 321 alone. Tick-by-tick `Last`, `AllLast` and
  `BidAsk` stream BTC; tick-by-tick takes no `whatToShow`. MSFT, without real-time permission:
  realtime `TRADES` bars draw 420 ("No market data permissions for ISLAND STK"), `AGGTRADES`,
  `MIDPOINT` and every tick-by-tick type draw 10089, keep-up-to-date `TRADES` draws 162 ("HMDS
  query returned no data"), and `AGGTRADES` draws 10299 and then 321.
- **Not probed:** realtime bars for STK, FUT, OPT, FOP, IND and CFD with real-time permission;
  BOND, WAR, FUND, IOPT, EC, BAG; crypto on ZEROHASH; live accounts.

### Keep-up-to-date bar updates

*Live 2026-11-24 Tue 09:15-09:35 UTC (FX, crypto and Xetra open; US equities overnight), paper,
server 223.*

- `reqHistoricalData` with `keepUpToDate`, EUR.USD MIDPOINT (5-second, 30-second, 1-minute and
  5-minute bars), BTC PAXOS MIDPOINT and SAP IBIS TRADES (1-minute bars): `HistoricalDataUpdate`
  arrives on a 5-second grid of the wall clock, 0.05 to 1 s after each boundary (one outlier at
  4.3 s). Each carries the bar that holds the 5 seconds just ended, sent even when nothing
  changed: 16 identical consecutive updates in 3 minutes of SAP, 10 in 5 minutes of EUR.USD.
- A 1-minute bar gets 12 updates. The 12th arrives 0.3 to 2.6 s after the bar ends and carries
  its final values; the next bar's first update arrives 5.3 to 6.1 s after that bar starts. A
  5-second bar gets one update, 5.3 to 9.3 s after it starts, so every update is already final.
- The initial batch sometimes ends with the forming bar (1-minute at 34 s into the bar, 5-minute
  at 2 min in), and the first update repeats or revises it. Other times it ends with the last
  complete bar (30-second at 6 s into the bar), and the first update is for the next bar. Once,
  with 5-second bars, the bar between the batch and the first update was never sent: the batch
  ended at 09:16:25, the first update was for 09:16:35.
- No update carried an earlier bar than the one before it (700+ updates).
- MIDPOINT updates send `volume`, `WAP` and `barCount` as `-1`, like the batch. TRADES updates
  send real values, but a bar's first update can report `barCount` 0 with volume 305.
- Refused or silent: crypto `AGGTRADES` and `BID_ASK` draw 321 "Source price not supported with
  live updates"; crypto `TRADES` draws 10299 (use AGGTRADES). Without real-time permission (ES,
  DAX and ESTX50 indices) and for AAPL and NVDA overnight, TWS sends the batch and then no update
  and no error.
- **Batch, then nothing** (*11:23 UTC, ES Dec 26 TRADES 1-minute, no real-time permission*): the
  batch's 30 bars ended with the 21:59 UTC bar of the previous day, the last before CME's
  daily break, followed by the batch end and no update in 150 s. `BarCompletion::apply` left
  that bar forming; `BarCompletion::expire` with a 10 s grace completed it at once. EUR.USD
  MIDPOINT on the same connection: updates completed each bar 0.3 to 0.5 s after its end, and
  `expire` with a 10 s grace never fired.
- Settles `historical::BarCompletion`: a bar is final when
  an update for it arrives after its end, or when an update for a later bar arrives.
- **Not probed:** RTH-only requests; daily and longer bars; BID, ASK and other MIDPOINT
  contracts. Session ends: see
  [keep-up-to-date bars at session boundaries](#keep-up-to-date-bars-at-session-boundaries).

### Keep-up-to-date bars at session boundaries

*Live 2026-11-24 16:24-16:49 UTC (Xetra close), 21:41-22:35 UTC (FX daily break, CME close),
paper TWS, server 223; 1-minute bars.*

- **Xetra close, SAP SMART TRADES** (primary IBIS, continuous trading ends 16:30 UTC, closing
  auction after it): updates kept coming every 5 seconds through the end of the run at 16:49,
  because SMART includes German venues that trade until 21:00 UTC. The 16:30 to 16:33 bars
  carried volume 0 and `barCount` 0 at the last price, updated 12 times each. The closing
  auction reached the 16:35 bar at 16:35:25 as volume 1,053,001 with `barCount` 0; later
  updates of that bar moved its open from 189.42 to 190.1.
- The full-day historical request for the same contract afterwards returned the same zero-volume
  bars (168 of 930 bars between 06:30 and 21:59 had volume 0: 16:30 to 16:33, 16:37 and 16:42 around
  the close), so they are TWS's bars, not artifacts of the stream. Several carried a new price with
  volume 0 (16:37: 190.08 after 189.94).
- The last update of each streamed bar matched that bar in the later historical reply on open,
  high, low, close and volume, for all 26 bars from 16:23 to 16:48. WAP matched to the three
  decimals the historical reply sends, except the auction bar: streamed 189.4236, historical
  190.1.
- **FX daily break, EUR.USD MIDPOINT**: a stream opened at 22:01:57 got a batch ending with the
  21:59 bar, then no update and no notice until 22:15:06, when the first update carried the
  22:15 bar. No bar for 22:00 to 22:14 was ever sent, neither in the batch nor later. The stream
  survived the break; `BarCompletion` completed the 22:15 bar at 22:16:00.8 from its last
  update.
- **Farm disconnect ends the stream**: at 21:56:59 notice 2105 "HMDS data farm connection is
  broken:cashhmds" was followed on the EUR.USD stream (and on a second client's EUR.USD stream)
  by error 10182 "Failed to request live updates (disconnected)." No update followed it. Reuse
  the `BarCompletion` across the new subscription.
- **No real-time permission, CME close**: ES and MES TRADES streams, with and without
  `switch_market_data_type(Delayed)`, got a batch whose last bar was 10 to 11 minutes old
  (21:35 at 21:46) and no update through the 22:00 close and the break. `BarCompletion::expire`
  completed the batch's last bar one second after the batch.
- **Historical service silent** (*2026-11-25 00:55-06:19 UTC, until a TWS restart*): every
  historical request, keep-up-to-date or not, on 700 SEHK, the HSI future and EUR.USD, got no
  batch, no update and no error, while `reqMktData` and realtime bars on the same connections
  kept working. A stream gives no sign of this: it looks like a session break.
  Contract-detail and completed-order requests went unanswered in the same stall.
- **SEHK close, 700 TRADES** (*2026-11-25 07:45-08:11 UTC*; continuous trading ends 08:00,
  closing auction to 08:10): the 07:59 bar got its last update at 08:00:01.2 and
  `BarCompletion::apply` completed it then. Bars for 08:00 to 08:09 followed through the auction,
  updated every 5 seconds at volume 0 and the last price; the auction print came in the 08:08
  bar's first update (08:08:06, volume 1,521,600, `barCount` 1, 337.4). The 08:09 bar's last
  update at 08:10:01.2 was the last frame: nothing followed to 08:11. Realtime 5-second bars on
  the same contract showed the same, ending with the 08:09:55 bar at 08:10:01.2.
- **Not probed:** the last bar before the FX daily break (the farm disconnect ended that stream
  first) and before the SEHK and HKFE lunch break (historical service silent); RTH-only
  requests at a close.

### Historical bars for a continuous future take no end date

*Live 2026-11-16 Mon 02:20 ET (CME Globex open), paper, server 223.*

- `reqHistoricalData` on ES `CONTFUT`, 1-hour TRADES bars: a 2-day duration with no end date
  returns 16 bars (2026-11-12 14:30 to 2026-11-13 21:00 UTC). The same request with an end date
  of 2026-11-13 21:00 UTC returns error 10339, "Setting end date/time for continuous future
  security type is not allowed". `between` sent an end date, so it got 10339 too.
- Settles that `between` on a continuous future must omit the end date and ask for a duration
  reaching back to `start` from now.
- **Not probed:** `keepUpToDate` streaming; CONTFUT on another exchange.

### Continuous future: schedule rejects an end date, ticks accept one; bar depth and cost

*Live 2026-11-22 Sun 19:56 ET (CME Globex open), paper, server 226. ES `CONTFUT`, extended
hours, TRADES.*

- `reqHistoricalData` with `whatToShow` SCHEDULE, 5 D: with an end date of 2026-11-13 20:00 UTC,
  error 10339, the same text as for bars. With no end date, 5 sessions.
- `reqHistoricalTicks`, 100 trade ticks: with an end date of 2026-11-13 20:00 UTC, 114 ticks
  ending 19:59:52; with a start date of 15:00 UTC, 128 ticks from 15:00:00. No 10339 either way.
- Bars with no end date: the bar-size step limit still applies. 1-second bars over 1800 S
  return 1800 bars; over 1 D they draw 162 "invalid step: 1". The boundary is 2000 S
  ([one-second bars](#one-second-bars-reach-back-at-most-2000-s)).
- Longer durations are accepted but slow. 1 M of 30-second bars took 74 s (55433 bars), 1 M of
  1-minute bars 73 s (27720), 1 Y of hourly bars 41 s (5896), 5 Y of hourly bars 122 s
  (16063). 2 M of 30-second bars and 1 Y of 1-minute bars did not finish within 90 s.
- History is shallower than asked: 5 Y of hourly bars starts 2023-09-27, and 20 Y of daily bars
  returns 1391 bars from 2021-06-07.
- Settles that schedule's `ending()` needs the same up-front rejection as bars and ticks'
  does not, and that `between` on a continuous future pays for `now - start` in wall-clock
  time rather than drawing an error.
- **Not probed:** whether the 90 s timeouts end in data or an error; bid/ask and midpoint
  ticks; daily bars over 5 Y or less.

### Historical tick pages: whole seconds, but one tick of a quote page's earliest second

*Live 2026-11-13 Fri near the US cash open (ES Z6, trades) and 2026-11-23 Mon 04:30 ET (ES
Z6, all three types, extended hours), paper TWS, server 223.*

- The end is read to the whole second and its second is excluded: ending at 14:30:00 returns
  trades up to 14:29:59, and ending at 14:28:35.5 returns the same ticks as 14:28:35. Ending
  at 09:17:01.5 and at 09:17:01 return the same bid/ask, midpoint and trade ticks.
- A trade page holds whole seconds, so it can exceed the count asked for: 100 trades ending
  09:17:01 returned 115, the earliest second (09:16:36) holding all 21 of its trades.
- A bid/ask or midpoint page holds whole seconds after its earliest second, but only one tick of
  that second. In all 12 quote pages probed (2 ends, 100 and 1000 ticks, both types) the
  earliest second held 1 tick; the same seconds hold 4 to 95 when they are not the earliest.
- So a page that ends where the previous page's earliest second begins loses that second's
  quotes but no trades. Over 09:16:46.5 to 09:18:46.5, paging that way returned 4345 bid/ask
  ticks with 100-tick pages and 5852 with 1000-tick pages; refetching the earliest second
  returned 6185 with both, and the same 511 trades and 992 midpoints with both.
- A stock pages the same way: AAPL trades over 2026-11-23 15:00 to 15:02 UTC (RTH) returned
  the same 5845 trades with 100-tick and 1000-tick pages.
- A request ending in a closed market returns the ticks before it: AAPL trades over Saturday
  2026-11-21 15:00 to 15:02 UTC (RTH) paged to `start` with no ticks in the range. A request
  ending before TWS's tick history returns no ticks: AAPL trades over 2005-01-03 15:00 to
  15:02 UTC (extended hours) got an empty first page. *Live 2026-11-23, paper TWS.*
- Settles the paging rules of `HistoricalTicksBuilder::between` and `latest`, and when they
  report `PagingStop::ReachedStart` or `PagingStop::EmptyPage`.
- **Not probed:** stock bid/ask pages, `IgnoreSize::Yes` pages, whether the single tick is the
  first or the last of its second.

### Continuous future 1-second bars: the 2000 S limit counts from now

*Live 2026-11-23 21:22Z (CME halt) and 21:31Z (CME open), paper TWS, server 223; ES `CONTFUT`,
1-second TRADES, `between`.*

- `between` sends no end date for a continuous future and asks for `now - start`, rounded up
  to a whole second ([no end date](#historical-bars-for-a-continuous-future-take-no-end-date)).
- `[now - 3000 s, now - 2500 s)` draws 162 "invalid step: 1" on both runs, although the range
  is 500 s.
- `[now - 1500 s, now - 1000 s)` returns exactly 500 bars on both runs (21:06:07 to 21:14:26
  UTC with CME open).
- Settles that the [2000 S limit](#one-second-bars-reach-back-at-most-2000-s) applies to
  `now - start`: a 1-second `between` on a continuous future must start no more than about
  2000 s before now, however short the range.
- **Not probed:** ranges starting between 2000 and 2500 s back; other bar sizes.

### Size fields are always sent

*Live 2026-11-15 Sun 22:25 ET, paper, server 226; raw capture, same session as above.*

- Every frame carried its size: `TickSize.size` (35 frames, tick types 0/3/5/8; `"0"` sent
  explicitly twice), tick-by-tick Last (ES, 9) and BidAsk (EUR.USD and ES, 40) sizes, and
  `MarketDepth` size (EUR.USD and ES, 80, insert and update). `TickReqParams.minTick` and
  `snapshotPermissions` were sent for all three contracts (`0` explicitly for EUR.USD).
- So decoding an absent size as `0` changes nothing observed; these stay `f64`.
- **Not probed:** `MarketDepthL2` (AAPL depth drew 10092); RTH equity ticks; AAPL
  tick-by-tick (no data overnight).

### `TickReqParams` precision fields

*Live 2026-11-15 Sun 22:25 ET, paper, server 226; raw capture; #1251.*

- `reqMktData` first frame: `lastPricePrecision` and `lastSizePrecision` sent as `"0.000001"`
  for AAPL, absent for EUR.USD and ES. Steps, not digit counts; the same values
  `ContractData` carries ([contracts](contracts.md#event-contract-minalgosize-and-precision-fields)).
- Not decoded: one stock out of three contracts, with no documented meaning.
- **Not probed:** options, crypto, non-US stocks.

### Historical ticks of another kind

*Live 2026-11-24 00:46Z (Monday 19:46 ET, off-hours), paper TWS, server 223; raw capture.*

- `reqHistoricalTicks` TRADES, 50 ticks ending 2026-11-23 20:00 UTC, for EUR.USD (IDEALPRO),
  XAUUSD (CMDTY, SMART) and the IBUS500 CFD: one `HistoricalTick` (midpoint) frame each, with
  `isDone` true, then nothing for 20 s. No `HistoricalTickLast` frame and no error. EUR.USD
  sent the same 56 ticks for TRADES and for MIDPOINT (first tick 1.09043, `size` `"0"`).
  1,000 ticks and a `startDateTime` request instead of an end date answered the same way.
- AAPL TRADES got one `HistoricalTickLast` frame (114 ticks, `isDone` true). An AAPL TRADES
  window with no ticks (start 2026-11-24 00:30 UTC, regular hours) got one
  `HistoricalTickLast` frame with no ticks and `isDone` true: an empty answer keeps the
  requested kind, so a frame of another kind means TWS has none of the requested kind.
- BID_ASK and MIDPOINT were each answered with their own kind for all four contracts. Earlier
  the same day (23:56Z), SPX index BID_ASK and MIDPOINT drew error 10187 and BTC (PAXOS)
  TRADES drew 10299 ("Expected what to show is AGGTRADES"): errors, not another kind.
- So `TickSubscription` ends with `Error::UnexpectedTickKind` on a frame of another kind; it
  skipped the frame and waited forever, because the channel stays open.
- **Not probed:** a BID_ASK or MIDPOINT request answered with another kind (none provoked);
  futures, options and bonds TRADES without trades; whether the substitute frame is ever
  split across several frames with `isDone` false.

### Delayed volume can be negative on the wire

*Live 2026-11-24 01:00Z (Monday 20:00 ET, after hours), paper TWS, server 223; raw capture.*

- `reqMktData` for US stocks under market data type 3 (delayed, streaming, generic tick 165)
  and 4 (delayed frozen, snapshot). `TickSize` tick type 74 (`DelayedVolume`) carried
  negative decimal strings for MSFT (`"-235087060"`, then `"-234537060"` 10 s later), NVDA,
  INTC, F, SPY and QQQ, in both modes. The raw frame holds the text: field 3, length 10,
  `2d 32 33 35 30 38 37 30 36 30` (`-235087060`). The decoder parses that string to the same
  `f64`; nothing narrows it to 32 bits. The first report, MSFT `-1232329840` (snapshot,
  23:49Z), is the same fact.
- The values also disagree with the day's volume where they are positive. Daily TRADES bars
  for 2026-11-23, extended hours: AAPL 41,406,011 against `DelayedVolume` 48,288,217; AMD
  42,241,724 against 2,119,449,190; IBKR 4,649,304 against 1,226,539,095. Neither the raw
  value nor a 32-bit unwrap of it (adding 2^32, with or without a factor of 100) matches the
  bar volumes, so a caller cannot repair the value or tell a bad positive one from a real one.
- Other size ticks in the same runs were plausible and never negative: `AvgVolume` (21, e.g.
  MSFT `"596315"`), `DelayedBidSize` (69) and `DelayedAskSize` (70).
- TSLA drew 10089 and no ticks; SPY and QQQ answered with delayed data.
- **Not probed:** real-time `Volume` (8) (no real-time US equity permission on this account);
  delayed volume during regular hours; other markets.

### Open interest ticks

*Live 2026-11-24 Tue 09:47-09:50 UTC (US equities and options closed, CME Globex open), paper,
server 223; delayed data (type 3).*

- `reqMktData` with generic tick `101` on an AAPL Dec 2026 350 call: `OptionCallOpenInterest`
  (27) `30680` and `OptionPutOpenInterest` (28) `0`, as size ticks, 0.2 s after the request. The
  350 put: 27 `0`, 28 `9762`. ES Dec 2026 7200 futures options (FOP) the same way: call 27
  `4957` / 28 `0`, put 27 `0` / 28 `9796`. Each pair arrived once in 20 s.
- Generic tick `588` on ES Dec 2026: `FuturesOpenInterest` (86) `2111144`, once in 20 s. On an ES
  futures option, `588` (with `101`) added nothing.
- `101` on AAPL stock drew 10089 (no stock data on this account), so the stock's aggregate call and
  put open interest was not seen.
- A snapshot with generic tick `101` draws 321 "Snapshot market data subscription is not
  applicable to generic ticks".
- Settles that an option's own open interest is the tick for its right, and the other tick's `0`
  is not a value for the contract. IB's tick type table describes `101` as "currently for
  stocks".
- **US trading hours** (*live 2026-11-24 15:57-16:04 UTC, US market open, paper TWS, server
  223*): the same shape. Delayed (type 3), `101` on the AAPL Dec 2026 350 call drew 27 `30568`
  and 28 `0` 0.1 s after the request, the 350 put 27 `0` and 28 `10223`, the ES Dec 2026 7500
  call 27 `7259` / 28 `0` and the put 27 `0` / 28 `13667`; each pair once in a 30 s window.
  The AAPL call's value differs from the 09:47 UTC read
  (30680) of the same day. Real-time (type 1) on the AAPL call and the ES option drew 354
  and no open interest tick.
- **Not probed:** real-time data on an account with option data permission; `OpenInterest` (22),
  which never arrived; a stock's aggregate values.

### Index ticks

*Live 2026-11-24 Tue 09:41-09:44 UTC (Xetra open; CBOE, OSE and HKFE closed), paper, server 223;
#1284.*

- `reqMktData` real-time on SPX (CBOE), DAX and ESTX50 (EUREX), N225 (OSE.JPN) and HSI (HKFE):
  354 for each; this account has no index subscription. Delayed (type 3) works after 10167.
- Delayed, on SPX, DAX, N225 and HSI: one `DelayedBid` and one `DelayedAsk` of `-1` with size `0` at the start,
  never a real one. `DelayedLast` is a real price with size `1`, closed markets included (SPX
  `7236.85`, N225 and HSI their last value), with a `DelayedLastTimestamp`. No `-1` last price
  arrived.
- `DelayedOpen` is `0` (a price tick, no size) on all four, DAX included while it traded.
  `DelayedHigh` and `DelayedLow` are real; SPX sent neither while CBOE was closed.
- DAX during its session: a new `DelayedLast` every 5 seconds, size `1` each time.
- Settles that an index's bid and ask are the no-quote placeholder of every other contract
  (`-1` with size `0`, #1284), and that its last price is a value, not a placeholder, even with
  the market closed. The size `1` is not a traded size.
- **US trading hours** (*live 2026-11-24 16:00-16:02 UTC, CBOE and Nasdaq open, paper TWS,
  server 223*): delayed SPX (CBOE), NDX (NASDAQ) and VIX (CBOE) the same way. One `DelayedBid`
  and one `DelayedAsk` of `-1` with size `0` first, then a `DelayedLast` with size `1` and a
  `DelayedLastTimestamp` every 5 seconds (SPX `7158.43`, `7154.96`, ...). `DelayedOpen` was `0`
  on all three with the session open; `DelayedHigh`, `DelayedLow` and `DelayedClose` (the
  previous close) were real. `DelayedVolume` came as a small count that grew (SPX `4535`, then
  `4565` 10 s later; NDX `4561`; VIX `1776`, then `1778`); what it counts was not
  established, since an index does not trade. Real-time on SPX
  and VIX: 354.
- **Not probed:** real-time index data (no subscription); delayed ESTX50; an index before its
  first value of a day; snapshots.

### Market depth rates, rows and refusals

*Live 2026-11-24 Tue 09:16-09:59 UTC (FX and Xetra open; US equities overnight), paper, server
223; nautechsystems/nautilus_trader#5395.*

- `reqMktDepth` EUR.USD (IDEALPRO), 10 rows, 26 minutes each with `isSmartDepth` false and true
  at once: both sides updated every minute, 2,400 to 9,200 rows per side per minute; the longest
  silence was 5.4 s, on both sides at the same moment. Neither side went silent while the other
  updated. Totals per run: about 218,000 updates, 8,900 inserts and 9,000 deletes.
- `isSmartDepth` false sends `MarketDepth` rows; true sends `MarketDepthL2` rows with market
  maker `IDEALPRO` and `isSmartDepth` true, after a 2152 notice "Exchanges - Depth: IDEALPRO;".
- Rows: 50 requested on EUR.USD gives positions 0-9 (10 rows); 5 requested gives 0-4, built with
  10 inserts and then updates only.
- A fourth concurrent request, from any client id on the same TWS, draws 309 "Max number (3) of
  market depth requests has been reached" and ends the stream.
- Refusals: AAPL `SMART` without smart depth draws 10092 and ends; AAPL `ISLAND` draws 10089
  as a notice and then nothing (the stream stays open); SAP `IBIS` and ES draw 354. AAPL smart
  depth draws a 2152 listing the exchanges without depth permission, then nothing overnight.
- No 317 (depth reset) or 316 (halted) arrived in 52 subscription-minutes.
- No one-sided stall (one side silent while the other updates) reproduced on FX depth.
- **Not probed:** US stock depth during US trading hours; a 317 reset (which rows TWS resends
  after it); depth longer than 26 minutes.

### Tick-by-tick sizes and `IgnoreSize`

*Live 2026-11-16 03:33Z (Sunday evening ET), paper Gateway, server 226, BTC/PAXOS; #1244.*

- `last` / `all_last` (24 frames) and `bid_ask` (121 frames, both `IgnoreSize` values): every
  frame carried its size fields (`size`; `size_bid` and `size_ask`).
- `mid_point` (84 frames): no size field in any frame.
- `bid_ask` with `IgnoreSize::Yes` (one window, 40 updates read): every update changed a price
  (0 of 39 consecutive pairs with both prices unchanged). With `IgnoreSize::No` (two windows, 40
  updates read in each), 12 of 39 and 4 of 39 pairs changed only sizes. Consistent with `Yes`
  dropping size-only updates rather than the size fields.
- Requested right after the `IgnoreSize::No` subscription on the same contract was cancelled,
  `IgnoreSize::Yes` got no frames and no error in 25 s. Requested first, it delivered. One run,
  not explained.
- Historical ticks (live 2026-11-15 08:27 ET, Sunday, paper Gateway, server 226; AAPL ticks of
  2026-11-13): every trade tick (1018) carried `size` and every bid/ask tick (17) `size_bid` and
  `size_ask`; every midpoint tick (19) carried `size`, always `"0"`.
- Historical bid/ask ticks with `IgnoreSize` (live 2026-11-16 05:02Z, Monday 00:02 ET, paper
  Gateway, server 226; AAPL, 200 ticks requested ending 2026-11-13 21:00 UTC, all returned
  ticks fall in 20:59:58-59 UTC; each request drew notice 2188 and still delivered): decoded
  from the capture, `Yes` 475 ticks and `No` 1009, every one with `size_bid` and `size_ask`. 0
  of 474 consecutive `Yes` pairs and 534 of 1008 `No` pairs had both prices unchanged: `Yes`
  drops size-only updates here too.
- ES and EUR.USD tick-by-tick sizes, all sent: [Size fields are always sent](#size-fields-are-always-sent).
- **Not probed:** live stock ticks during RTH, live accounts, TWS rather than the Gateway.

### Market data type and permission notices

*Live 2026-11-24 10:00-10:30 UTC (US pre-market, CME Globex and FX open), paper TWS, server 223;
#677.*

Each request under each market data type set with `switch_market_data_type`, read for 10-15 s;
"nothing" means no tick or bar for that time. A cancel sent afterwards shows whether TWS still
held the request (300: it did not).

| Request | Type 1 live / 2 frozen | Type 3 delayed / 4 delayed frozen |
| --- | --- | --- |
| AAPL market data, snapshot, realtime bars, tick-by-tick | 10089, nothing; cancel 300 | the same (10089, nothing, 300) |
| MSFT, IBM market data and snapshot | 10089, nothing; cancel 300 | `MarketDataType(Delayed)`, 10167, delayed ticks; snapshot ends |
| ES market data and snapshot | 354 | `MarketDataType(Delayed)`, 10167, delayed ticks |
| VOD (LSE) market data | 10090, then 354 (frozen snapshot: 10168) | 10090, 10167, delayed ticks |
| ES, VOD realtime bars | 420 | 420 |
| ES, VOD tick-by-tick | 354 | 354 |
| EUR.USD, all four | real-time data, `MarketDataType(Realtime)` | the same: real-time data |

- **10089 is never followed by data.** It ended the request in 36 of 36 requests across all
  four types and four request kinds; every later cancel drew 300. On types 3 and 4 TWS sends
  10167 instead when it has delayed data for the contract (MSFT, IBM), and still sends 10089
  when it does not (AAPL on this account, at this time; AAPL had delayed data at 09:22 UTC the
  same day). So the market data type does not decide what 10089 means.
- **10167 is followed by delayed data**, after a `MarketDataType(Delayed)` tick.
- **10090 is followed by whatever else the request gets**: 354 on types 1 and 2, delayed data
  on types 3 and 4.
- Generic ticks 233, 236, 258, 456 drew 10358 (fundamentals) on stocks and futures and 321 on
  forex before any notice above.
- **Not probed:** 10091; RTH; an account with real-time US equity data; options.

