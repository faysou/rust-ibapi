# Orders wire facts

Placement, echoes, status, executions and completed orders. Format: [README](README.md).

### `transmit` absent means held

*Live 2026-09-23, paper Gateway, server 220; #841 (closed, not a bug).*

- A placeOrder proto with `transmit` left out (what `some_bool(false)` produces): TWS holds the
  order. permId assigned, no OrderStatus, absent from open_orders; cancel returns 202 "Order was
  discarded" + 161. Control with `transmit=true` goes PreSubmitted → Submitted.
- The official clients (`EClientUtils` in C#/Java/Python/C++) also set `transmit` only when
  true. Our decoder's `unwrap_or(true)` applies to inbound messages only and says nothing about
  how TWS reads an absent flag in outbound ones.
- An untransmitted order gets no open-order or status frames, so a TWS-derived value (e.g. a
  VOL order's limit) is only visible once transmitted.

*Live 2026-11-25 19:12 UTC (US RTH), paper TWS, server 223.*

- An AAPL BUY 1 LMT with `transmit=false`, then an unrelated AAPL BUY 1 LMT with `transmit=true`
  on the same client: the second went `PreSubmitted` and `Submitted`; the held one stayed held.
  Eight seconds later it had still sent no frame on its own stream and `all_open_orders` did not
  list it. Only a child that names it as `parent_id` releases a held order (see
  [Bracket child quantity](#bracket-child-quantity-follows-the-parent)). Its cancel drew
  `Cancelled`, 202 "Order was discarded" and 161, as above.

### Attached orders (`attached_orders`)

*Live 2026-09-27, paper Gateway, server 220; #842.*

- `slOrderType` / `ptOrderType`: only `"PRESET"` accepted. `STP`, `LMT`, `TRAIL`, `""` or junk
  → 320 "Invalid value for Stop Loss order-id or order-type" (text always says Stop Loss).
- Id without type, or type without id → 320. Each leg needs both.
- After a 320 the parent is never created.
- `PRESET` with no preset defined → 10355 "Cannot auto-attach … Preset is not defined.", then
  OrderStatus Cancelled + 202 "Order was discarded". The id stays consumed (103 on re-send).
- `transmit=false` does not defer the preset check.
- **Not probed:** child frames and parent-cancel behavior. The paper account has no presets;
  they are defined in TWS Global Config → Presets, and Gateway has no UI for them.

### `adjustable_trailing_unit`

*Live 2026-10-04, paper Gateway, AAPL parent LMT + attached adjustable STP → TRAIL; #901.*

- 0 (amount) and 100 (percent) accepted and echoed unchanged.
- 1 → 201 "Order rejected - Invalid value in field # 6269". 50 → 364 "Invalid Adjusted
  Trailing Amount".
- C# `Order.cs` documents percent as 1; wrong. The OrderSamples comments (100) are right.

### What-if and additional order params

*Live 2026-10-27 RTH, paper Gateway, server 226, SPY BUY LMT SMART DAY; #1065.*

- `what_if_type` without `what_if` → 321 "What-If type specified but What-If flag is not set";
  nothing placed. (Docs shipped in 5.3.0 said the order is placed live; corrected.)
- None / FullOrderCheck(0) / MarginRequirements(1) all echo `what_if_type =
  MarginRequirements` with the same OrderState. No observable effect on a stock what-if. The
  encoder does send 0.
- `post_only`, `allow_pre_open`, `seek_price_improvement` echo back as sent.
- `ignore_open_auction=true` echoes as false (sent, but not reported for SMART DAY LMT).
- `seek_price_improvement` unset → open order reports `Some(false)`: TWS sends an explicit
  false.

### IBKRATS PEG BEST / PEG MID

*Live 2026-10-04 (Sunday), paper Gateway, AAPL on IBKRATS, BUY 100 @ $1; #936.*

- Gateway first blocked every IBKRATS order with 10311 "This order will be directly routed to
  IBKRATS". `advanced_error_override="10311"` did not bypass it; changing the gateway's API
  precaution setting did.
- `not_held: false` → status Inactive, no error or warning. Required.
- `min_trade_qty` (1 or 100, the C# sample value) → 10302 "Min trade quantity is not allowed".
- Accepted (PreSubmitted) without `min_compete_size`, `compete_against_best_offset`, mid
  offsets (also with the +inf up-to-mid sentinel), `limit_price`; PEG MID with a stray
  `aux_price = 0.0` too.
- PEG BEST on SMART with `not_held` → 10297 "Not Held attribute is invalid for this order".
- **Caveat:** weekend PreSubmitted is not validation at the open.

### VOL order volatility is percent

*Live 2026-10-27 RTH, paper Gateway, server 226, SPY 20261030 calls; #1112.*

- BUY VOL 0.1 Annual on C770 (SPY ~768) → TWS-derived limit 0.01. A 10% vol would price ~2.12,
  which implies 0.1 means 0.1%.
- Contrast `calculate_option_price`, which takes a fraction: [market-data](market-data.md#option-computation-volatility-is-a-fraction).

### Combo (BAG) orders

*Live 2026-10-29, paper Gateway, server 226, AAPL options, what-if via `.analyze()`; #1123,
#1131, #1132. Guard: `integration/*/tests/spreads.rs`.*

- BAG with no `symbol` → 321 "The symbol or the local-symbol or the security id must be
  entered". Hence `Contract::spread(symbol)` requires it.
- Iron condor bought to open is a credit: Buy @ -1.00 accepted. Buy @ +1 → 201
  "Guaranteed-to-Lose"; Sell @ +1 → 201 "Riskless". TWS rejects mis-signed combo prices.
- Collar (STK leg ratio 100 + long put + short call) accepted, limit ≈ 100 × spot. Unlike
  option-only combos, no 201 bounds: prices below the put floor and above the call cap were
  all accepted.
- What-if answers outside RTH; all 8 `SpreadBuilder` presets PreSubmitted at 23:55 ET.
- *Live 2026-11-15 weekend (Sunday 10:00 ET), paper Gateway, server 226; what-if AAPL
  20261218 C250 / C260 call spread:* the second leg's ratio omitted (`0`)
  → 321 "The combo details for leg '1' are invalid. -  conid, ratio, side: 692807836, 0, 2";
  both ratios 0 → the same for leg '0'. Ratios 1/1 gave a normal what-if reply. TWS does not
  read a missing ratio as 1.

### `use_price_mgmt_algo` / `route_marketable_to_bbo` echo and acceptance

*Live 2026-11-05 off-hours (23:43 ET), paper Gateway, server 226; AAPL BUY 1 LMT 1.00; #1191.*

- Sent each field absent / 0 / 1, one at a time. Every `openOrder` echo carried both fields
  set: absent and 0 both came back as 0, so `decode_order` yields `Some(_)` and an echo
  never shows `None`.
- `use_price_mgmt_algo`: absent and 0 accepted (PreSubmitted, echo 0); 1 accepted (echo 1).
  This account's default is off.
- `route_marketable_to_bbo`: absent and 0 accepted (echo 0); 1 rejected with 10353
  ("...can result in higher total cost..."), "Restriction is specified in Precautionary
  Settings of Global Configuration/Presets"; the order was discarded (cancel drew 201).
- **Not probed:** an account whose default has either feature on (does 0 override it?);
  `route_marketable_to_bbo=1` with the precautionary setting relaxed; RTH routing effect;
  live TWS (Gateway only); servers below 226.

### OrderStatus `filled` / `remaining` always sent

*Live 2026-10-26, paper Gateway, EUR.USD; #1089.*

- Present in every state: PreSubmitted, Submitted, Cancelled carry `filled="0"`; Filled
  carries `remaining="0"`. TWS does not drop zero decimal strings, so both decode as required.

### Completed order `totalQuantity` is the unfilled quantity

*Live 2026-10-27, server 226, raw capture of CompletedOrder frames; #1117.*

| Order | totalQuantity | filledQuantity | status |
|---|---|---|---|
| SPY 1, filled | "0" | "1" | Filled |
| EWN 1000, filled | "0" | "1000" | Filled |
| EWN 1000 LMT, cancelled after 600 filled | "400" | "600" | Cancelled |
| cancelled, unfilled | original | "0" | Cancelled |

- Original size = total + filled. Live OrderStatus agrees (remaining 400).
- Paper partial fills: a limit BUY 1000 EWN at the ask filled in chunks (300/105/595) over
  ~10s. Late fills can arrive with the cancel reply; re-read `filled` from it.
- Bracket sibling-cancel (#1030) confirmed the same day; a fill without a Filled status never
  appeared in 8 fills.

### `open_orders` excludes finished orders

*Live 2026-10-18, paper Gateway, server 220; #1020.*

- After order 1 was cancelled (Cancelled status seen), `open_orders()` returned only working
  order 2, on the same connection and on a fresh one with the same client id. Finished orders
  are reachable only through `completed_orders`.
- **Not probed:** filled orders (assumed the same).

### Executions: `last_n_days` and `specific_dates`

*Live 2026-09-27 (Sunday), paper Gateway, server 220; #858.*

- Default `ExecutionFilter` → today only (Sunday: empty).
- `last_n_days = N` counts calendar days including today: on Sunday N=1,2 → empty; N=3 →
  Friday; N=7 → Tue–Fri.
- `specific_dates = ["yyyymmdd"]` → exactly that day.
- One commission report per execution comes back alongside.

*Live 2026-11-16 01:43Z (Sunday evening ET), paper Gateway, server 226; #1235.*

- `last_n_days = 7` → 14 executions: the 2026-11-11 UTC fills (`Execution.time` dates them
  `20261110`; see below) and 2026-11-15.
- `last_n_days` 8, 14 and 30 → none, no error, only the end message, although 8 spans the same
  2026-11-11 and 2026-11-15 fills. Above 7, TWS returns nothing rather than capping the window.
  IB's `ExecutionFilter.proto` gives 7 as the maximum.
- `specific_dates` one day at a time, 2026-10-26 to 2026-11-16: 2026-11-11 and 2026-11-15
  returned their fills; every other day, including each day 8 to 21 days back, returned none,
  no error, only the end message.
*Live 2026-11-25 19:15 UTC, paper TWS, server 223; `time` with `last_n_days = 7`.*

- `time` from 1978-01-01 00:00 UTC to the present was accepted: each date from one day to
  48 years back (1978-01-01, 1980, 2002, 30, 60, 365 and 3,650 days back) returned the same 317
  executions as the 7-day window. 1977-12-31 and earlier (1977-08-14, 1975, 1970-01-02, 1970-01-01)
  drew 10314 "The date, time, or time-zone entered is invalid." A `time` one day ahead returned
  no executions and no error.
- **Not probed:** a `specific_dates` day older than a week that has executions. None was
  reachable (`last_n_days` above 7 returns nothing), so whether TWS applies the "within the
  past week" in IB's `ExecutionFilter.proto` to `specific_dates` is not known.

### `Execution.time` rendering

*Live 2026-11-11 overnight (02:17Z to 03:09Z), paper Gateway, server 226; #1225.*

- The Gateway setting "Send instrument-specific attributes for dual-mode API client in"
  picks the rendering of `Execution.time` and `OrderState.completed_time`. One ES (CME) fill
  at 02:17:28Z, re-requested under each setting:

  | Setting | `Execution.time` | `completed_time` |
  |---|---|---|
  | Instrument timezone (the default on this install) | `20261110 20:17:28 US/Central` | `20261110 20:17:28 US/Central` |
  | Operator timezone (login zone Europe/London) | `20261111 02:17:28 Europe/London` | `20261111 02:17:28 Europe/London` |
  | UTC format | `20261111-02:17:28Z` | `20261111-02:17:28` |

- Zone-named forms are `yyyymmdd HH:MM:SS <IANA zone>`, single spaces. The UTC form is
  `yyyymmdd-HH:MM:SS`, with a trailing `Z` on `Execution.time` and none on `completed_time`.
- Stored as an instant, rendered at send time: the same fill re-rendered under each setting,
  and fills placed under UTC format and Operator timezone rendered the same way. The setting
  took effect without a restart.
- Execution filters: the default `ExecutionFilter`, `last_n_days` 1 and 2, `client_id` of the
  placing client and 0, and `time = "20261110-20:00:00"` each returned every fill of the
  session. `specific_dates = ["20261111"]` returned them and `["20261110"]` none, under all
  three settings: the filter's day is the UTC or operator day, not the instrument day
  (dated 20261110 in US/Central). A request sent in the same second as a fill did not
  include it; it was present on a later request.
- `ExecutionFilter::time` (Instrument timezone setting only; eight fills at 02:17:28Z,
  02:36:38Z, 02:37:49Z and 02:39:55Z, two each): the dashed form is UTC, as the 10314 text
  states, and the zone-less space form was read as UTC (or the operator zone,
  Europe/London, which equals UTC in November), not the instrument zone.
  `"20261111-02:30:00"` returned the six after 02:30Z and `"20261110-20:30:00"` all eight;
  `"20261110 20:30:00"` also returned eight (as Central it would have returned six) and
  `"20261111 02:30:00 Europe/London"` six. A zone suffix on the dashed form
  (`"20261110-20:30:00 US/Central"`, `"20261111-02:30:00 UTC"`) drew error 10314 ("The
  date, time, or time-zone entered is invalid") and no fills. The 10314 text itself says "If
  no time-zone is specified, local time-zone is assumed(deprecated)" and "yyyymmddd-hh:mm:ss
  time is in UTC".

*Live 2026-11-16 Mon 09:18 ET, paper Gateway, server 226, handshake zone "Greenwich Mean
Time"; four BTC (PAXOS) fills of 2026-11-15 at 15:37:22Z, 15:50:40Z, 15:53:03Z and 15:53:23Z,
`Execution.time` rendered in US/Eastern; #1255.*

- `ExecutionFilter::time` zone suffix, each string naming 15:52 on 2026-11-15 in its zone,
  with `last_n_days = 2`: `"20261115 15:52:00 UTC"` and `"20261115 15:52:00
  Europe/London"` returned the two fills after 15:52Z, as did `"20261115-15:52:00"` and the
  zone-less `"20261115 15:52:00"`. `GMT`, `Etc/UTC`, `Europe/Paris`, `US/Eastern`,
  `America/New_York`, `US/Central`, `America/Chicago`, `US/Pacific`, `Asia/Tokyo` and `EST` each
  drew 10314 and no fills, as did `"20261115-15:52:00 UTC"`; a rejection settles those strings
  whatever instant each named. So the accepted zones were `UTC` and `Europe/London`: the
  login zone of the 2026-11-11 run, and the zone the handshake's "Greenwich Mean Time" most
  likely names (Java uses that display name for both `GMT` and `Europe/London`, and `GMT`
  itself was rejected). Which of the two decides was not probed.
  The instrument's zone was rejected, although the 10314 text gives `20031126 15:59:00
  US/Eastern` as its example.
- `time` does not reach past the days the filter covers (see
  [Executions: `last_n_days` and `specific_dates`](#executions-last_n_days-and-specific_dates)).
  With `last_n_days = 0` the four accepted forms above returned nothing (the fills were a day
  old); the same strings with `last_n_days = 2` returned the two fills. Controls: default filter
  none, `last_n_days = 2` four, `last_n_days = 7` fourteen.
- The zone-less form drew warning 2174 ("... without explicit time zone. Please switch to
  use yyyymmdd-hh:mm:ss in UTC or use instrument time zone, like US/Eastern. Implied time
  zone functionality will be removed in the next API release") on the first such request of
  the connection and not on the second.
- *Live 2026-11-24 Tue 19:30 ET, paper Gateway, server 226; 32 ES fills over 2026-11-18 to
  2026-11-24, `last_n_days = 7`.* The client's `yyyymmdd hh:mm:ss UTC` form drew no 10314 and
  cut at the instant: `20261124 15:00:00 UTC` returned the four fills from 15:05:05Z on, as
  did 09:00 at −06:00 (the same instant, sent as UTC); `20261119 15:12:00 UTC` twenty, from
  15:13:06Z on. `20261124 15:05:05 UTC` still returned the fill stamped 15:05:05Z, so the
  cutoff is inclusive at whole-second precision (or the fill's sub-seconds fall after it).
- **Not probed:** live (non-paper) accounts; a space-form zone suffix against an instrument
  in that zone other than US/Eastern (BTC); the dual-mode setting other than Instrument
  timezone, which both runs used for `time`, so whether "Operator timezone" or "UTC format"
  changes the accepted zones; a Gateway running in a zone other than London's, so whether
  the accepted zone follows the Gateway and whether `specific_dates` and the zone-less
  `time` filter read UTC or the operator zone (London and UTC coincide in November); `time`
  together with `specific_dates`; TWS as opposed to the Gateway.

### `exercise_options` account

*Live 2026-10-04, paper Gateway, AAPL call, no position held; #901.*

- Account `""`, the managed account, and a bogus `DU0000000` all returned 322 "No unlapsed
  position exists in this option in account <X>"; for `""` X was the login's account. Account
  is optional on a single-account login.
- **Not probed:** multi-account (FA) logins.

### `exercise_options` customer account and professional customer

*Live 2026-11-15 20:43 ET (Sunday evening), paper Gateway, server 226, SPY 20261204 930 call, no
position held; #1226.*

- Neither field set → 322 "No unlapsed position exists in this option in account <X>", as in
  the account entry above.
- `customer_account("CUST123")` → 145 "Error in validating entry fields -Account config doesn't
  allow to specify customer account value: CUST123 for account <X>". The same with the value set
  to the account itself.
- `professional_customer()` → 145 "... Account config doesn't allow to assign 'true' value for
  ProfessionalCustomer for account <X>".
- Both set → 145 with the customer-account message only.
- So the check is per account configuration, and runs before the position check: 145 replaced
  322. The C# client documents the parameters only as "Customer Account" / "Professional
  Customer".
- **Not probed:** an account whose configuration allows either field (omnibus, FA), live
  accounts, an exercise with a position held, and `Order`'s fields of the same names on
  `place_order`.

### `exercise_options` replies

*Live 2026-11-13 11:22–11:38 ET RTH, paper TWS, server 223, SPY 20261120 calls (755, 780), exercise
with `override_natural_action`.*

- First reply: 10349 "Order TIF was set to DAY based on order preset.", bound to the exercise's
  order id. The exercise goes ahead.
- Then, on the exercise's order id: OpenOrder (BUY LMT, price 0, quantity 1, no account)
  PreSubmitted; ExecutionData for the underlying (BOT 100 at the strike); OpenOrder and
  OrderStatus Filled with quantity and `filled` **100**; ExecutionData for the option (SLD 1 at
  0); OpenOrder and OrderStatus Filled with `filled` 1. Both executions carry
  `optExerciseOrLapseType` 1 (Exercise) and the exercise order's `permId`.
- No commission report for either execution, within 40 s or by `reqExecutions` later the same
  day.
- The open order has no field marking it as an exercise; only the shared `permId` links it to the
  executions.
- **Again during US hours** (*live 2026-11-24 16:23 and 16:31 UTC, paper TWS, server 223;
  AAPL Nov 27 2026 calls, bought by one client and exercised by another*): the same frames and
  order. The 340 call (in the money, AAPL at 352): 10349, OpenOrder and OrderStatus
  `PreSubmitted`, 6 ms later ExecutionData BOT 100 AAPL at 340, OpenOrder and OrderStatus Filled
  with 100, then ExecutionData SLD 1 call at 0 and Filled with 1, all within 20 ms. The two
  execution ids share their first two fields and differ in the third (`.01.01` stock, `.02.01`
  option), like a combo's. No commission report for either, live or in `executions` 3 minutes
  later. An exercise of the out-of-the-money 380 call (AAPL at 351) went through the same way,
  buying 100 AAPL at 380.
- Lapse before the last trading day: 10349, OpenOrder and OrderStatus `Inactive`, 201 "Order
  rejected - reason:trade date must match last trade date of the contract", then 322
  "Error processing request.Exercise/Lapse failed due to server rejection." on the same id.
- Futures option (ES Dec 2026 6800 put, held): 10349, `Inactive`, 201 "Order rejected -
  reason:Real time exercise of FOP is not available. Please send TMS ticket to request this
  exercise.", then 322. The position was unchanged.
- **Not probed:** a lapse on the last trading day, assignment, automatic exercise at expiry,
  puts, and the Gateway.

### Weekend cancel of a PreSubmitted ES order wedges in PendingCancel

*Live 2026-11-14 weekend (Sat 16:55 ET, CME closed), paper Gateway, server 226; #1214.*

- Plain `cancel_order(id)` (default `OrderCancel`; 5.x `cancel_order(id, "")`) on three
  PreSubmitted ES buy limits (CME, 202612) → no reply within 5s, and `all_open_orders` lists
  all three as `PendingCancel` afterwards. The same cancel on a PreSubmitted AAPL OPT order
  (SMART) in the same session → `Cancelled` at once.
- A different trigger from the hyphenated `ext_operator` wedge above: no `ext_operator`, the
  exchange closed.
- **Not probed:** whether the cancel completes at the Sunday 18:00 ET CME open or reverts at the
  daily reset, as the hyphen wedge did.

### `nextValidId` can repeat an exercise's order id after a disconnect

*Live 2026-11-13 RTH, paper TWS, server 223; observed while probing `exercise_options` (#1210).*

- Twice, a session disconnected right after its exercise; the next session's `nextValidId`
  returned the exercise's order id again, and an order sent with that id got no reply at all
  (no status, no error).
- After a session that stayed connected until the exercise finished, `nextValidId` did not
  repeat the id.
- **Not probed:** whether a plain order (not an exercise) shows the same, whether it depends on
  the client id, and whether TWS rejects the reused id later.

### `ext_operator` and `manual_order_indicator` on orders and cancels

*Live 2026-11-14 16:33–16:42 ET (weekend), paper Gateway, server 226; #1206. Earlier run: live
2026-11-10 20:03–20:07 ET (AAPL off-hours, ES Globex overnight), server 226; #1200.*

- **Accepted `ext_operator` set, orders and cancels alike:** ASCII letters and digits, either
  case, 1–16 characters. Tried on AAPL BUY 1 LMT $1 orders: `TRADER7`, `Trader7`, `trader7`,
  `T`, 8 and 16 characters; `TRADER7` and `12345` on ES (#1200).
- **Order, rejected (201 "Order rejected - reason:Invalid value in field # 8089", order
  Inactive):** `-`, `_`, `.`, `/`, `@`, a space anywhere (`TRADER 7`, ` TRADER7`, `" "`), and
  17–64 letters and digits (tried 17, 18, 19, 20, 32, 33, 64). The rejected order echoes the
  value sent; an accepted AAPL order echoes `ext_operator` as `""`. In #1200 an ES order echoed `TRADER7`.
- **65 or more letters and digits are accepted again** (tried 65, 96, 112, 127, 128, 129, 256),
  on orders and cancels. Whether TWS keeps or drops such a value was not observable from the
  AAPL echo. The client rejects these anyway (#1206).
- **Non-ASCII** (`TRADÉR7`): 10363 "Non-ASCII symbol in field
  PlaceOrderRequest.order.extOperator" (or `CancelOrderRequest.orderCancel.extOperator`). No
  order is created and the id is not used up. A cancel refused this way leaves the order working.
- **Cancel (`cancel_order_with`), PreSubmitted AAPL / Submitted ES:** an accepted value, or
  `manual_order_indicator` 0 or 1, → OrderStatus Cancelled + 202 within ~1s. A value the order
  side rejects with 201 (`TRADER_7`, 17 characters, `" "`, and in #1200 `probe-cxl`) → **no
  reply**: the order goes PendingCancel and stays there. A later plain cancel draws 10148
  "cannot be cancelled, state: PendingCancel"; a global cancel draws 161. In #1200 the orders
  were still wedged at 20:07 ET. By 01:18 ET (inside the daily reset window) TWS had dropped
  the cancel: each order was back in its prior state, and a plain cancel then got Cancelled + 202.
  When between the two it reverted was not observed.
- **Global cancel (`global_cancel_with`):** `TRADER7` + indicator 1 → 202 for each working order.
  `probe-gcx` (hyphen) → the order went PendingCancel and wedged, as above (#1200).
- **What-if** (`what_if = true`) does not check `ext_operator`: `probe-op` returns a normal
  what-if OpenOrder.
- **Order, `manual_order_indicator`:** 0 and 1 accepted on AAPL (PreSubmitted). The OpenOrder
  echo reports it absent (`None`); only a rejected order echoed the value sent.
- **Not probed:** the 1–16 / 65+ sets on futures (#1206 ran on AAPL only; Globex was closed),
  whether a 65+ value reaches the order, `global_cancel_with` with values other than the
  hyphen, any indicator value other than 0/1, what reverts a wedged PendingCancel (the reset or
  a timeout), and live (non-paper) accounts.

### `adjusted_order_type` and `delta_neutral_order_type` echo

*Live 2026-11-15 00:44–00:46Z (Saturday evening ET, off-hours), paper Gateway, server 226; #1220.*

- **`adjusted_order_type` set:** two STP children of a resting AAPL BUY LMT, built with
  `attach_adjustable_stop`, echoed `TRAIL` and `STP LMT` as sent, with `trigger_price`,
  `adjusted_stop_price` and `adjusted_stop_limit_price` as sent, on the `place_order` echo,
  `open_orders()` and `completed_orders()`.
- **Unset:** the parent LMT order echoed the string `None` in `adjusted_order_type` on all three
  reads, and all three stock orders (none with a hedge) echoed `None` in `delta_neutral_order_type`
  on all three reads; no echo was empty. IB's Java `OrderType.get` maps `None` and `""` to its
  `OrderType.None`; the crate decodes both to `None`.
- **`delta_neutral_order_type` set:** on a non-VOL order (a one-leg option BAG with a
  `delta_neutral_contract`) → 321 "Cannot set VOL attribute on non-VOL order", order not created.
  On a VOL order (`order_builder::volatility`, 20 % annual, AAPL 20261218 C300) with
  `delta_neutral_aux_price` → 321 "Delta hedge order type requires that no delta hedge aux price be
  specified". Without the aux price (`LMT` or `MKT`), and a VOL order with no hedge, the Gateway
  gave no OpenOrder or notice in 4 s and `open_orders()` was empty 4–8 s later; `cancel_order` then
  drew 202 "Order Canceled - reason:Order was discarded." and 161. Every VOL order was discarded,
  hedged or not; off-hours is the likely cause, unverified.
- Cancelling the parent cancelled both children (`completed_status` "Cancelled by System:
  Partial Decimal Quantity"); a cancel for an id TWS never created draws 10147.
- **Modify from the echo** (*live 2026-11-15 02:19–02:20Z, same session*): a resting AAPL BUY
  LMT read back with `open_orders()` and re-sent through `place_order` with a new limit price.
  With 5.x-style `String` fields that re-sends `"None"` in `delta_neutral_order_type` and
  `adjusted_order_type`; TWS took the modify with no notice and `open_orders()` showed the new
  price. Sending neither field (this crate since #1220) behaved the same. The echoed `"None"` is
  harmless on a modify.
- **MTL with a limit price** (*live 2026-11-15 02:23–02:24Z, same session*): AAPL BUY MTL,
  `DAY` and `GTC`, with and without `lmt_price` 100 → PreSubmitted plus 399 "will not be placed
  at the exchange until" the open; every echo reported `lmt_price` 0. TWS accepts a limit price
  on a non-auction MTL and drops it. `TimeInForce::Auction` on AAPL `SMART` → 201 "The
  time-in-force AUC is invalid for this combination of exchange and security type."
- **Not probed:** the echo of a set `delta_neutral_order_type` and whether a VOL order is kept
  during market hours; whether an auction MTL (an exchange that takes `AUC`) keeps its limit
  price; MTL during market hours; `adjusted_order_type` `STP` (`AdjustTo::Stop`); whether `None` is ever
  echoed for `order_type` itself (it was `LMT`, `STP` or `VOL` on every echo); executions and
  `order_update_stream`; TWS as opposed to the Gateway; live accounts.

### Open order fields TWS leaves out

*Live 2026-11-15 Sun 22:27 ET (overnight), paper, server 226; raw capture.*

- An unfilled AAPL BUY 1 LMT 1.00 (`outside_rth`), read back with `open_orders()` (5 frames):
  `Order.refFuturesConId` (116) **absent** in every frame; `filledQuantity` (115) **sent** as
  `"0"`.
- Settles that a stock order carries no reference future, so absent decodes as `None`, as C#
  leaves it unset (`HasRefFuturesConId`). `filledQuantity` is always sent, so it stays `f64`.
- **Not probed:** a futures or EFP order, where `refFuturesConId` would be set; a filled or
  partially filled order.


### Non-US orders outside and during the trading session

*Live 2026-11-24 22:03-22:17 UTC (SEHK, HKFE, TSE, OSE and KSE closed) and 2026-11-24
23:46 to 2026-11-25 04:07 UTC (in session and at lunch), paper TWS, server 223;
non-marketable limit orders, each cancelled after 5 to 10 s.*

| Order | Reply | Cancel |
| ----- | ----- | ------ |
| 700 SEHK BUY 100 LMT 320 | 399 "Warning: Your order will not be placed at the exchange until 2026-11-25 09:30:00 Hongkong", then PreSubmitted | Cancelled, 202 |
| HSI future HKFE BUY 1 LMT 20000; HSI option `HSI25600L6` BUY 1 LMT 1 | the same 399 with `09:15:00 Hongkong`, PreSubmitted, then Submitted within 1.3 s | Cancelled, 202 |
| HSI futures option `PHS25600L6` BUY 1 LMT 1 | 399 with `09:15:00`, PreSubmitted twice | Cancelled, 202 |
| N225M future OSE.JPN BUY 1 LMT 40000 | PreSubmitted, no 399 | Cancelled, 202 |
| 7203 SMART BUY 100 LMT 2000 | Submitted, no 399 | Cancelled, 202 |
| K200 future KSE BUY 1 LMT 300 | nothing in 5 s | nothing in 5 s; `PendingCancel` in `all_open_orders` 21 s after placing; at 22:17 a second cancel drew 10147 and no open order was listed |

- The 399 carries a `Warning:` line and the order goes on, so it is a warning, not a rejection.
  The time is the exchange's next open in its own zone, as text.
- HKFE orders reached `Submitted` while the exchange was closed: `Submitted` does not mean the
  order is at the exchange.
- The KSE order neither acknowledged nor cancelled promptly, like the
  [weekend ES cancel](#weekend-cancel-of-a-presubmitted-es-order-wedges-in-pendingcancel), and
  cleared without a terminal status reaching the probe.
- The same orders while the venue traded or was at lunch (2026-11-25, each cancelled after 10 s):

  | Order | Session | Lunch break |
  | ----- | ------- | ----------- |
  | 700 SEHK BUY 100 LMT 320 (last 344) | 01:36 UTC: PreSubmitted, no 399 | 04:05: PreSubmitted, no 399 |
  | HSI option `HSI25600L6` BUY 1 LMT 1 | 01:36: Submitted | 04:05: Submitted |
  | HSI future BUY 1 LMT 20000 | not run (connection timed out) | 04:07: Submitted |
  | 7203 SMART BUY 100 LMT 2000 | 00:01: PreSubmitted | 02:35: PreSubmitted; 03:35 after lunch: PreSubmitted |
  | K200 future BUY 1 LMT 700; N225M future BUY 1 LMT 50000 | 23:46 (day session): Submitted, cancelled at once | |

- No 399 came during a session or a lunch break. The SEHK and TSE stock orders stayed
  PreSubmitted with an empty `why_held` while their venue traded, and 7203 had been Submitted at
  22:03 when TSE was closed, so neither status tells whether the order is at the exchange.
- Distance from the market decides the SEHK status (06:25, bid 339, ask 339.2): BUY 100 LMT 336
  and LMT 330 went PreSubmitted, then Submitted 0.2 s later; LMT 320 stayed PreSubmitted, and so
  did 7203 LMT 2000 again. IB holds a limit that far from the market without sending it on, and
  says so in no field.
- **Not probed:** fills; marketable orders; orders during the SEHK closing auction.

### Auction orders on SEHK and TSE

*Live 2026-11-24 22:04-22:06 UTC (both closed), paper TWS, server 223.*

| Order | 700 SEHK | 7203 SMART (JPY) |
| ----- | -------- | ---------------- |
| LMT, `tif` OPG (LOO) | Inactive, 201 "The time-in-force OPG is invalid for this combination of exchange and security type." | not run (connection timed out) |
| MKT, `tif` OPG (MOO) | not run (connection timed out) | Submitted; cancelled |
| MOC | 387 "Unsupported order type for this exchange and security type." | no frame in 5 s; `PendingSubmit` in `all_open_orders`; cancelled (PendingCancel, Cancelled, 202) |
| LOC | 387 | as MOC |
| MTL or LMT, `tif` AUC | Inactive, 201 "The time-in-force AUC is invalid ..." | Inactive, 201 (MTL) |

- The unsupported time in force drew an Inactive status before the 201; the unsupported order
  type drew 387 with no status.
- The 7203 MOC and LOC orders sat in `PendingSubmit` with no frame on their `place_order`
  replies while two other connections in the same minute timed out at the handshake, so the
  silence may be TWS load rather than the order type.
- **Not probed:** SEHK's closing auction order types (`tif` AUC was refused; no other type
  tried), TSE MOC and LOC during a session.

### Crypto orders on PAXOS: time in force and cash quantity

*Live 2026-11-25 11:01-11:03 UTC, paper TWS, server 223; BTC.USD (`CRYPTO`, PAXOS), BUY limits at
1000 and SELL limits at 1000000, 0.01 BTC or 10 USD. The account has no crypto trading
permission, so an order that passes every other check is rejected for that.*

| Order | Reply |
| ----- | ----- |
| BUY LMT, `tif` DAY or GTC | Inactive, 201 "The crypto buy order must be Minutes or IOC." |
| BUY LMT, `tif` IOC or `Minutes`; SELL LMT, `tif` DAY or `Minutes` | Inactive, 201 for the missing permission |
| BUY LMT or BUY MKT, `tif` FOK | 10052 "Invalid time in force:FOK", no open order or status |
| SELL LMT, `tif` FOK | Inactive, 201 "The time-in-force FOK is invalid for this combination of exchange and security type." |
| BUY MKT `Minutes` with cash quantity; SELL MKT DAY | Inactive, 201 "The crypto MKT order must be IOC." |
| BUY MKT IOC with a quantity and no cash quantity | 10289 "You must set Cash Quantity for this order", no open order |
| BUY MKT IOC with cash quantity 10; SELL MKT IOC with a quantity | Inactive, 201 for the missing permission |
| BUY LMT `Minutes` or SELL MKT IOC with cash quantity 10 | 10244 "Cash Quantity cannot be used for this order", no open order |

- A crypto BUY order takes only IOC or `Minutes`, a market order only IOC, and FOK is refused on
  every order. A market BUY needs a cash quantity, and every other order refuses one.
- TWS sends `Minutes` as the `tif` text of the order and echoes it on the open order. IB cancels
  an unfilled `Minutes` order after 5 minutes; this run could not observe that, as every order was
  rejected.
- The open order and the Inactive status came before the 201 in all 21 orders that drew both,
  0.1 to 33 ms apart; the open order and status were then sent again. A downstream run reported
  the 201 first in some orders, 0.1 to 0.7 s apart; this run did not reproduce it.
- The 10052, 10289 and 10244 refusals came within 4 ms, with no open order: these orders are not
  in completed orders either. Each order that drew a 201 is in `completed_orders` with its order
  id and perm id, status `Cancelled`, and `completed_status` "Rejected by System:" followed by
  the 201 text. A market order shows the price cap IB set (`lmt` 58796.25 on a SELL, 58798 on a 10 USD
  BUY, whose quantity became 0.00017006).
- Observed once, in a downstream run on 2026-11-25: an order with a refused time in force drew no
  frame at all for 55 minutes and then appeared in completed orders as rejected. Not reproduced
  here.
- **Not probed:** fills and the 5-minute cancel of a `Minutes` order (needs crypto permission);
  ZEROHASH; other crypto symbols; GTD and OPG.

### OCA groups

*Live 2026-11-25 19:12-19:14 UTC (US RTH), paper TWS, server 223; AAPL limits far below the
market, EUR.USD market orders.*

- Cancelling one member cancels the others: with two AAPL BUY limits in one group (`ocaType` 1),
  a cancel of the first sent the second `PendingCancel`, then `Cancelled` with 202, on its own
  stream; `all_open_orders` no longer listed it.
- A member placed after another member filled is rejected: EUR.USD BUY 20,000 MKT in a group
  filled at once; a second BUY 20,000 MKT in the same group went `Inactive`, then drew 201 "Order
  rejected - reason:OCA group is already filled".
- **Not probed:** `ocaType` 2 (reduce with block) and 3. Reported downstream the same day: type 2
  reduces the remaining members by the filled quantity, with block.

### Bracket child quantity follows the parent

*Live 2026-11-25 19:12 UTC (US RTH), paper TWS, server 223.*

- Parent AAPL BUY 1 LMT with `transmit=false`, then a child SELL 5,000,000 LMT with `parent_id` set
  to the parent and `transmit=true`: TWS echoed the child with `totalQuantity` 1 and `remaining` 1,
  `PreSubmitted` with `whyHeld` `child`, and no error. The child released the parent, which went
  `Submitted`.
- Cancelling the parent removed the child: `all_open_orders` listed neither afterwards.
- So TWS cuts a child's quantity to the parent's without telling the caller. Compare the echo with
  what was sent.

### Modify and directed routing refusals

*Live 2026-11-25 19:13 UTC (US RTH), paper TWS, server 223; AAPL BUY 1 LMT far below the market.*

| Request | Reply |
| ------- | ----- |
| Modify a working SMART order with the exchange changed to ARCA | 105 "Order being modified does not match original order." The order kept working. |
| Modify the same order's price, exchange unchanged | `PreSubmitted`, `Submitted` |
| New order directed to LSE, which does not list AAPL | 200 "No security definition has been found for the request" |
| New order directed to IEX | `PreSubmitted`, `Submitted`, no 10311 |
| Cancel of the order rejected with 200 | 10147 "OrderId 6 that needs to be cancelled is not found." |
| Second cancel of an order cancelled 3 s before | 10148 "OrderId 5 that needs to be cancelled cannot be cancelled, state: Cancelled." |

- 10311 ("This order will be directly routed to ...") did not appear because the TWS API
  precautionary setting allowed direct routes. With the setting off, a downstream adapter saw
  10311 for directed orders.
- Reported downstream the same day: 10147 for cancels of orders it had cancelled earlier. Not
  reproduced here: a second cancel within the session drew 10148.
- **Not probed:** a modify that changes the exchange of a held order.

### Option orders acknowledged late

*Live 2026-11-25 19:14 UTC (US RTH), paper TWS, server 223; AAPL 2026-12-18 245 call (deep in the
money) BUY 1 LMT 0.05.*

- The order's own stream received nothing from TWS for 13.1 s, then an `OpenOrder` past
  `PendingSubmit` (`PreSubmitted` or `Submitted`; the log does not say which). Before that,
  each `all_open_orders` request (at 5 s and 10 s) listed the order as `PendingSubmit`, and that
  reply also reached the order's stream as an `OpenOrder` and an `OrderStatus` `PendingSubmit`.
- A second order cancelled 3.5 s after placing went `PendingCancel` 1.7 s after the cancel, then
  `Cancelled`.
- A downstream adapter reported delays from 1.5 s to 60 s on the same paper TWS. Treat an option
  order without an acknowledgement as pending, not lost.
- **Not probed:** option orders on IB Gateway or a live account; other option roots.
