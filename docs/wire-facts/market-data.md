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
- Settles that a realtime bar can carry either form, and that the WAP sentinel scales with the
  price magnifier, so only a volume test can tell it from a price.
- **Not probed:** realtime bars on a magnified contract; TRADES `keepUpToDate` updates (BTC draws
  10299 for TRADES and 321 "Source price not supported with live updates" for AGGTRADES).

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
