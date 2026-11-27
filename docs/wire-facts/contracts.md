# Contracts wire facts

Contract details and option chains. Format: [README](README.md).

### Quarterly-only futures need `next_quarter()`, not `front_month()`

*Live 2026-11-03 22:39 ET off-hours, paper, server 226; #1149.*

- `Contract::futures("ES").front_month().on_exchange("CME")` asks for the current month
  (`ContractMonth::front` picks the current month through the 15th, then the next): November
  on 2026-11-03. ES lists only quarterly contracts, so `historical_schedules` drew error 200 "No
  security definition has been found for the request". `next_quarter()` (December) resolved.
- `Contract::futures("GC").front_month().on_exchange("COMEX")` resolved: GC lists nearby
  monthly contracts.
- **Not probed:** other quarterly-only products (NQ, ZN, 6E), and which month `front_month()`
  picks near a quarterly expiry.

### `Contract.lastTradeDate` (tag 21)

*Live 2026-10-26, server 226, contract_details; #1099.*

- Plain `YYYYMMDD` for OPT, FUT and FOP; absent for STK and BOND.
- The `YYYYMMDD HH:MM:SS Zone` form appears only in tag 4 `lastTradeDateOrContractMonth`
  (FUT/FOP).
- C# never encodes tag 21, so `ContractBuilder::last_trade_date` has no wire effect.

### `lastTradeDateOrContractMonth` (tag 4) on a future

*Live 2026-11-13 09:55 ET RTH, paper TWS, server 223; contract_details for ES 202612.*

- Tag 4 arrives as `20261218 08:30:00 US/Central`; `time_zone_id` is `US/Central` too.
- Sent back as the date alone or in that full form, `contract_details`, `head_timestamp`
  and a market-data snapshot all succeed: this build accepts both forms.
- **Paper Gateway accepts every form too** (*live 2026-11-14 14:15 ET off-hours, paper
  Gateway, server 226*): ES sent as `20261218 08:30:00 US/Central`, `20261218 08:30:00`,
  `20261218` and `202612` each resolved in `contract_details`, `head_timestamp`, a market-data
  snapshot and a what-if limit order (same margin for all four). No 10372.
- **Zone equals `time_zone_id`** (*live 2026-11-14 14:00 ET off-hours, paper Gateway, server
  226*): every contract carrying a zone in tag 4 matched its `time_zone_id`. FUT on CME (ES),
  NYMEX (CL), CBOT (ZN), CFE (VIX), EUREX (DAX, ESTX50 → `MET`), ICEEU (Z → `GB-Eire`),
  OSE.JPN (N225 → `Japan`), HKFE (HSI → `Hongkong`); FOP ES 202612 (6943 contracts); OPT SPX
  (`US/Central`) and SAP on EUREX. AAPL OPT on SMART sends the date alone (`20261218`), no time
  or zone. So dropping the zone loses nothing.
- *Live 2026-11-23, paper TWS, server 223:* the label still matches `time_zone_id` when the
  clock does not: AAPL options on CBOE carry New York times under `US/Central`
  ([last trade time](#last-trade-time-and-the-expiry-session)).
- **Not probed:** the gateway build behind server 226 (the handshake doesn't report it), so
  an NT adapter note of 10372 on Gateway 10.51 is neither confirmed nor ruled out; live
  (non-what-if) orders; bonds.

### `GB-Eire` last trade dates

*Live 2026-11-23 18:45 ET off-hours, paper TWS, server 223; contract_details.*

- ICE Europe sends tag 4 with a hyphenated zone: `20261218 16:30:00 GB-Eire` (VOD options,
  144 contracts), `20261218 10:15:00 GB-Eire` (Z 202612), `20270325 11:00:00 GB-Eire` (R
  202703), `20270315 10:00:00 GB-Eire` (I 202703). `time_zone_id` is `GB-Eire`, and an LSE
  stock (VOD, BARC, HSBA) carries it too.
- C#'s `SetLastTradeDate` splits on `-` whenever the text holds one, so it cuts these inside the
  zone name: date `20261218 16:30:00 GB`, time `Eire`. The decoder splits text holding
  whitespace on whitespace and keeps the `-` split for text without it.
- **Not probed:** whether TWS ever sends the `YYYYMMDD-HH:MM:SS` form the `-` split exists for.

### Open-order echo of `lastTradeDateOrContractMonth` (tag 4)

*Live 2026-11-14 weekend, paper Gateway, server 226; #1214.*

- Buy limits far from the market, read back with `open_orders`. ES on CME sent as `202612`,
  as `20261218` and as `20261218 08:30:00 US/Central` all came back as `20261218` (conid
  515416632, PreSubmitted). AAPL OPT sent as `20261218` came back as `20261218`.
- So TWS normalizes tag 4 to the last trading day: a contract sent as a month comes back as a
  date, and the open-order echo carries no time or zone. `Expiry` therefore doesn't promise the
  variant round-trips.
- **Not probed:** positions and portfolio (the account held no FUT/OPT), executions (need a
  fill), and live TWS.

### Last trade time and the expiry session

*Live 2026-11-23 18:45-19:00 ET off-hours (Thanksgiving week), paper TWS, server 223;
contract_details.*

| Contract | Exchange | Tag 4 | `timeZoneId` | `realExpirationDate` | Session on the expiry date |
| --- | --- | --- | --- | --- | --- |
| AAPL OPT weekly 20261127 | SMART | `20261127 13:15:00 US/Eastern` | `US/Eastern` | 20261127 | `0930-1315` |
| AAPL OPT weekly 20261127 | CBOE | `20261127 13:15:00 US/Central` | `US/Central` | 20261127 | `0830-1215` |
| AAPL OPT weekly 20261204 | SMART | `20261204 16:15:00 US/Eastern` | `US/Eastern` | 20261204 | not listed |
| AAPL OPT 20261218, 20270115 | SMART, CBOE | `20261218` (date alone) | `US/Eastern`, `US/Central` | same date | not listed |
| SPXW OPT 20261124 | CBOE | `20261124 15:00:00 US/Central` | `US/Central` | 20261124 | `0830-1500` |
| SPXW OPT 20261218 | CBOE | `20261218 15:00:00 US/Central` | `US/Central` | 20261218 | not listed |
| SPX OPT (AM) Dec 2026 | CBOE | `20261217 16:00:00 US/Central` | `US/Central` | 20261218 | not listed |
| ES FUT, ES FOP 202612 | CME | `20261218 08:30:00 US/Central` | `US/Central` | 20261218 | not listed |
| ES FOP weekly EW4 | CME | `20261127 12:00:00 US/Central` | `US/Central` | 20261127 | `1700-1200` |
| CL FUT 202701 | NYMEX | `20261221 14:30:00 US/Eastern` | `US/Eastern` | 20261221 | not listed |
| ZC FUT 202703 | CBOT | `20270312 12:01:00 US/Central` | `US/Central` | 20270312 | not listed |
| FESX, OESX 202612 | EUREX | `20261218 12:00:00 MET` | `MET` | 20261218 | not listed |
| FDAX 202612 | EUREX | `20261218 13:00:00 MET` | `MET` | 20261218 | not listed |
| N225 FUT 202612 | OSE.JPN | `20261210 15:45:00 Japan` | `Japan` | 20261211 | not listed |
| HSI FUT 202612 | HKFE | `20261230 16:00:00 Hongkong` | `Hongkong` | 20261230 | not listed |
| Z FUT 202612 | ICEEU | `20261218 10:15:00 GB-Eire` | `GB-Eire` | 20261218 | not listed |

- A time comes with every future, futures option, index option, EUREX, OSE, HKFE and ICE
  Europe contract seen, and with US equity weeklies. Standard monthly US equity options sent
  the date alone in this sweep; that is not a fixed rule (see the table below).
- The zone in tag 4 always equalled `timeZoneId`, but for US equity options the clock does not
  follow it: on CBOE, AAPL and MSFT 20261127 send `13:15:00 US/Central` while CBOE's session
  that day ends at `12:15` Central, and AAPL 20261204 sends `16:15:00 US/Central`. The time is
  the contract's New York time under the requested exchange's zone label. SPX on CBOE and
  SPXW on SMART carry Central times under `US/Central`.
- Zone names seen: `US/Eastern`, `US/Central`, `MET`, `GB-Eire`, `Japan`, `Hongkong`. All are
  legacy aliases, none a canonical IANA name.
- A contract's own `tradingHours` end on its expiry date when that date falls in the listed
  week, and that last session closes at the last trade time (SPXW 20261124 closes at 15:00
  against 16:00 on other days; AAPL 20261125 lists nothing after it). Later expiries have no
  session in the list, and a regular session's close is not the last trade time: ES trades to
  16:00 Central on other days but stops at 08:30 on its last day, CL at 14:30 against 17:00,
  FESX at 12:00 against 22:05.
- An AM-settled SPX option's tag 4 is its last trading day, a Thursday (20261217), and
  `realExpirationDate` holds the Friday. Querying it by 20261218 draws 200. N225 has the
  same shape (trades to 20261210, expires 20261211).
- AAPL OPT sessions on SMART run to 16:15 New York and its weeklies carry `16:15:00`: the
  time matches IB's session close. Compared with no exchange's published hours.
- **Not probed:** UK or HK stock options (VOD options on ICEEU answered; `700` on HKFE drew
  200), bonds, and whether a date-alone contract gains a time once its expiry enters the
  listed week.
*Live 2026-11-24 04:15-04:45 ET off-hours (Thanksgiving week), paper TWS, server 223;
contract_details per exchange, every contract of one expiry.*

The last trade time compared with the requested exchange's own `tradingHours` on an expiry
date inside the listed week (20261124, 20261125 or 20261127; 20261127 is a half day):

| Contract | Exchange | `lastTradeTime` | `timeZoneId` | `tradingHours` on the expiry date | Stated in |
| --- | --- | --- | --- | --- | --- |
| AAPL OPT 20261127 | SMART, AMEX | `13:15:00` | `US/Eastern` | `0930-1315` | New York |
| AAPL OPT 20261127 | PHLX, ISE, BOX, NASDAQOM, MIAX, BATS, EDGX, GEMINI, EMERALD, PEARL | `13:15:00` | `US/Eastern` | `0930-1300` | New York |
| AAPL, MSFT, TSLA, QQQ, IWM OPT 20261127 | CBOE | `13:15:00` | `US/Central` | `0830-1215` | New York |
| AAPL OPT 20261127 | CBOE2 | `13:15:00` | `US/Central` | `0830-1200` | New York |
| AAPL OPT 20261127 | PSE | `13:15:00` | `US/Pacific` | `0630-1000` | New York |
| SPY OPT 20261124 | CBOE | `16:15:00` | `US/Central` | `0830-1515` | New York |
| SPY OPT 20261124, 20261125 | SMART, AMEX | `16:15:00` | `US/Eastern` | `0930-1615` | New York |
| SPXW OPT 20261124 | CBOE, SMART | `15:00:00` | `US/Central` | `0830-1500` | Chicago |
| SPXW OPT 20261127 | CBOE | `12:00:00` | `US/Central` | `0830-1200` | Chicago |
| XSP, RUTW OPT 20261124 | CBOE | `15:00:00` | `US/Central` | `0830-1500` | Chicago |
| OEX, XEO OPT 20261127 | CBOE | `12:15:00` | `US/Central` | `0830-1215` | Chicago |
| VIXW OPT 20261130 | CBOE | `15:15:00` | `US/Central` | `0830-1515` | Chicago |
| NDXP OPT 20261124, 20261127 | SMART, ISE | `16:00:00`, `13:00:00` | `US/Eastern` | `0930-1600`, `0930-1300` | New York |
| ES FOP E4B, E4C, EW4 | CME | `15:00:00`, `15:00:00`, `12:00:00` | `US/Central` | ends `1500`, `1500`, `1200` | Chicago |
| 6E FOP 4EU 20261127 | CME | `09:00:00` | `US/Central` | ends `0900` | Chicago |
| ZC FOP 20261127 | CBOT | `12:00:00` | `US/Central` | `0830-1200` | Chicago |
| CL FOP 20261124, 20261127 | NYMEX | `14:30:00` | `US/Eastern` | ends `1430` | New York |
| GC FOP OG 20261124 | COMEX | `13:30:00` | `US/Eastern` | ends `1330` | New York |
| OESX, OEXP 20261124, 20261127 | EUREX | `12:00:00`, `17:30:00` | `MET` | ends `1200`, `1730` | `MET` |

- Every option whose underlying is a US stock or ETF (`underSecType` `STK`) states its last
  trade time in New York time on all 15 exchanges probed. CBOE and CBOE2 label it
  `US/Central` and PSE `US/Pacific`, so localizing it with `timeZoneId` puts the expiry one or
  three hours late.
  Everything else seen states it in `timeZoneId`: index options on CBOE (SPX, SPXW, XSP,
  RUT, OEX, XEO, VIX), Nasdaq index options, CME, CBOT, NYMEX and COMEX futures options, and
  EUREX. Options on non-US stocks state their exchange's zone (SAP on EUREX `17:30:00 MET`, RY
  on CDE `16:00:00 US/Eastern`, VOD on ICEEU `16:30:00 GB-Eire`); none was in the listed week.
- Compare with `tradingHours`, not `liquidHours`: on SMART, AAPL's liquid session on
  20261127 ends at `1300` while its trading session and last trade time end at `1315`.
  Exchange sessions do not pin the time either: most US option exchanges end the half day at
  13:00 New York against the contracts' `13:15:00`, and their regular liquid sessions at
  16:00 (AMEX 16:15).
- A date alone (no time) came for the standard monthly and quarterly options on US stocks and
  ETFs on every exchange (AAPL, TSLA, MSFT 20261218; SPY 20261218 and 20261231), none in the
  listed week, so no session can stand in for the time. This is not a monthly-versus-weekly
  rule: a contributor's captures of 2026-11-15 (AAPL calls on SMART) had the November monthly
  20261120 as `20261120 16:15:00 US/Eastern` and the 20261204 and 20261211 weeklies as the date
  alone, and 20261127 then carried `16:15:00`, not the `13:15:00` seen here a week later.
  Inferred: whether a time comes, and which, depends on how near the expiry is. The exchange sessions near them close
  at 16:00 or 16:15 New York depending on the exchange, which is the uncertainty a fallback
  would carry.
- An expired contract TWS still lists sends neither: VIXW 20261123, queried on 20261124,
  has empty `lastTradeTime` and empty `timeZoneId`. AAPL 20261120 drew 200.
- Reply times for one expiry, every strike: 0.2-14 s (SPY 20261231 on CBOE, 650 contracts,
  13.6 s; SPXW 20261124 on CBOE, 702 contracts, 1.6 s).
- **Not probed:** a date-alone expiry inside the listed week (the next is 20261218), options
  on US stocks quoted outside CBOE, CBOE2 and PSE in a non-Eastern zone, and index options
  listed on an exchange whose zone differs from the index's home.

### Bond contract details on a paper account

*Live 2026-11-25 19:16 UTC (US RTH), paper TWS, server 223; raw capture of `ContractData`. First
seen 2026-11-14 on paper Gateway, server 226; #1214.*

- `Contract::bond_cusip("912810TM0")` (UST 4 11/15/42) returns one contract (conid 598796519). Its
  contract message carries only the conid, `BOND`, strike 0, exchange `SMART`, local symbol
  `IBCID598796519` and trading class `US-T`. The details carry `minTick` 0.00001, order types,
  `validExchanges` `SMART`, `priceMagnifier` 1, `contractMonth` `204211`, time zone `US/Eastern`,
  trading and liquid hours, the CUSIP and ISIN in `secIdList`, `aggGroup` 7, `cusip` and
  `descAppend` `T 4 11/15/42`.
- TWS sends no symbol, currency, tag 4, coupon, maturity or issue date, so these fields are empty
  because TWS leaves them out, not because the decoder drops them. `912810TW8` (UST 4 3/4
  11/15/43) on 2026-11-14 had the same gaps. It looks permission-stripped.
- Re-querying by conid resolves. With conid 0 the query draws 200, even with the fields as
  returned. A `SecurityType::Bond` query by CUSIP without exchange and currency also draws 200;
  `bond_cusip` sets `SMART` and `USD`.
- `UKT`/GBP bonds: 190 contracts, all with tag 4 empty.
- So the zoned bond form (`"20430504 16:00 Europe/London"`, from C#'s `isBond` split) was not
  seen here, and whether a bond sent back with the date alone resolves is unanswered. The
  decoder leaves a bond's tag 4 as sent (`Expiry::Unknown` when zoned), as C# does, so nothing
  depends on the answer.
- **Not probed:** an account with bond permissions; the text protocol (the handshake requires
  server 213, where contract details are protobuf only).

### `tradingHours` and `liquidHours` sessions

*Live 2026-11-13 09:55 ET RTH, paper TWS, server 223; contract_details for ES 202612.*

- Both fields arrive as `;`-separated entries, each `YYYYMMDD:HHMM-YYYYMMDD:HHMM` or
  `YYYYMMDD:CLOSED`, covering about a week: `20261112:1700-20261113:1600;20261114:CLOSED;...`.
- The close carries its own date, so an overnight session (17:00 to 16:00) needs no rule. The
  pre-970 form (`20090507:0700-1830,1830-2330`) did not appear.
- `liquidHours` lists a closed day the full session still trades (`20261115:CLOSED` beside
  `20261115:1700-20261116:1600` in `tradingHours`) and can hold two sessions for one date
  (`20261118:0830-20261118:1600;20261118:1700-20261119:1600`).
- Times are wall clock in `timeZoneId` (`US/Central` here).
- *Live 2026-11-15 15:42 ET (Sunday), paper Gateway, server 226; AAPL STK SMART, EUR.USD CASH
  IDEALPRO, SPX OPT CBOE 20261120 (1014 contracts), SAP STK IBIS EUR:* every entry is one of the
  two forms; `TradingSession` parsed all of them. EUR.USD (both fields) and SPX `tradingHours`
  start with an empty entry (`;20261115:1715-20261116:1700;...`), so a parser must skip empty
  items. SPX `tradingHours` holds two sessions per date (`1915-0825` overnight, `0830-1600`);
  `timeZoneId` was `US/Eastern`, `US/Central` (SPX) and `MET` (SAP), not always IANA.
- **Not probed:** futures options, bonds, and products whose `timeZoneId` differs from the
  exchange's.

### Price magnifier units

*Live 2026-11-23 18:50-19:10 ET off-hours, paper TWS, server 223; contract_details,
option_chain, delayed-frozen market data, historical bars, calculate_option_price.*

`priceMagnifier` by product:

| Magnifier | Contracts |
| --- | --- |
| `100` | FUT ZC, ZS, ZW, ZL (CBOT), HE, LE (CME), KC, SB (NYBOT); FOP ZC; STK VOD, BARC, HSBA (LSE); OPT VOD (ICEEU); FUT and IND Z (ICEEU) |
| `1` | FUT ES, GC, CL, ZM, N225, HSI, FESX, FDAX, R; FOP ES; OPT AAPL, SPX, SAP, 9984; STK AAPL, 7203 (TSEJ), 700 (SEHK), SAP; CASH EUR.USD; IND SPX, N225; CFD AAPL |

No contract sent `0` or a negative value.

Units for the magnified contracts probed (ZC FUT 202703, ZC OPT OZC Jan 2027 450 C, VOD
STK LSE, VOD OPT ICEEU Dec 2026 100 C, Z FUT 202612):

| Value | Unit | ZC future / ZC option | VOD stock / VOD option |
| --- | --- | --- | --- |
| Market data bid, ask, last, close | sub-unit | `449` / `7.875` | `102.85` / `6.25` |
| Historical bar prices | sub-unit | `449` / `7.75` | `103` / error 200 with the details strike `1` |
| `option_chain` strikes | sub-unit | `360`, `365`, ... | `50`, `60`, ... |
| Strike TWS accepts in a request | sub-unit | `450` (`4.5` draws 200) | `120` (`1.2` draws 200) |
| Strike in contract details | currency | `4.5` | `1.2`, `1` |
| Option computation `optPrice` | currency | `0.07875` | `0.0525` |
| Option computation `undPrice` | currency | `4.4875` | `1.0293` |
| `calculateOptionPrice` underlying price | currency | `4.49` gives `0.0767`; `449` treats the option as deep in the money | not tried |
| `minTick` | currency (ZC's quarter-cent tick is `0.0025`) | `0.0025` / `0.00125` | `0.000001` / `0.0025` |

- Price × multiplier / magnifier gives the notional value: ZC `449` × `5000` / `100` =
  $22,450. Z on ICEEU quotes index points (`10524`) with magnifier `100` and multiplier `1000`,
  so its sub-unit is not a currency unit.
- The contract details return for the ZC option (conid 896736796, strike `4.5`) drew 200 from
  `historical_data`. The same contract with strike `450`, strike `0` or the contract id alone
  returned bars. A delayed market-data request accepted it with strike `4.5`.
- IB's documentation says the magnifier is not used for futures-option strikes from TWS 972.
  That holds only for the strike in contract details.
- **Not probed:** orders and executions (no orders placed), HKD and JPY products with a
  magnifier above 1 (none found), real-time market data (no CME or LSE subscription; values
  are delayed-frozen).

### No contract details for a BAG; futures spreads are BAGs

*Live 2026-11-23 17:26 ET off-hours, paper TWS, server 223.*

- `reqContractDetails` with `secType` `BAG` draws 321 "'BAG' isn't supported for contract data
  request", with or without legs, for an AAPL vertical and an ES calendar on CME or SMART, and
  with the combo placeholder con id. That con id alone (no `secType`) draws 200.
- IB lists no futures calendar as a contract of its own: `FUT` with local symbol `ESZ6-ESH7`
  draws 200, and none of the 134 CL rows on NYMEX has a spread local symbol. `matching_symbols`
  reports `BAG` among the ES index's derivative types. A calendar is a BAG of the two futures'
  con ids; see [orders](orders.md#combo-bag-shape-and-identity).
- **Not probed:** other futures exchanges (ICE, EUREX), and products IB might list as
  exchange spreads.

### Option chain `exchange` is a futures-options filter

*Live 2026-09-03, paper Gateway, server 220, `reqSecDefOptParams`; #792.*

| exchange | contract_id | Result |
|---|---|---|
| `""` | 265598 (AAPL) | 39 chains across 20 exchanges |
| `SMART` / `CBOE` / `NASDAQOM` | 265598 | clean end, zero chains |
| any | 0 | 321 "Invalid contract id" |
| `""` | 756733 (SPY) | 39 chains |

- The field is TWS's `futFopExchange`. Stock underlyings must leave it unset; a named exchange
  returns an empty chain, silently.
- `contract_id` is required; "0 for underlying" was wrong. Re-run 2026-11-23 18:43 ET
  (off-hours), paper TWS, server 223: AAPL with contract id 0 still drew `321 Error validating
  request.-'ao' : cause - Invalid contract id`. `option_chain` now returns
  `Error::Validation(ValidationError::InvalidField { .. })` for `contract_id` 0 without sending.
- Same run: ES futures options on `CME` (underlying ESZ6, contract id 515416632) returned a
  chain with trading class `ES`.
- **Not probed:** futures options on other exchanges.

### No contract details for a BAG

*Live 2026-11-23 Mon 18:43 ET (off-hours), paper TWS, server 223.*

- `reqContractDetails` with `secType` `BAG` (AAPL, `SMART`, `USD`), with one combo leg and with
  none, drew `321 Error validating request.-'y' : cause - 'BAG' isn't supported for contract
  data request. Please enter a valid security type` both times, and no rows.
- `contract_details` and `contract_details_stream` now return `Error::InvalidArgument` for a
  `SecurityType::Spread` contract without sending.
- The [earlier capture](#no-contract-details-for-a-bag-futures-spreads-are-bags) also tested
  an ES calendar on CME and SMART, and the combo placeholder contract ID without `secType`.
- **Not probed:** BAGs on other exchanges.

### `f64::MAX` strike is read as unset in contract details

*Live 2026-11-23 Mon 18:43 ET (off-hours), paper TWS, server 223.*

- The Python client marks an unset strike with `UNSET_DOUBLE` (`sys.float_info.max`); this crate
  encodes any non-zero strike, so `f64::MAX` goes on the wire as a strike.
- `reqContractDetails` for AAPL `OPT` 20261127 calls on `SMART` returned the same 78 rows
  (strikes [110, 425]) with strike `0.0` (not sent) and with `f64::MAX`. ES `FOP` 20261218
  puts on `CME`, trading class `ES`, returned the same 359 rows (strikes [100, 12000]) either way.
- Settles leaving `f64::MAX` as it is: TWS already treats it as unset in this request.
- **Not probed:** `f64::MAX` on orders, market data, or other requests that carry a contract.

### `underSecType` is a security type code

*Live 2026-11-16 Mon 02:30 ET (US equities closed, CME Globex open), paper, server 223.*

- `reqContractDetails`: AAPL stock and SPX index send none (empty); the ES future sends `IND`;
  680 AAPL options for 202612 all send `STK`; 7582 ES futures options for 202612 all send `FUT`.
- Settles typing `ContractDetails::under_security_type` as an optional `SecurityType`.
- **Not probed:** bonds, warrants, CFDs, forex, and futures on other underlyings.

### `secType` is optional with a `conId`, required without one

*Live 2026-11-27 Fri 11:15–12:35 ET (US RTH, half day), paper, server 226; #1330, #1353.*

- `reqContractDetails` by `conId` alone (exchange and currency empty) for AAPL stock, the ESZ6
  future, a SPY option, SPX and EUR.USD, with `secType` `STK`, absent, or the contract's own type:
  each returned the one contract, with its real security type.
- `reqHeadTimestamp` by `conId` and exchange for the same five, with the same three `secType`
  values: the same timestamp for each. One EUR.USD call with `STK` returned a different one; the
  rerun matched.
- Without a `conId`, an absent `secType` is error 321 "Please enter a valid security type": AAPL
  (with `SMART`/`USD`, and with neither), ES on `CME`, SPX. A `localSymbol` alone is no
  different: ESZ6 (with and without `CME`), AAPL on `SMART`/`USD`, EUR.USD on `IDEALPRO`, and a
  symbol with a primary exchange are each 321.
- A security id needs no `secType`: AAPL's ISIN `US0378331005` on `SMART`/`USD` returned the
  stock, and with no exchange or currency 46 rows, all with their types. A CUSIP bond with no type
  was 200 (not found), not 321.
- With its type, a `localSymbol` needs nothing else that names the contract: ESZ6 (`FUT`, with
  and without `CME`) and ESH7 (`FUT`, `CME`) with no contract month; `SPY   270115C00580000`
  (`OPT`, with and without `SMART`) and `ESZ6 C7000` (`FOP`, `CME`) with no strike, right or
  expiry. Each resolved its contract. `ESZ6 C7000` with no exchange timed out at 40s (#1353).
- A symbol with no month or expiry is an open query: ES `FUT` on `CME` returned all 21 futures;
  SPY `OPT` on `SMART` timed out at 40s (#1353).
- None of the 622 contracts these requests returned lacked a `secType`.
- Settles that an unset `Contract::security_type` sends nothing: TWS resolves a `conId` without
  it and rejects a symbol lookup loudly, with no silent fallback to `STK`.
- `ContractBuilder::build` follows this: a type is required unless a contract id or security id
  identifies the contract, and a contract id or local symbol waives the month, strike, right and
  expiry (#1353).
- **Not probed:** orders, market data, scanners; a `conId` with a wrong type other than `STK`;
  a CUSIP that resolves; warrants, CFDs.

### Matching-symbols `derivativeSecTypes` are security type codes

*Live 2026-11-22 Sun 23:54 ET (markets closed), paper, server 226; #1261.*

- `reqMatchingSymbols` for 30 patterns (AAPL, SPX, ES, EUR, BTC, GOLD, SAP, DAX, ZN, CL, HSI,
  7203, and others): 1330 descriptions across STK, IND, BOND, FUND, CASH, CMDTY and CRYPTO.
  953 carry an empty list; the rest carry only `BAG`, `CFD`, `EC`, `FOP`, `FUT`, `IOPT`, `OPT` and
  `WAR`. No entry was an empty string.
- `IOPT` appeared on 93 descriptions (68 STK, 23 IND, 2 CASH); `EC` on 12, all IND, among them
  event-contract indices such as `KXBTCY` and `KXGOLDD`. #1261 models both as `SecurityType`
  variants.
- Settles typing `ContractDescription::derivative_security_types` as `Vec<SecurityType>`.
- **Not probed:** patterns outside this list; only one session; whether TWS accepts `IOPT` or `EC`
  as a request contract's `secType`.

### Option chain expirations are `YYYYMMDD` dates

*Live 2026-11-16 Mon 02:30 ET (US equities closed, CME Globex open), paper, server 223; #1253.*

- `reqSecDefOptParams` for AAPL (22 chains), SPX (9 chains) and ES futures options on `CME`
  (21 chains, one per trading class such as `EW1`, `E3B`): every `expirations` entry is an
  8-digit `YYYYMMDD` date, weeklies and dailies included.
- Settles typing `OptionChain::expirations` as dates. No contract month (`YYYYMM`) form appeared.
- **Not probed:** bond or warrant chains; other futures roots.

### Listing option contracts: `option_chain` against one `contract_details` query

*Live 2026-11-24 04:50-05:20 ET off-hours, paper TWS, server 223; `reqSecDefOptParams` and
`reqContractDetails` with no strike.*

| Underlying | `option_chain` | Contracts from `contract_details` |
| --- | --- | --- |
| AAPL (265598) | 22 chains, one per exchange (SMART, CBOE, ... IBUSOPT), each 24 expiries x 133 strikes | 3,546 on SMART (49 s), 3,546 on CBOE (62 s); 980 for 202612 (0.8 s); 156 for 20261127 (0.4 s) |
| SPX (416904, IND) | 9 chains: SPX, SPXW and SPXO each on SMART, CBOE and IBUSOPT; SPXW 38 x 843 | 33,502 on SMART (72 s); 23,248 for class SPXW (38 s); 13,156 for 202612 (20 s) |
| ESZ6 (515416632, CME) | 19 chains, one per trading class (ES, EW, EW1-EW4, E1A-E4C), 1 expiry each | `FOP` ES on CME: 25,847 over 13 underlying futures (66 s); 11,583 for 202612 over ESZ6 and ESH7 (4.9 s) |
| ZCH7 (748101140, CBOT) | 12 chains, strikes `360`, `365`, ... | `FOP` ZC 202612: 776 over ZCH7 and ZCZ7, strikes `3.6` to `5.85`, magnifier 100 |
| VOD (ICEEU) | not run | 2,140 options, 60 expiries (41 s) |

- `option_chain` answers in 2 ms to 1.2 s with expiry and strike lists. Their combinations
  far exceed the listed contracts: AAPL's 24 x 133 x 2 = 6,384 per exchange, against 3,546
  real ones. The chain also lists the expiry that ended the day before (20261123).
- A `contract_details` query with symbol, security type, exchange and currency and no strike
  returns every listed contract, each once, when the exchange is named. With an empty exchange,
  AAPL 20261127 came back 3,432 times for 156 contracts: once per listing exchange.
- The query contract's security type follows the underlying: `OPT` for a stock, ETF or index,
  `FOP` for a future, with the future's exchange.
- A futures-option query returns options on several futures. Neither a date nor a month
  narrows it to one: ZC 20261224 returned 266 `OZC` options on ZCH7 and 204 `OCD` options on
  ZCZ7. Select one future's options by `underConId`.
- Expiry narrows to one date or one month. For `OPT` the month is the expiry month (SPX
  202612: 20261201 to 20261231). For `FOP` it is the option's contract month: `OZC` January
  2027 options expire 20261224, `202701` with class `OZC` returned 266, and `202612` with class
  `OZC` drew 200.
- Without an expiry a reply takes 40 to 75 s, and the rows can all come at the end: AAPL on
  CBOE sent its first row at 61 s of 62, SPX on SMART at 49 s of 72 (AAPL on SMART at 2.7 s
  of 49).
- Right narrows: AAPL calls only returned 1,773 of 3,546. A strike narrows to that strike, and
  one not listed draws 200 (AAPL 20261127 267.5).
- Strikes in contract details are in currency units, and in `option_chain` they are in the
  sub-unit for a magnified product (ZC `3.6` against `360`, `priceMagnifier` 100).
- **Not probed:** pacing of many back-to-back per-strike requests (not needed by the single
  query), queries during market hours, and chains of more than one currency.

### Market rule ids align with valid exchanges

*Live 2026-11-25 19:13-19:16 UTC (US RTH; TSE, SEHK and LSE closed), paper TWS, server 223;
`contract_details`, `market_rule` and orders. First seen 2026-11-10 on paper Gateway, server
226; #1198.*

- `contract_details` for STK (AAPL SMART and any exchange, SAP@IBIS, 7203@TSEJ), FUT (ES,
  GC), OPT (SPY 20261120), FOP (ES 202612), CASH, CRYPTO, IND, CFD, CMDTY and FUND on
  2026-11-10: 5,834 rows, 16,598 `marketRuleIds` tokens. Every token is a plain integer: no
  whitespace, no empty entries.
- In every row the id count equals the `validExchanges` count: id *i* is the rule for exchange
  *i*, as IB's `ContractDetails` doc says. The 2026-11-25 run matched in all 1,084 rows of the
  contracts below. Usually all ids are equal (AAPL: 4563 ×27); some contracts mix them (SAP@IBIS:
  1874, 1943, 3055; 7203: 3407 on SMART and JPNNEXT, 1460 on TSEJ; LSE stocks: 1918, with 3060 on
  EUIBSI).
- TWS checks an order's price against the rule of the exchange the order is sent to, and SMART
  has its own rule. 7203 BUY 100 LMT 2000.1 on SMART was accepted (`PreSubmitted`, with a 399
  "will not be placed at the exchange until 2026-11-26 09:00:00 Japan"); the same order on TSEJ
  drew 110 "The price does not conform to the minimum price variation for this contract." Rule
  3407 has a 0.1 increment below 10,000 JPY; rule 1460 has 0.5 above 1,000.
- `minTick` equals the increment of the first band of the contract's rules in every row: AAPL
  0.01, CTXR 0.0001, 7203 0.1, DGE and VOD 0.000001, 700 and 5 0.001, ES 0.25, ZC 0.0025, CL
  0.01, NG 0.001, GC 0.1, and 718 ES options 0.05. It is not the increment at the contract's
  price.

| Rule                | Contracts                             | Bands (low edge: increment)              |
| ------------------- | ------------------------------------- | ---------------------------------------- |
| 4563                | AAPL                                  | 0: 0.01                                  |
| 557                 | CTXR (US stock below 1 USD)           | 0: 0.0001, 1: 0.01                       |
| 67, 151, 32, 98, 65 | ES, ZC, CL, NG, GC futures            | one band: 0.25, 0.0025, 0.01, 0.001, 0.1 |
| 235                 | ES options (FOP, 718 rows for 202612) | 0: 0.05, 5: 0.1, 20: 0.25, 100: 0.5      |

- **Not probed:** bonds, warrants, structured products; contracts with no market rule; an order
  on JPNNEXT.

### Event-contract, `minAlgoSize` and precision fields

*Live 2026-11-15 Sun 23:52 ET (off-hours), paper, server 226; raw capture of `ContractData`; #1251.*

| Contract | `eventContract1` / `Description1` / `Description2` | `minAlgoSize` | `lastPricePrecision` / `lastSizePrecision` |
|---|---|---|---|
| AAPL STK | absent | `"0"` | `"0.000001"` / `"0.000001"` |
| ES FUT (front month) | absent | `"0"` | absent |
| EUR.USD CASH | absent | `"0"` | absent |
| BTC CRYPTO (PAXOS) | absent | `"0"` | absent |
| FF OPT on FORECASTX (62 rows) | `"US Fed Funds Target Rate"` / `"FF Jan28'27 3.125"` / `"Will the US Fed Funds Target Rate be set above 3.125% at the FOMC meeting ending January 27, 2027?"` | `"0"` | absent |

- The event-contract strings are present and meaningful only on event contracts.
- `minAlgoSize` was `"0"` on every row, including AAPL, which takes algo orders, so no
  contract probed carried a usable value.
- The precision fields are steps (`0.000001`), not digit counts, and only AAPL sent them.
  `TickReqParams` matched ([market data](market-data.md#tickreqparams-precision-fields)).
- **Not probed:** equity and index options, bonds, funds, non-US stocks; a contract with a non-zero
  `minAlgoSize`; what the AAPL step governs (sub-penny prices and fractional shares are both
  smaller than its `minTick` 0.01 and `minSize` 0.0001).

### An empty currency lets TWS resolve it

*Live 2026-11-24 off-hours, paper TWS, server 223.*

`contract_details` with the currency empty, then set:

| Contract | Empty | `USD` | Native |
| --- | --- | --- | --- |
| FUT HSI HKFE 202612 | 1 row, HKD | 200 | HKD: 1 row |
| FUT `localSymbol` HSIZ6 HKFE | 1 row, HKD | 200 | not run |
| FUT ES CME 202612 | 1 row, USD | 1 row | = USD |
| CONTFUT HSI HKFE / ES CME | 1 row, HKD / USD | 200 / 1 row | HKD: 1 row |
| FOP HSI HKFE 202612 C | 127 rows, HKD | 200 | HKD: 127 rows |
| FOP ES CME 20261218 6500 C | 2 rows (ES, EW3), USD | 2 rows | = USD |
| OPT AAPL SMART 20261218 250 C | 1 row, USD | 1 row | = USD |
| IND HSI HKFE / DAX EUREX | 1 row, HKD / EUR | 200 | not run |
| IND SPX CBOE | 1 row, USD | 1 row | = USD |
| STK 700 SEHK | 1 row, HKD | 200 | HKD: 1 row |
| STK BP, TM SMART | 1 row, USD | 1 row | = USD |
| STK AAPL SMART | 3 rows: USD, CHF (EBS), CAD (TSE) | 1 row | = USD |
| STK SAP, BHP SMART | 4 rows each, one per currency | 1 row | - |
| STK SHEL, VOD, RY SMART | 2-3 rows, one per currency | 1 row | - |
| CASH EUR IDEALPRO | 27 rows, one per quote currency | 1 row | - |

- Market data snapshot with an empty currency: HSI future streams (HKD); AAPL, SAP and EUR
  CASH fail with 200 "The contract description specified for AAPL is ambiguous; you must
  specify the currency". What-if orders behave the same: HSI future accepted, AAPL, SAP and
  EUR CASH ambiguous (200), HSI future in USD 200.
- A contract id resolves whatever the currency: the HSI future's id with `USD` returns the HKD
  contract.
- `Contract::futures("HSI").on_exchange("HKFE")` and `Contract::stock("700").on_exchange("SEHK")`
  keep the builders' USD and draw 200; adding `in_currency("HKD")` resolves them.
- **Not probed:** RTH, bonds, warrants, combos (BAG), `Contract::news`, and placing a real order.

### Local symbols of HKFE, OSE and KSE options

*Live 2026-11-24 21:47-21:51 UTC (HKFE, OSE and KSE closed) and 2026-11-25 06:21 UTC (SEHK
options), paper TWS, server 223; contract details of every listed contract.*

| Query                   | Rows  | `localSymbol`                                            | `tradingClass` |
| ----------------------- | ----- | -------------------------------------------------------- | -------------- |
| OPT HSI HKFE            | 2,572 | `HSI25600L6`, `HSI25600X6`, `HSI19200L6W04`              | `HSI`          |
| OPT MHI HKFE            | 866   | `MHI12800L6`, `MHI12800X6`                               | `MHI`          |
| FOP HSI HKFE            | 1,942 | `PHS25600L6`, `PHS25600X6`                               | `PHS`          |
| OPT N225 OSE.JPN 202612 | 706   | `131126218` (16250 P), `141127718` (17750 C)             | `225`          |
| OPT K200 KSE 202612     | 1,134 | `K200270L6.KS`, `K200992L6.KS` (992.5 C), `K200220X6.KS` | `01`           |
| OPT 700 SEHK 202612 C   | 139   | `TCH DEC26 640 C`                                        | `TCH`          |

- HKFE and KSE: the trading class on HKFE or the symbol on KSE (`K200`; its trading class is
  `01`), strike, one letter for month and right, last digit of the expiry year, then `.KS` on KSE. Calls take `A` (January) to `L` (December), puts `M` to
  `X`. All 5,380 HKFE rows matched the contract's own strike, right and expiry month and year.
  KSE drops a strike's decimals: `K200992L6.KS` is the 992.5 call.
- The year is one digit: `HSI19200L6` expires in 2026 and `HSI19200L0` in 2030, so the symbol
  alone does not give the decade.
- The one HSI weekly series listed, expiring 2026-12-04, carried `W04` after the year
  (`HSI19200L6W04`). Whether the digits are the expiry day or a week number is not settled by a
  single series.
- FOP HSI (class `PHS`) is an option on the HSI future: `underSymbol` `HSIZ6`, expiring
  2026-12-18, where OPT HSI (class `HSI`) is on the index and expires 2026-12-30. FOP MHI and OPT
  700 on HKFE drew 200.
- OSE.JPN local symbols are 9-digit exchange codes. Seen: the second digit is `3` or `8` for
  puts and `4` or `9` for calls; the last two are `18` (also on the N225 future, `161120018`;
  the N225 mini future ends in `19`). The middle digits change with the strike range. Read the
  strike, right and expiry from the contract fields; the code does not spell them.
- SEHK stock options list under the stock's symbol on `SEHK` (OPT 700 on HKFE drew 200), with a
  short class (`TCH` for 700), a spelled-out local symbol `TCH DEC26 640 C` and multiplier 100.
- **Not probed:** KSE weekly options, OSE weekly options and other OSE products.

### Board lots: `min_size` and `size_increment`

*Live 2026-11-24 21:47-22:23 UTC (TSE, SEHK and LSE closed), paper TWS, server 223.*

| Contract                                                | `minSize` | `sizeIncrement` | `suggestedSizeIncrement` |
| ------------------------------------------------------- | --------- | --------------- | ------------------------ |
| 7203 TSEJ (Toyota)                                      | 100       | 100             | 100                      |
| 700 SEHK (Tencent)                                      | 100       | 100             | 100                      |
| 5 SEHK (HSBC)                                           | 400       | 400             | 400                      |
| VOD LSE                                                 | 0.0001    | 0.0001          | 1                        |
| HSI, N225, N225M, K200 futures; HSI, K200, N225 options | 1         | 1               | 1                        |

- The board lot is `minSize`, and `sizeIncrement` equals it on TSE and SEHK. It differs per
  stock on SEHK (100 for 700, 400 for 5).
- An order below it is rejected before TWS creates it: 700 BUY 50, 7203 BUY 50 and 5 BUY 100 drew
  388 "Order size 50 is smaller than the minimum required size of 100." (400 for 5).
- A size above the lot but not a multiple of it is discarded with 461: 700 BUY 150 and 7203
  (SMART) BUY 150 drew "Order size 150 is not correct; it should be a multiple of 100".
- **Not probed:** SEHK and TSE odd-lot order types; a modify to an odd size.

### Tick-size bands of TSE, SEHK and LSE stocks

*Live 2026-11-24 21:47 and 22:03-22:10 UTC (all three closed), paper TWS, server 223;
`market_rule`, orders and what-if orders. LSE units and bars: 2026-11-25 19:16 UTC (LSE closed),
historical ticks and bars.*

| Contract (exchanges)                                           | Rule             | Bands (low edge: increment)                                                 |
| -------------------------------------------------------------- | ---------------- | --------------------------------------------------------------------------- |
| 7203 (TSEJ)                                                    | 1460             | 0: 0.1, 1,000: 0.5, 3,000: 1, 10,000: 5, 30,000: 10, ... 30,000,000: 10,000 |
| 7203 (SMART, JPNNEXT)                                          | 3407             | 0: 0.1, 10,000: 0.5, 30,000: 1, 100,000: 5, 300,000: 10, 1,000,000: 100     |
| 700, 5 (SEHK)                                                  | 1910, 1852       | 0: 0.001, 0.25: 0.005, 10: 0.01, 20: 0.02, 50: 0.05, 100: 0.1, ... 5,000: 5 |
| VOD (LSE and 5 others; EUIBSI 3060)                            | 1918             | 0: 0.000001, ... 1: 0.0005, 2: 0.001, ... 500: 0.2                          |
| HSI future and options; N225 future; N225M future; K200 future | 68, 197, 380, 11 | one band: 1, 10, 5, 0.05                                                    |

- `minTick` is the first band's increment (7203: 0.1, SEHK: 0.001), not the increment at the
  stock's price. One stock can carry different rules per venue: 7203's TSEJ rule moves to 0.5 at
  1,000 JPY, its SMART rule at 10,000.
- Off-tick prices drew 110 "The price does not conform to the minimum price variation": 700 LMT
  320.01 (HKD band 0.2 above 200), 7203 TSEJ LMT 2000.1 (band 0.5 above 1,000).
- LSE rules are in pounds while LSE stocks are quoted and ordered in pence (`priceMagnifier` 100,
  currency `GBP`; IB sends no `GBX`). Multiply a band's edges and increment by `priceMagnifier`
  before comparing them with an order or market-data price.
- Diageo (DGE, rule 1918) settles the unit: on 2026-11-25 (LSE closed, historical data), 9,066
  historical bid/ask ticks on LSE, from nine 1,000-tick windows over the session, quoted 18,132
  prices in [1921, 1946.5] pence. Every one sits on a 0.5 pence grid, which is rule 1918 read in
  pounds (GBP 19.21 is in the 10: 0.005 band). Read in pence, the rule gives 0.2 pence (the 500:
  0.2 band), and 9,453 half-pence prices such as 1921.5 are off that grid.
- VOD cannot tell the readings apart: at about 100 pence both give 0.05 pence (bands 1: 0.0005 and
  100: 0.05). A what-if VOD BUY 100 LMT 100 cost commission 3.5 (stamp duty on GBP 100 plus the
  GBP 3 minimum); LMT 1.0 cost 3.005 (GBP 1). LMT 100.05 was accepted and LMT 100.01 and 100.001
  drew 110.
- Historical bars are not book prices: half of DGE's 1-minute bar prices that day were off the
  0.5 pence grid (TRADES 1,012 of 2,040 open, high, low and close values, BID 1,082, ASK 1,027;
  for example BID 1941.4).
- **Not probed:** an LSE order or fill (what-if only); real-time quotes (no TSE permission: 162
  "No market data permissions for TSEJ STK").

### Local symbols of TSE, KRX and EUREX contracts

*Live 2026-11-25 19:15-19:23 UTC (TSE, KRX and EUREX closed), paper TWS, server 223;
`contract_details` and `matching_symbols`.*

| Contract                           | Query                 | `localSymbol`     | Notes                                       |
| ---------------------------------- | --------------------- | ----------------- | ------------------------------------------- |
| Toyota 7203                        | STK on TSEJ or SMART  | `7203.T`          | primary `TSEJ`                              |
| Toyota 7203                        | STK on JPNNEXT        | `7203`            | same conid 13905804                         |
| Samsung Electronics 005930 (KOSPI) | by conid              | `005930.KS`       | primary `KRX`, exchange `SMART`             |
| Ecopro BM 247540 (KOSDAQ)          | by conid              | `247540.KQ`       | primary `KRX`                               |
| DAX future, December 2026          | FUT `DAX` on EUREX    | `FDAX 20261218 M` | class `FDAX`; the mini is `FDXM 20261218 M` |
| Euro Stoxx 50 index                | IND `ESTX50` on EUREX | `SX5E`            | IND `SX5E` draws 200                        |

- A TSE stock's local symbol depends on the exchange in the query: `7203.T` on TSEJ and SMART,
  `7203` on JPNNEXT.
- Korean stocks list under primary exchange `KRX`, with `.KS` for KOSPI and `.KQ` for KOSDAQ
  (also `005935.KS`, `068270.KS`, `068760.KQ`, `086520.KQ`). A query on `KSE` with the stock code
  and `KRW` draws 200: `KSE` lists the derivatives.
- EUREX futures local symbols are the trading class, the last trading day and `M`. A query by
  trading class `FDAX` and contract month 202612 resolves with symbol `FDAX` or `DAX`; with no
  symbol it draws 321 "The symbol or the local-symbol or the security id must be entered".
- IB's symbol for the Euro Stoxx 50 index is `ESTX50`; its local symbol is `SX5E`.
- **Not probed:** other EUREX products; Korean preferred shares beyond `005935`.

### KSE contracts report the `Japan` time zone

*Live 2026-11-24 21:47 and 22:08 UTC, paper TWS, server 223.*

- `contract_details` for the K200 future and K200 options on KSE: `timeZoneId` `Japan`, trading
  hours `20261124:1800-20261125:0600;20261125:0845-20261125:1545`. Korea and Japan share UTC+9, so
  the hours are right in either zone, but a caller mapping `timeZoneId` to a market calendar gets
  Japan's.
- A historical K200 request echoed its window in `+09:00`.
- **Not probed:** KSE stocks (005930 on KSE drew 200).
