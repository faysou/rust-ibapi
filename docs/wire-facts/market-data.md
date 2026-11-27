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

### Option computation ticks during US hours

*Live 2026-11-24 15:57-16:04 UTC, US market open, paper TWS, server 223; streaming `reqMktData`
for 30 s on AAPL Dec 2026 350 call and put, Nov 27 350 call and Dec 500 call, and ES Dec 2026
7500 call and put; raw capture decoded.*

- Delayed data (type 3): `DelayedBidOption` (80), `DelayedAskOption` (81), `DelayedLastOption`
  (82) and `DelayedModelOption` (83), 172 frames in all. Bid, ask and last arrived 6 to 12 times
  per contract in 30 s, the model 1 to 4 times. No real-time computation tick (10 to 13) came.
- Real-time data (type 1) on the AAPL options and the ES option: 354, and no computation tick.
- A field TWS has not computed is sent as a value, never left out: `-1` for implied volatility
  and the option price, `-2` for delta, gamma, vega and theta. Every frame carried all eleven
  fields. 20 of the 158 bid, ask and last frames carried `-1` and `-2` for implied volatility
  and every greek, 11 of them also `-1` for the price (a side with no quote); the other 138
  carried them all. The first model frame of the AAPL 350 call sent present value of dividends
  `-1`, its later ones `0`; bid, ask and last sent `0` there every time.
- A real value can equal a sentinel of the other group: `DelayedLastOption` on the ES 7500 put
  sent theta `-1.0`, then `-1.25`, `-0.5` and `-0.0` as its last price aged (the other
  computations of that put had theta about `-1.9`). Theta `-1` is a value; only `-2` means not
  computed for a greek.
- `tick_attribute` was `0` (return based) on every frame. `underlying_price` was always set.
- **Not probed:** real-time computations (no option data permission); snapshots during US
  hours; index options.

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
- Real-time EUR.USD (IDEALPRO) for 10 minutes (*live 2026-11-26 16:28-16:38 UTC, US
  Thanksgiving holiday, paper TWS, server 223*): 427 bid and ask `TickPrice` ticks and 1157
  `BidSize` / `AskSize` ticks, none with size `0` and none with a price of `0` or below.
- Settles that every placeholder price came with size `0`, and every real quote, negative ones
  included, with a positive size. A positive price with size `0` on a bid or ask was never seen.
- **Not probed:** `pre_open` during a real-time pre-open or halt (no such data on this account;
  delayed ticks carry no attributes); empty sides in tick-by-tick bid/ask or last (no real-time
  data on an illiquid contract); real-time equity and option quotes in US hours (this account has
  no real-time US equity or option data; US markets were closed on the 2026-11-26 run).

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

### Bar trade fields (volume, WAP, count)

*Live 2026-11-15 Sun 22:25 ET (FX and CME Globex open, US equities overnight), paper, server
226; raw capture.*

- `reqRealTimeBars` MIDPOINT, EUR.USD and ES: `RealTimeBarTick.volume` (7) and `WAP` (8)
  **absent** in 7 of 7 frames; `count` sent as `-1`. TRADES on ES: all three sent with real
  values. EUR.USD TRADES draws 420 (no Last data for CASH).
- `reqHistoricalData` MIDPOINT and BID, 1-hour bars, AAPL, EUR.USD and ES: `volume` and `WAP`
  **sent** as `"-1"`, `barCount` as `-1`. TRADES sends real values.
- Settles that the two bar types differ: realtime leaves the fields out, historical sends a
  `-1` sentinel. C# reads an absent realtime value as unset (`decimal.MaxValue`).

*Live 2026-11-16, paper, server 226; raw capture; #1247.*

- A TRADES bar in which nothing traded sends volume `"0"`, count `0` and WAP as a price, not
  the `-1` sentinel: 428 of 541 historical 5-second AAPL premarket bars, 6 of 597 ES, 3 of 3
  realtime BTC/PAXOS.
- Realtime BID and ASK (EUR.USD, BTC/PAXOS) match MIDPOINT: volume and WAP absent, count `-1`.
  Historical ASK and BID_ASK (ES), MIDPOINT and ASK (EUR.USD), MIDPOINT (BTC/PAXOS): `-1` for
  all three. Keep-up-to-date `HistoricalDataUpdate` bars for MIDPOINT and BID: `-1` for all
  three (16 of 16, EUR.USD and BTC/PAXOS).
- ADJUSTED_LAST and AGGTRADES carry real trade values. HISTORICAL_VOLATILITY and
  OPTION_IMPLIED_VOLATILITY (AAPL) send volume `"1"`, count `0` and a WAP.
- Keep-up-to-date TRADES on BTC/PAXOS, 8 minutes: 97 update bars, all with `bar_count` (81
  with trades, 16 quiet). TWS answered 10299 ("use AGGTRADES") and streamed anyway.
- Settles that `-1` and absence both mean no trade data, while a quiet TRADES bar is a real
  zero.
- **Not probed:** AAPL realtime bars (no data overnight); a keep-up-to-date TRADES update bar
  without `bar_count` (#1057), for which stocks and futures need up-to-the-second data (2188 on
  the probing account).

*Live 2026-11-23 Mon 18:55-19:05 ET (FX, crypto and CME Globex open, US equities after
hours), paper TWS, server 223; raw capture; #1285.*

- `reqRealTimeBars` MIDPOINT, BID and ASK, EUR.USD, XAUUSD and BTC (PAXOS): `volume` (7) and
  `WAP` (8) **sent** as `"-1"` and `count` as `-1` in every frame, where server 226 left volume
  and WAP out. TRADES and AGGTRADES on BTC send real values.
- `reqHistoricalData` MIDPOINT, BID, ASK and BID_ASK, 1-hour bars (AAPL, ES, EUR.USD, an AAPL
  option, a CBOT corn option, the IBUS500 CFD, XAUUSD, BTC) and daily bars (AAPL, ES,
  EUR.USD): `-1` for all three, as on server 226. An SPX TRADES bar sends `volume` and `WAP`
  as `"0"` with a real `barCount`.
- The corn option (OZC, price magnifier 100) sends the non-TRADES `WAP` as `"-100"`, `-1` times
  the magnifier. `volume` and `barCount` stay `-1`.
- Volatility and adjusted bars (2026-11-25 Wed 11:10 ET, US equities open, same server; decoded
  values, 1-hour bars over 2 days, AAPL): ADJUSTED_LAST sent real volume, WAP and count, the
  same as TRADES (9 bars). HISTORICAL_VOLATILITY (1 bar) and OPTION_IMPLIED_VOLATILITY (10 bars)
  sent volume `1` and count `0` on every bar, with the WAP a volatility (`0.289`, `0.260`), as
  on server 226.
- Settles that a realtime bar can carry either form, and that the WAP sentinel scales with the
  price magnifier, so only a volume test can tell it from a price.
- **Not probed:** realtime bars on a magnified contract; TRADES `keepUpToDate` updates (BTC draws
  10299 for TRADES and 321 "Source price not supported with live updates" for AGGTRADES).

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
  bar size), not on bars returned. ESZ6 returns fewer bars than steps (1192 over 1800 S), so
  its 2400 S request would hold about 1600 bars, yet it failed. Nor is it a bar cap shared by
  every bar size: 5760 five-second bars passed.
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
  from 20:30:00 to 20:59:59; [above](#one-second-bars-reach-back-at-most-2000-s)). Weekly and
  monthly bars are stamped with the end of the period instead; `HistoricalData::trim_to`
  documents both rules (#1267).
- **Not probed:** daily bars in this run; a holiday on a period's last weekday, so the last
  trading day is not separated from the last weekday; other instruments; the Gateway.

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
  TRADES series; EUR.USD draws 162 (2026-11-24, #1298). "Not probed" realtime cells drew 10089,
  354 or 420 for missing real-time permission. SPX drew 10187 for BID_ASK and MIDPOINT historical ticks.
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
  rejection only for historical ticks, where nothing follows it. The first run here recorded a
  client that ended the request on the notice and so dropped the bars.
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
- Twelve requests through `between(..).fetch()`, 0.5 s apart (*live 2026-11-26 17:36 UTC, US
  Thanksgiving holiday, same server*): EUR.USD `MIDPOINT` 1-second bars, extended hours, over the
  24,000 s before now took 10.0 s and returned 24,000 bars, one per second, with no repeat, gap
  or pacing error.
- **Not probed:** count-back across futures session breaks, more than twelve requests in a row,
  several clients paging at once, or Gateway.

### Fixed-end historical bar requests can end with a no-data error

*Live 2026-11-26 23:46-23:47 UTC (after US close), paper TWS, server 223; #1270.*

- Three fixed-end requests for 2000 S of 1-second CME futures `TRADES` bars with extended hours
  returned only error 162. ESZ7 ending 2026-11-25 16:00 UTC and 2026-11-26 06:00 UTC returned
  `Historical Market Data Service error message:HMDS query returned no data: ESZ7@CME Trades`.
  ESZ8 ending 2026-11-25 16:00 UTC returned the same message for ESZ8.
- Two AAPL `TRADES` regular-hours controls with the same duration and bar size each returned
  2000 bars in `HistoricalData` followed by `HistoricalDataEnd`. Both ended at 21:00 UTC,
  one on 2026-11-25 and the other on 2025-11-25.
- Split `between(..).fetch()` treats this no-data form of error 162 as an empty page and
  continues paging.
- **Not probed:** an empty page between populated pages of the same contract, an all-empty
  split fetch, or Gateway.

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
- Settles the paging rules of `historical_ticks_between` and `historical_ticks_latest`, and when they
  report `PagingStop::ReachedStart` or `PagingStop::EmptyPage`.
- **Not probed:** stock bid/ask pages, `IgnoreSize::Yes` pages, whether the single tick is the
  first or the last of its second, or whether a quote reply covering just one second is complete.

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

### Index ticks

*Live 2026-11-24 Tue 09:41-09:44 UTC (Xetra open; CBOE, OSE and HKFE closed), paper, server 223;
#1284. US trading hours: live 2026-11-24 16:00-16:02 UTC (CBOE and Nasdaq open), paper TWS,
server 223.*

- `reqMktData` real-time on SPX (CBOE), DAX and ESTX50 (EUREX), N225 (OSE.JPN) and HSI (HKFE):
  354 for each; this account has no index subscription. Delayed data (type 3) arrives after a
  10167 notice.
- Delayed, on SPX, DAX, N225 and HSI: one `DelayedBid` and one `DelayedAsk` of `-1` with size
  `0` at the start, never a real one. `DelayedLast` is a real price with size `1`, closed markets included (SPX
  `7236.85`, N225 and HSI their last value), with a `DelayedLastTimestamp`. No `-1` last price
  arrived.
- `DelayedOpen` is `0` (a price tick, no size) on all four, DAX included while it traded.
  `DelayedHigh` and `DelayedLow` are real; SPX sent neither while CBOE was closed.
- DAX during its session: a new `DelayedLast` every 5 seconds, size `1` each time.
- Settles that an index's bid and ask are the no-quote placeholder of every other contract
  (`-1` with size `0`, #1284), and that its last price is a value, not a placeholder, even with
  the market closed. Inferred: the size `1` is not a traded size.
- **US trading hours**: delayed SPX (CBOE), NDX (NASDAQ) and VIX (CBOE) the same way. One `DelayedBid`
  and one `DelayedAsk` of `-1` with size `0` first, then a `DelayedLast` with size `1` and a
  `DelayedLastTimestamp` every 5 seconds (SPX `7158.43`, `7154.96`, ...). `DelayedOpen` was `0`
  on all three with the session open; `DelayedHigh`, `DelayedLow` and `DelayedClose` (the
  previous close) were real. `DelayedVolume` came as a small count that grew (SPX `4535`, then
  `4565` 10 s later; NDX `4561`; VIX `1776`, then `1778`); what it counts was not
  established, since an index does not trade. Real-time on SPX and VIX: 354.
- **Not probed:** real-time index data (no subscription); delayed ESTX50; an index before its
  first value of a day; snapshots.

### Delayed volume is negative or wrong on the wire

*Live 2026-11-24 01:00Z (Monday 20:00 ET, after hours), paper TWS, server 223; raw capture.*

- `reqMktData` for US stocks under market data type 3 (delayed, streaming, generic tick 165)
  and 4 (delayed frozen, snapshot). `TickSize` tick type 74 (`DelayedVolume`) carried
  negative decimal strings for MSFT (`"-235087060"`, then `"-234537060"` 10 s later), NVDA,
  INTC, F, SPY and QQQ, in both modes. The raw frame holds the text: field 3, length 10,
  `2d 32 33 35 30 38 37 30 36 30` (`-235087060`). The decoder parses that string to the same
  `f64`; nothing narrows it to 32 bits. An earlier run (snapshot, 23:49Z) saw MSFT
  `-1232329840`.
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

### Market depth rates, rows and refusals

*Live 2026-11-24 Tue 09:16-09:59 UTC (FX and Xetra open; US equities overnight), paper, server
223; nautechsystems/nautilus_trader#5395.*

- `reqMktDepth` EUR.USD (IDEALPRO), 10 rows, two concurrent 26-minute subscriptions, one with
  `isSmartDepth` false and one true: both sides updated every minute, 2,400 to 9,200 updates per
  side per minute; the longest silence was 5.4 s, on both sides at the same moment. Totals per
  subscription: about 218,000 updates, 8,900 inserts and 9,000 deletes.
- `isSmartDepth` false sends `MarketDepth` rows; true sends `MarketDepthL2` rows with market
  maker `IDEALPRO` and `isSmartDepth` true, after a 2152 notice "Exchanges - Depth: IDEALPRO;".
- Rows: 50 requested on EUR.USD gives positions 0-9 (10 rows) on each side. 5 requested on
  GBP.USD and USD.JPY gives positions 0-4 on each side, built with 5 inserts per side and then
  updates only.
- A fourth concurrent request, from any client id on the same TWS, draws 309 "Max number (3) of
  market depth requests has been reached" and ends the stream.
- Refusals: AAPL `SMART` without smart depth draws 10092 and ends; AAPL `ISLAND` draws 10089
  and then no rows; SAP `IBIS` and ES draw 354. AAPL smart
  depth draws a 2152 listing the exchanges without depth permission, then nothing overnight.
- No 317 (depth reset) or 316 (halted) arrived in 52 subscription-minutes.
- No one-sided stall (one side silent while the other updates) reproduced on FX depth.
- **Not probed:** US stock depth during US trading hours; a 317 reset (which rows TWS resends
  after it); depth longer than 26 minutes.

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
- So `historical_ticks(..)` ends with `Error::UnexpectedTickKind` on a frame of another kind.
  Skipping the frame waits forever, or until `one_shot_timeout`, because TWS sends nothing more.
- **Not probed:** a BID_ASK or MIDPOINT request answered with another kind (none provoked);
  futures, options and bonds TRADES without trades; whether the substitute frame is ever
  split across several frames with `isDone` false.

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

### Realtime bars across the SEHK and HKFE lunch break

*Live 2026-11-25 01:38-05:00 UTC (SEHK and HKFE trading, lunch 04:00-05:00 UTC), paper TWS,
server 223; `realtime_bars` (5-second TRADES bars, all trading hours) on 700 (SEHK) and the HSI
December future (HKFE), both with real-time permission.*

- During the session TWS sent a bar every 5 seconds, 5 to 6 s after the bar's start, including
  bars with volume 0 at the last price (700 at 03:58:00, 03:58:15; HSI at 03:59:50, 03:59:55).
- The last bar before the break, 03:59:55, arrived at 04:00:01.3 like any other. Then nothing,
  no bar and no notice, for both contracts until 05:00:05.4, when the 05:00:00 bar arrived.
  No bar was sent for the break.
- The first 700 bar after the break carried 196,800 shares against hundreds to tens of thousands
  per bar before it.
- **Not probed:** the SEHK closing auction (08:00-08:10 UTC); RTH-only realtime bars;
  keep-up-to-date historical bars across the same break (historical requests went unanswered
  from 00:55 UTC that night, with no error, while market data kept working).

### EUR.USD depth rows are fixed positions

*Live 2026-11-25 19:16-19:19 UTC, paper TWS, server 223; EUR.USD on IDEALPRO, 5 rows, 150 s.
Reported downstream the same day: a 3-minute recording with 10 rows.*

- 12,336 row operations: 73 inserts, 12,139 updates and 124 deletes. The downstream recording had
  46,775.
- An update replaces the price and size of the row at `position`: 2,052 of the updates changed
  the row's price, so a row is a slot, not a price level.
- Every insert after the first dump landed above every row present on its side, that is at the
  end of the side (73 here, all 1,322 downstream).
- Deletes target the last row and arrive with price 0 and size 0 (124 of 124). Here 64 removed
  the side's last row and 60 repeated a delete of a row already gone, at the same position.
  Downstream, 1,292 removed the last row, 17 a lower row and 18 an absent one.
- Read the operations as fixed slots: insert and update set row `position`, delete clears it and
  the rows after it. Downstream, a model that shifts rows on insert and delete left a side
  unsorted after 31,049 of the operations, against 3,775 transient states for fixed slots. In
  this run both models gave the same book, because no delete hit a lower row.
- **Not probed:** depth of other instruments (no CME or US stock depth permission); SMART depth
  (`MarketDepthL2`).

### Long historical bar requests take minutes

*Live 2026-11-25 19:12-19:16 UTC, paper TWS, server 223; one request at a time. ES `CONTFUT`
reported downstream the same day.*

| Request                                               | Time    | Bars   |
| ----------------------------------------------------- | ------- | ------ |
| EUR.USD MIDPOINT, 1-minute bars, 30 D                 | 102.8 s | 42,583 |
| EUR.USD MIDPOINT, 1-hour bars, 5 Y                    | 138.0 s | 31,072 |
| ES `CONTFUT` TRADES, 1-minute bars, 30 D (downstream) | 107.6 s | 40,877 |
| ES `CONTFUT` TRADES, 1-hour bars, 5 Y (downstream)    | 161.8 s | 16,121 |

- Inferred from the times, not run: a `one_shot_timeout` of 60 s would end each of these with
  `Error::Timeout` before the bars arrive. Size the timeout to the longest request, or split the
  range.
- **Not probed:** other bar sizes and instruments; how the time grows with concurrent requests.

### 10299 `whatToShow` substitution

*Live 2026-11-24 10:20-10:35 UTC, paper TWS, server 223; #1286.*

10299 names the `whatToShow` TWS expects: "Expected what to show is AGGTRADES, please use that
instead of TRADES" on crypto, "Expected what to show is TRADES ..." on others. What follows it:

| Request | Contract, `whatToShow` | After 10299 |
| --- | --- | --- |
| Historical bars (`fetch`, `subscribe`) | BTC, ETH (PAXOS) `TRADES` | the bars and `HistoricalDataEnd`; bars identical to an `AGGTRADES` request |
| Historical bars | AAPL, ES `AGGTRADES` | the `TRADES` bars and `HistoricalDataEnd` |
| Historical bars | EUR.USD `AGGTRADES` | 162 "No historical market data" |
| `keepUpToDate` bars (`subscribe_with_updates`) | BTC, ETH `TRADES` | the bars, `HistoricalDataEnd`, then updates every few seconds; a cancel draws 162 (cancelled) |
| `keepUpToDate` bars | AAPL, EUR.USD, ES `AGGTRADES` | 321 "Source price not supported with live updates" at once (AAPL, again 2026-11-26: same millisecond, then nothing for 65 s); a cancel draws 366 |
| Historical ticks | BTC, ETH `TRADES` | nothing for 20 s; a cancel draws no reply |

- Without 10299: crypto `AGGTRADES` bars come with no notice; crypto `AGGTRADES` with
  `keepUpToDate` draws 321 and nothing else, so `TRADES` is the only way to stream crypto bars.
- `head_timestamp` with crypto `TRADES` or `AGGTRADES` answers with no 10299 (2019-03-13 for
  BTC). With `AGGTRADES` on AAPL, EUR.USD and ES it draws 162. `histogram_data` takes no
  `whatToShow` and answers for BTC and ETH.
- A cancel sent within ~1 ms of the 10299 on a crypto `TRADES` `keepUpToDate` request (earlier
  run) drew 366 while the updates kept coming: TWS had not yet set up the substituted query.
  One sent seconds later drew 162 and stopped them.
- **Not probed:** other crypto venues (ZEROHASH), RTH equities, bar sizes other than 1 hour
  and 1 minute.

### Streams across an internet outage

*Live 2026-11-25 12:40-13:08 UTC and 2026-11-26 12:00-12:35 UTC (US pre-market, FX open), paper
TWS, server 223; EUR.USD IDEALPRO; Wi-Fi off for 15 s, 2 min, 6 min and 20 min, router uplink
unplugged for 2 min; one client never cancelled or re-requested; #1300.*

- `reqMktData`, tick-by-tick, depth and realtime bars resumed on the same request 0.5 to 1.5 s
  before the 1102. A re-request at the last farm OK or the 1102 delivered 3.4 to 4.4 s later.
  2026-11-26: they resumed within [-2.2, +0.9] s of the 1102; tick-by-tick 0.1 to 0.9 s after it.
- Realtime bars: TWS sent every missed 5-second bar on the original request in one burst (5, 30,
  81 bars, no slot missing; 2026-11-26: 244 bars from 12:00:50 to 12:21:05 and 27 bars, no slot
  missing). A re-request delivers only new bars. Across a TWS restart nothing is back-filled; see
  [TWS restart and reconnect](protocol.md#tws-restart-and-reconnect).
- No tick-by-tick tick stamped more than 5 s before arrival; rate after restore was normal.
- Depth: 317 arrived in the same millisecond as 2103 for `cashfarm`, at the loss. Rows resumed on
  the same request with no second 317. 2026-11-26: the first rows after the restore inserted all
  10 rows, so the book refills from empty.
- Keep-up-to-date bars: 10182 "Failed to request live updates (disconnected)." arrived in the
  same millisecond as 2105 for `cashhmds`, after it, every run, every client (one also got 165);
  2026-11-26, about 1 ms after it. No frame followed in 270 s, 389 s, 635 s, 1587 s and 390 s.
  A cancel after the 10182 drew 366 within 3 ms (4 of 4), so the stream sends none. A new
  request after the restore returned a batch covering every outage minute, 2.5 to 3.7 s after
  the 1102. One sent at the first HMDS OK was answered 0.6 s after the 1102, one sent at the
  second HMDS OK 0.1 s before it. `Client::resubscribing` reopens the request once no HMDS farm
  reports broken, and once more if TWS confirms the farms back later and the reopened request
  has delivered no bar.
- The second reopen ran live in the 20-minute run: the reopen at the 15-minute limit, with the
  HMDS farms still broken, got no frame; at the `cashhmds` OK the wrapper cancelled it and
  reopened, and the batch held all 21 outage minutes:

  ```text
  12:01:30.656 [10182] on 1500000003; the wrapper yields it and drops the request
  12:16:30.660 reqHistoricalData 1500000007 (15 min after the 10182; farms still broken)
  12:21:11.428 2106 cashhmds OK; cancel 1500000007, reqHistoricalData 1500000008
  12:21:11.960 [366] for the cancel of 1500000007
  12:21:12.490 batch on 1500000008, 21 of 21 outage minutes, 0.1 s before the 1102
  ```

- 10197 "No market data during competing live session" does not end a market data request: drawn
  51 s into the 6-minute outage on all clients; on the client that sent no cancel, ticks resumed
  on the same id at restore (2233 to the end). Tick-by-tick, depth and realtime bars got no 10197.
  A market data subscription yields it as a notice and keeps reading
  (`TickTypes::error_continues_request`).
- **Not probed:** 1101, and whether streams resume on their own ids after it (a reopen after 1101
  loses the realtime bar back-fill if they do); whether TWS answers a keep-up-to-date bar request
  sent during the outage once its farm is back (every such request was replaced before the
  restore); the path where an HMDS farm goes inactive (2107) instead of reporting OK; stocks,
  futures, RTH; depth update rows after restore; 10197 during a real competing session.

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
- Market depth (*live 2026-11-26 16:27 UTC, US Thanksgiving holiday, same server, type 1*): AAPL
  `ISLAND`, 5 rows, drew 10089 ("... AAPL NASDAQ.NMS/DEEP") 0.5 s after the request, then no row
  for 65 s; the cancel then drew 310 "Can't find the subscribed market depth with tickerId", so
  TWS had ended it.
- **Not probed:** 10091; RTH; an account with real-time US equity data; options; depth on other
  contracts.

### Historical ticks reply

*Live 2026-11-15 weekend (Sunday 08:27 ET), 2026-11-16 RTH (09:58 ET) and 2026-11-16 after the
close (17:58-18:05 ET), paper Gateway, server 226; the first two runs AAPL with `ending`
2026-11-13 20:00 UTC, the third AAPL, SEB, ES, EUR.USD and BTC/PAXOS with `starting` or `ending` (#1317).*

- Every answered request (23) got one frame with `is_done` true. With `ending`: 10 trades asked
  → one `HistoricalTickLast` of 14 ticks; 1000 asked → 1004; 10 midpoints → one `HistoricalTick`
  of 19; 10 bid/asks → one `HistoricalTickBidAsk` of 17. Notice 2188 preceded each AAPL frame.
- TWS returns whole seconds, so a reply can be much longer than asked. AAPL trades with
  `starting` at the open (2026-11-13 14:30:00 UTC): 10 asked → 332 ticks, all in that second;
  1000 asked → 2058 ticks over three seconds. Ticks arrive oldest first.
- A midpoint or bid/ask reply holds one tick of its earliest second. That was so in all 10
  distinct replies with more than a few ticks per second (12 frames, the 20:00 requests being
  repeated in two runs), where later seconds held up to 385. With `starting`, that tick is
  stamped before the start (14:29:59 for a 14:30:00 start; 14:28:56 on the thin ES 202712). A
  trades reply with `starting` returned its first second whole: the open's second held 332 ticks
  whether 10 or 1000 were asked.
- A contract with no trades in the window gets one frame with no ticks (ES 202712).
- 1001 and 5000 trades asked → 10187 "Failed to request historical ticks:Max length exceeded"
  and no frame. 0 asked → 321 "... Number of ticks must be > 0". IB's historical-ticks page
  (interactivebrokers.com/docs/tws-api/doc/md/historical-ticks.md) reads "Max is 1000 per
  request".
- Trades asked for a forex pair (EUR.USD): no error, and the reply is a `HistoricalTick`
  (midpoint) frame, not `HistoricalTickLast`, so `last()` returns `Error::UnexpectedTickKind`
  ([another kind](#historical-ticks-of-another-kind)); use `mid_point()` or `bid_ask(..)` for
  forex. BTC/PAXOS trades → 10299 "Expected what to show is AGGTRADES".
- The reply is finite and one frame here, which is why `historical_ticks` returns a `Vec`; the
  crate still reads frames until `is_done`, so a reply TWS split would be returned whole.
- **Not probed:** whether the 1000 cap depends on the account or its market data subscriptions
  (one paper account); a reply of more than one frame, which was not seen; non-US stocks.
