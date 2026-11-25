# Orders wire facts

Placement, echoes, status, executions and completed orders. Format: [README](README.md).

### `transmit` absent means held

*Live 2026-11-23 off-hours, paper TWS, server 223, AAPL BUY 1 LMT 1.00; first seen 2026-09-23,
paper Gateway, server 220 (#841, closed, not a bug).*

- A placeOrder proto with `transmit` left out (what `some_bool(false)` produces): TWS holds the
  order. It gets a permId but sends nothing for it: no OpenOrder, OrderStatus or notice on
  `order_update_stream` within 3 s. Control with `transmit=true` goes PreSubmitted → Submitted.
- A held order is listed nowhere: absent from `open_orders`, `all_open_orders` and
  `completed_orders(false)` while held, and from `completed_orders` after it is cancelled.
- Cancelling it is order-bound: the cancel's own subscription receives OrderStatus `Cancelled`
  (with the permId) and 202 "Order Canceled - reason:Order was discarded.", then 161 "Cancel
  attempted when order is not in a cancellable state. Order permId =…" as an error that ends the
  subscription. The same three frames reach `order_update_stream`. Server 220 showed 202 + 161.
- Cancelling the id again returns 10148 "…cannot be cancelled, state: Cancelled."; an id that was
  never sent returns 10147 "…is not found." So a cancel tells the three states apart: 202 =
  was held, 10148 = already done, 10147 = never reached TWS.
- It survives a disconnect: held, `disconnect()`, reconnect 3 s later with the same client id. It
  is still listed nowhere, and the cancel returns 202 "Order was discarded" + 161.
- It survives a pause: two orders held for 20 s were released by the last order's
  `transmit=true` like an immediate one.
- The official clients (`EClientUtils` in C#/Java/Python/C++) also set `transmit` only when
  true. Our decoder's `unwrap_or(true)` applies to inbound messages only and says nothing about
  how TWS reads an absent flag in outbound ones.
- An untransmitted order gets no open-order or status frames, so a TWS-derived value (e.g. a
  VOL order's limit) is only visible once transmitted.
- **Not probed:** how long a held order lasts beyond 20 s or a 3 s disconnect, whether a TWS
  restart discards it, and whether another client id (or client 0) can cancel it.

### Held parent and children

*Live 2026-11-23 off-hours, paper TWS, server 223, AAPL parent BUY 1 LMT 1.00 with SELL LMT
5000 / 6000 children.*

- Parent and first child held, last child `transmit=true`: all three go PreSubmitted within
  0.4 s and are listed by `open_orders`. Cancelling the parent then cancels both children
  (OrderStatus `Cancelled` + 202 for each id).
- Cancelling a held parent does **not** discard its held child: the child's own cancel still
  returns 202 "Order was discarded" + 161, not 10147. Cleaning up a partly sent list therefore
  needs a cancel per held order, not just the parent's.
- Re-sending a held parent with `transmit=true` releases its held child too: both go
  PreSubmitted.
- `submit_linked_orders` and bracket `submit_all` (sync and async share the code) on this
  session: all three orders PreSubmitted, then cancelled through the parent; a list whose last id
  is 1,500,000,000 failed with `OrderIdInRequestRange` and sent nothing (cancels of the first two
  ids returned 10147).
- **Not probed:** fills of a released bracket, and a send failing part-way on a live connection
  (the client checks every order before sending, so only a lost connection fails a later send).

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

### Trailing order echo

*Live 2026-11-23 23:13-23:14Z, paper TWS, server 223; EUR.USD SELL 20000 GTC, placed with
`trailing_stop` and `trailing_stop_limit`.*

| Placed | `order_type` | `aux_price` | `trailing_percent` | `trail_stop_price` | `limit_price_offset` | `limit_price` |
| ------ | ------------ | ----------- | ------------------ | ------------------ | -------------------- | ------------- |
| `TrailBy::Amount(0.3)`, stop 0.7 | `TRAIL` | 0.3 | absent | 0.7, then 0.79043 | absent | 0 |
| `TrailBy::Percent(20.0)`, stop 0.7 | `TRAIL` | absent | 20 | 0.7 | absent | 1.09047 |
| `TrailBy::Amount(0.3)`, stop 0.7, offset 0.007 | `TRAIL LIMIT` | 0.3 | absent | 0.7 | 0.007 | 0 |

- The echo carries the fields the builder sets, so `TrailBy` reads back from `aux_price` or
  `trailing_percent`. TWS trails the stop: within a second of placement the amount order's stop
  moved from 0.7 to 0.79043 (market 1.0905 less 0.3), and each later echo carried the new stop.
- The percent order echoed `limit_price` 1.09047, about the market price, where the amount orders
  echoed 0.
- Plain AAPL LMT orders echoed `trail_stop_price` 2.0 on the first echoes and none after a
  modify; the field is not a sign of a trailing order.
- A modify of the stop price alone draws 10067 "Stop price must be changed together with the
  trailing amount for a trailing order", and the order keeps working unchanged.
- **Not probed:** stocks or futures, trailing percent on `TRAIL LIMIT`, the percent order's stop
  after it starts to trail, and the echo after a trigger.

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

### What-if answer frames

*Live 2026-11-24 off-hours (04:15 ET), paper TWS, server 223; raw capture of `openOrder`
frames; #1289.*

| Order (what-if)                                              | Frames | Margin fields           | Commission             | Then    |
| ------------------------------------------------------------ | ------ | ----------------------- | ---------------------- | ------- |
| AAPL BUY 10 LMT 100, SELL 5 LMT 900                          | 2      | 1st absent, 2nd present | 1st `0`, 2nd `1.00003` | nothing |
| AAPL BUY 5 MKT; ES H7 LMT; EUR.USD LMT and MKT; SAP IBIS LMT | 1      | present                 | present                | nothing |
| AAPL BUY 10,000,000 LMT 1                                    | 2      | 1st absent, 2nd present | 1st `0`, 2nd absent    | 201     |
| AAPL BUY 10,000,000 MKT; ES H7 BUY or SELL 100,000           | 1      | present                 | absent                 | 201     |

- The first frame of a two-frame answer carries only `status` (PreSubmitted), `commission_and_fees
  = 0` and `warning_text` (the limit-price collar warning, "If your order is not immediately
  executable..."): no margin fields, no currencies. The second, 0 to 80 ms later (three runs),
  carries the margins, commission and currencies and no `warning_text`. A consumer that
  takes the first frame reports no margins and a zero commission. A stock market order drew no
  warning and one frame.
- An order the account's margin cannot carry: the frame with margins has `suggested_size` set
  (`"4000"`, `"14"`, `"16"`) and no commission or commission currency; an error 201 "YOUR ORDER
  IS NOT ACCEPTED. IN ORDER TO OBTAIN THE DESIRED POSITION ... MUST EXCEED ..." follows within
  1 ms. No accepted what-if carried `suggested_size`.
- No `orderStatus`, `openOrderEnd` or other end marker follows any what-if answer; the reply
  stays silent for the 6 to 10 s read.
- Rejected before any frame: unknown symbol, 200; EUR.USD LMT -1, 110; EUR.USD 500,000,000,
  201 "Order size is not within the limits".
- The answer's frames reach `order_update_stream` as well as the place-order reply, each with
  `order.what_if = true` (live 2026-11-24 10:59 UTC, `Client::analyze_order`: AAPL BUY 10 LMT
  100 two frames 33 ms apart, EUR.USD BUY 20,000 LMT 1.0 one frame with commission `2`).
- **Not probed:** RTH (the US stock frames may differ at the open); options, combos and other
  warnings; Gateway (the 2026-10-27 SPY probe above ran on Gateway 226 and did not count frames).

### What-if order ids

*Live 2026-11-24 21:53 UTC (US stocks closed), paper TWS, server 223; #1289.*

- One connection per step, same client id, AAPL BUY 1 LMT 100: what-if under id 1000 answered
  with two frames (PreSubmitted, then the margins); a real order under 1000 then drew 103
  "Duplicate order id: 1000", and so did a second what-if under 1000. A what-if under 1005
  answered; a second what-if under 1005, a what-if under 1003 and a real order under 1005 each
  drew 103.
- Settles that a what-if uses its order id like a placed order: the id and every id below it are
  spent for that client, although no order is left working. A caller that keeps its own id
  sequence must take what-if ids from it, or its next order collides.
- **Not probed:** whether the spent id survives a TWS restart; Gateway.

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

### Combo (BAG) shape and identity

*Live 2026-11-23 17:24-17:35 ET off-hours (CME Globex closed 17:00-18:00 ET), paper TWS, server
223; what-if orders on AAPL, SPY, MSFT and IBM options and stock, ES and CL futures; three AAPL
and MSFT verticals placed at 0.01 and cancelled. Guard: `Contract::spread_from_legs`,
`integration/*/tests/spreads.rs`.*

- **No combo contract id**: the echoed BAG `contract_id` is a placeholder per combo exchange,
  the same for every combo there: 28812380 for every SMART combo (AAPL, SPY, MSFT and AAPL/IBM
  combos, and futures combos sent to SMART), 14442663 on CBOE, 16472979 on CME (Z6/H7 and H7/M7
  ES calendars), 40348702 on NYMEX. Placed orders echo the same id in `open_orders` and
  `completed_orders`. So the legs identify a combo, not the id.
- **Echo**: legs come back sorted by con id ascending, whatever order they were sent in; ratio
  positive with the side in `action`; leg exchange filled (`SMART`, or the futures exchange).
  `local_symbol` repeats the symbol, `trading_class` is `COMB`, and `combo_legs_description` is
  `conid|signed ratio` per leg (`692808040|-1,838697388|1`), with `|CME` appended per leg when a
  futures combo goes to SMART.
- **Symbol**: must match the legs; another symbol draws 478 "Requested symbol MSFT, in legs
  AAPL". For legs on several underlyings TWS accepts any one leg's symbol or all of them sorted
  and comma-joined, and echoes the joined form (`AAPL,IBM`, though IBM's con id sorts first).
  `IBM,AAPL` draws 478 "... in legs AAPL,IBM".
- **Currency**: required on SMART (empty: 321 "Currency null isn't supported for smart combo"),
  but its value is not checked: `EUR` on USD legs echoed `USD`. Empty was accepted on CME.
- **Exchange**: required (empty: 321 "Missing order exchange"). Options take `SMART` or `CBOE`.
  A stock leg takes only `SMART` (10125). A futures calendar takes the legs' exchange (`CME`,
  `NYMEX`), or `SMART` only when each leg names that exchange; `SMART` with blank leg exchanges,
  or a wrong exchange, draws 200. SMART and CME gave the same margin for the ES calendar.
- **Leg exchange**: ignored for options (blank, `SMART` and `CBOE` all echoed the combo
  exchange). Blank is fine for futures on their own exchange; `NYMEX` on ES legs drew 200.
- **Ratios**: positive and with no common factor. 2:2, 2:4 and a single leg at 3 draw 321
  "Invalid leg ratio"; 1:3 and 2:3 were accepted. A zero or negative ratio draws 321 "The combo
  details for leg ... are invalid".
- **Not answered** (no reply within 20-40 s, no error): a BAG with one leg at ratio 1 (option
  or stock), the same leg twice, and the collar (stock 100 + put + call), with or without
  `NonGuaranteed`. Gateway 226 accepted the collar on 2026-10-29 (above).
- **Stock pair** (AAPL/MSFT, AAPL/IBM): 10043 unless the order is `NonGuaranteed`.
- **Not probed:** Gateway (TWS only); RTH; other directed option exchanges; non-USD combos;
  legs in different currencies; ES/NQ on CME drew 201 "Unknown contract" (no native
  inter-commodity listing), so a directed multi-underlying combo stays untested.

### Combo (BAG) executions and commissions

*Live 2026-11-23 23:11-23:39 UTC (CME Globex evening session), paper TWS, server 223; ES
calendars ESZ6/ESH7 (1x1 and 1x2) and an ES put vertical (P6500/P6450 Dec) as BAGs on CME,
market orders of 1 to 5 combos and a passive 10-lot limit cancelled after its first fill; 14
fills, each read on `place_order`, `order_update_stream` and `executions`. Guard:
`FillMatcher::add_combo`, `orders/fills_tests.rs`.*

- **Shape**: each fill of a combo order is one execution for the combo, then one per leg. The
  combo-level execution has security type `BAG`, the per-exchange placeholder contract id
  (16472979 on CME, recorded in #1271), `local_symbol` `ES`, no `combo_legs` and an empty
  `combo_legs_description`. Its `shares` is the combo
  quantity of the fill and its `price` the net price per combo: -7397 = 7260 - 2 x 7328.5 on
  the 1x2. Each leg execution carries the leg contract, `shares` = combo shares x ratio (2 on
  the ratio-2 leg) and the leg's own price.
- **Order**: combo-level execution first in all 13 fills read live, then the legs, the three
  within 8 to 14 ms. The leg with the higher con id came first every time (ESH7 before ESZ6,
  P6450 before P6500), on buys and sells and with the legs sent in either order. An OpenOrder
  and OrderStatus pair follows each execution; the OrderStatus that reports the fill follows
  the combo-level execution, before the legs.
- **Link**: the three executions share `order_id`, `client_id` and `perm_id`. Their
  `execution_id`s share the first two fields and differ in the third: `0000e1a7.6b0717ad.01.01`
  (combo), `.02.01` (first leg), `.03.01` (second leg). Each fill of a partly filled order gets
  a new second field: a 4-lot order filled 2 + 2 as `0000e1a7.6b0719db.*` and
  `0000e1a7.6b0719de.*`, 11 s apart.
- **Commissions**: one report per leg execution, 1 to 31 ms after the second leg's execution,
  in leg order: 2.16 USD per ES future, 4.32 for the ratio-2 leg's 2 contracts, 1.41 per ES
  option. No report for the combo-level execution, live or in `executions`. `realized_pnl` is
  set on closing legs only.
- **Partial fills**: a combo-level execution and its legs per fill, sized to that fill.
  `cumulative_quantity` and `average_price` run per order and per contract: the combo 2 then
  4, each leg 2 then 4 with the leg's running average (ESH7 7325.55 then 7325.925).
- **`executions` replay**: the same executions in the same order (per fill: combo, then legs;
  fills in time order), then all reports, none for a combo-level execution; the combo-level
  contract carries no legs there either. The replay was read on a connection with no live
  `place_order` subscription for the order: while one is open, the client delivers that
  order's executions to it and they are missing from the reply.
- **Legs from the reply alone** (*live 2026-11-24 09:16 UTC, paper TWS, server 223,
  `last_n_days` 7*): 91 executions and 77 reports. Every one of the 14 combo-level executions
  had exactly two executions with its `perm_id` and its first two id fields, both leg
  contracts (ES futures or options). Every execution with a combo order's `perm_id` was a
  combo-level execution or a leg of one: the orders filled in 2 and 3 parts gave 2 and 3 such
  groups, one per second field. The 14 missing reports were the combo-level executions'.
- **Legs against the order echo** (*live 2026-11-24 11:20 UTC, paper TWS, server 223,
  `last_n_days` 7*): for each of the 14 combo fills, the legs `Fill::combo_legs` derives (ratio
  = leg shares / combo shares, `Buy` when the leg executed on the combo execution's side) equal
  the legs TWS echoed on that order's `openOrder` frames when it was placed (contract id, ratio
  and action; 1x1 and 1x2 ES calendars and an ES put vertical, bought and sold, fills of 1, 2
  and 3 parts). Each leg execution's contract carried exchange `CME`, as the echo's legs do.
  `completed_orders` no longer listed those orders; it listed one cancelled ES calendar order,
  legs sorted by contract id with ratio 1 and `CME`.
- **Executions missing once**: on one fill (1 of a 4-lot market sell, 23:26:39 UTC) the client
  got the two leg reports on both streams but none of the three executions; the OrderStatus
  for the fill came 8 s later with `last_fill_price` 0, and `executions` afterwards listed all
  three. A raw capture of the next order (3 fills) showed the normal shape, so whether TWS
  skipped the frames or the client dropped them is not settled.
- **US option combos on `SMART`** (*live 2026-11-24 16:06-16:09 UTC, US market open, paper
  TWS, server 223; AAPL Dec 2026 350/355 call vertical, BAG on `SMART` with `SMART` legs,
  market orders of 1, sold then bought back*): the same shape and order as on CME. Combo-level
  execution first, then both legs within 14 ms, the higher con id first (the 355 call,
  927346994, before the 350 call, 692808279); execution ids `.01.01`, `.02.01`, `.03.01`; one
  commission report per leg (1.03 to 1.05 USD) 1 to 9 ms after the second leg, none for the combo.
  The combo-level contract id was 28812380 for both fills and for another client's AAPL combo
  that executed on BOX.
- **Executing venue, not `SMART`**: each fill's three executions carry the venue that executed
  it as the contract's exchange, `PHLX` for the sale and `EDGX` for the buy-back, so
  `Fill::combo_legs` gave the legs with `PHLX` and then `EDGX`. `completed_orders` echoed both
  orders' legs with exchange `SMART`, contract ids sorted, ratio 1, `Buy` 350 and `Sell` 355,
  which match the derived legs except for the exchange.
- A `SMART` combo buy sent while a single-leg sell of one of its legs was still working drew 201
  "Order rejected - reason:Cannot have open orders on both sides of the same US Option
  contract." (Inactive, then the 201).
- **Not probed:** SMART-routed futures combos, more than two
  legs, a leg execution before its combo-level execution, a combo-level report, execution
  corrections (an id ending `.02`), several combo fills interleaved, Gateway.

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

### Completed order ids can be `0`

*Live 2026-11-25 Wed 11:10 ET (RTH), paper TWS, server 223; #1263.*

- `completed_orders(ApiOnly::No)` returned 230 records; 185 carried `order.order_id` `0`. The
  ones printed had `order.client_id` `0`, status `Cancelled` and `completed_status` "Cancelled by
  Trader", each with a distinct `perm_id`. Paper Gateway (server 226, 2026-11-15) listed three
  cancelled orders the same way.
- Settles that a completed record's order id does not identify it; `perm_id` does. The
  `CompletedOrder` message has no top-level id, so `order.order_id` is the only id it carries.
- On paper Gateway (server 226), every `OpenOrder` frame captured (226 across sessions) carried
  the same id at the top level and on its order, including an order listed to a second client
  and to client 0's `all_open_orders`. The second client saw the first client's order under id
  `1`, so an open order's id identifies it only with `order.client_id`.
- **Not probed:** how the id-`0` orders were placed (they predate this run); an order entered in
  TWS by hand; one bound with `auto_open_orders`.

### `open_orders` excludes finished orders

*Live 2026-10-18, paper Gateway, server 220; #1020.*

- After order 1 was cancelled (Cancelled status seen), `open_orders()` returned only working
  order 2, on the same connection and on a fresh one with the same client id. Finished orders
  are reachable only through `completed_orders`.
- A 201-rejected order also leaves, within 50 ms:
  [Order rejected with 201](#order-rejected-with-201-events-and-listings).
- **Not probed:** filled orders (assumed the same).

### Order rejected with 201: events and listings

*Live 2026-11-23 21:17-21:19Z (after the US close), paper TWS, server 223. Six orders, each from
a fresh client; listings polled at +0, 50, 100, 250 and 500 ms and +1, 2, 3 and 10 s after the
first `Inactive` event.*

- Never accepted (SPY BUY 1 LMT 1.00, 201 "No Trading Permission, Customer Ineligible", MiFID
  KID; four runs): the first OpenOrder and the first OrderStatus already carry `Inactive`, with
  a permId; no PreSubmitted step. The 201 follows 4-19 ms later.
- Accepted, then rejected (AAPL BUY 10,000,000 LMT 1.00, 201 "Your order is too large"; two
  runs): OpenOrder and OrderStatus `PreSubmitted`, then both `Inactive` 1-3 ms later, then the
  201 2-13 ms after that. The permId is the same throughout.
- The `place_order` stream ends with the 201.
- The placing client's `open_orders()` listed the order only in the +0 ms poll of 2 of 6 runs,
  polls that started 1-2 ms after the `Inactive` event: status `Inactive`, filled 0, total the
  ordered quantity, `order_id` and `order.order_id` both the placed id. From +50 ms on it was
  never listed. This crate also routes a client's own OpenOrder frames to a listening
  `open_orders()` channel, so those two hits may be pushed frames rather than rows of the
  `reqOpenOrders` reply; the probe did not tell them apart.
- Another client's `all_open_orders()` never listed it, from +0 ms on.
- `completed_orders(false)` lists it with status `Cancelled`, not `Inactive`; filled 0; total
  the ordered quantity, as for any unfilled cancelled order
  ([totalQuantity](#completed-order-totalquantity-is-the-unfilled-quantity)); the API order
  id in `order.order_id`. At the time of the probe the crate's top-level `order_id` was -1
  (#1263). The first listing that held it ended 19-204 ms after the `Inactive` event (five
  runs); two listings that ended 6 and 10 ms after it did not hold it. It stayed listed through
  +10 s and in a read minutes later. In one run the same +0 ms poll showed it `Inactive` in
  `open_orders()` and `Cancelled` in `completed_orders()`.
- Settles that a 201-rejected order leaves `open_orders` like any finished order
  ([`open_orders` excludes finished orders](#open_orders-excludes-finished-orders)), within
  50 ms. Inferred: the `Inactive` event is the terminal signal, and a lookup in
  `completed_orders` has to match on `perm_id` or `order.order_id`.
- **Not probed:** other 201 reasons; a 201 on a modify; `completed_orders(true)`; a raw capture
  to settle the two `open_orders()` hits; the Gateway.

### Overlapping open-order requests

*Live 2026-11-25 off-hours, paper TWS, server 223; client with 3 resting EUR.USD limits, a
second client with 1, other clients' orders resting too.*

- Two requests written back to back on one client, before any reply frame: TWS sends one full
  reply per request, each with its own end marker, in the order sent, never interleaved. Seen
  for `reqOpenOrders` twice (`5 3 5 3 5 3 53`, then the same again), `reqAllOpenOrders`
  twice, both mixed orders (the own-client list of 3 orders and `53`, then the all-clients
  list and `53`, or the reverse), and `reqCompletedOrders` twice (76 `CompletedOrder` and
  `102`, twice). The second reply came about 100 ms after the first.
- `reqPositions` twice behaves the same: 14 `Position` and `62`, twice.
- A second request written after the first reply's first frame arrived draws the same: the
  rest of the first reply, its end marker, then a full second reply.
- At connect, before `nextValidId`, TWS sends this client's open orders and an `OpenOrderEnd`
  that answers no request.
- So routing these replies by message type alone is not enough: every reply reaches every
  live subscription of its types, and a subscription must pass over the replies outstanding
  when its request was written (`ChannelMapping::reply_end`). Without that, a second
  `open_orders` made mid-reply read 4 of 6 frames and ended, and `all_open_orders` written
  after `open_orders` read the 3 own orders instead of all of them.
- **Not probed:** whether a busier account ever interleaves two replies, an unsolicited
  `OpenOrderEnd` while a request is outstanding. `reqAutoOpenOrders`: see
  [`auto_open_orders` draws no reply](#auto_open_orders-draws-no-reply).

### `auto_open_orders` draws no reply

*Live 2026-11-25 10:09-10:11 UTC off-hours, paper TWS, server 223, client id 0, async client
with `IBAPI_RECORDING_DIR`.*

- `reqAutoOpenOrders(autoBind=true)` drew no frame at all in the next 5 to 6 s, with no open
  order on the account, and again with a resting EUR.USD limit of client 0 and one of another
  client. No `OpenOrder`, no `OrderStatus`, no `OpenOrderEnd`: the request has no reply and
  no end marker.
- A far EUR.USD limit placed by another API client while the request was live sent client 0
  nothing.
- `reqAllOpenOrders` on client 0 while the request was live drew `5 3 53` (the other client's
  order). The client fanned that reply out to the `auto_open_orders` stream as well, which
  yielded the order and its status and then ended at the `53`. `reqOpenOrders` on client 0
  (with its own resting order) did the same. So every `OpenOrderEnd` a client 0 connection
  sees ends some other request's reply, and the auto stream now ignores it
  (`ChannelMapping` for `RequestAutoOpenOrders` lists no `OpenOrderEnd`).
- At connect, TWS sent client 0 its resting order, its status and an `OpenOrderEnd` before
  `nextValidId`; a connection whose client had no open order got none of the three.
- **Not probed:** an order placed in the TWS window, the case `autoBind` exists for (no manual
  order was placed); `autoBind=false`.

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
- **Not probed:** a `specific_dates` day older than a week that has executions. None was
  reachable (`last_n_days` above 7 returns nothing), so whether TWS applies the "within the
  past week" in IB's `ExecutionFilter.proto` to `specific_dates` is not known.

### Commission report timing and values

*Live 2026-11-23 21:20Z (after the US close), paper TWS, server 223; EUR.USD BUY 20000 MKT, then
SELL 20000 MKT to flatten.*

- One `CommissionAndFeesReport` per fill on the `place_order` stream, 5.0 ms (BUY) and 5.9 ms
  (SELL) after its ExecutionData.
- BUY sequence: OpenOrder Submitted; ExecutionData; OpenOrder Filled with
  `order_state.commission_and_fees` `None`; OrderStatus Filled; OpenOrder Filled with `Some(2.0)`;
  the report. The SELL's OpenOrder Filled with `Some(2.0)` came 6 ms after its ExecutionData,
  in the same millisecond as the report.
- Each report: `commission_and_fees` 2.0 USD, `realized_pnl` `None` (also on the SELL that
  closed the position), `yields` `None`. The decoder maps an absent field and `f64::MAX` both
  to `None`; which one TWS sent was not captured.
- No `-1`, `f64::MAX` or other sentinel in `commission_and_fees`, live or in the six reports
  `executions()` replayed 8-16 s later (2.0, 2.0, 2.24, 2.24, 2.0, 2.0).
- **Stocks and options during US hours** (*live 2026-11-24 16:04-16:21 UTC, paper TWS, server
  223; AAPL 1 share and an AAPL Dec 2026 350 call, 1 contract, each bought and sold at market;
  MSFT orders of 500 shares filled in 7 to 9 parts*): one report per execution, 1 to 2 ms after
  it, 20 ms for the option sale. The sequence is the forex BUY's: ExecutionData; OpenOrder
  Filled with `commission_and_fees` `None`; OrderStatus Filled; OpenOrder Filled with the
  commission; OrderStatus; the report. Values: 1.000003 and 1.007222 USD for the stock, 0.7653
  and 0.7859 for the option; `realized_pnl` `None` on the opening executions, set on the
  closing ones (-2.127225, -21.5512).
- **Partial fills**: each part got its own report right after its execution. The first part
  carried the order's minimum (1.000126 USD for 42 MSFT shares), later parts the increment
  (0.00012 for 40 shares, 0.60036 for 120), and OpenOrder's `commission_and_fees` ran as the
  order's total (1.000126, 1.000246, ... 2.5015 after 500 shares).
- **Not probed:** futures; a later corrected report; a raw capture of `realized_pnl`; the
  Gateway.

### Executions: request id on replies and live fills

*Live 2026-11-23 23:56Z off-hours, paper TWS, server 223; EUR.USD BUY 20,000 MKT, then
`executions(ExecutionFilter::default())`.*

- The live `ExecutionData` frame carried request id -1; the same execution in the
  `executions` reply carried the request's id (`1500000000`). The request id alone tells a
  reply from a live fill.
- The reply carried a commission report for each execution, other clients' executions
  included. A commission report carries no request id or client id, so only the execution id
  links it to its execution.

*Live 2026-11-24 00:26Z–00:32Z off-hours, paper TWS, server 223; raw captures; EUR.USD BUY
20,000 MKT with `executions()` sent on the live execution (5 runs), before the fill (2 runs),
and every 10 ms across the fill (5 runs, 40 requests each).*

- TWS sends one commission report for the live fill and one more inside each reply that lists
  the fill, before that reply's end marker.
- In 5 runs a reply landed between the live execution and its commission report. Each time that
  reply listed the execution without a report, its end marker followed, and then the live
  report arrived on its own. The fill's reports matched the replies that listed it one for one
  (38 replies, 38 reports in one run): the live one stood in for the racing reply's.
- A reply sent before the fill does not list it, and the fill's live report follows as usual.
- **Not probed:** a live report arriving inside a reply, before its end marker.

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

### Cancel replies, and a cancel that races a fill

*Live 2026-11-23 23:56Z off-hours, paper TWS, server 223; AAPL BUY 1 LMT 1.00 (PreSubmitted),
and EUR.USD BUY 20,000 MKT cancelled right after placing.*

- Resting order: `cancel_order` drew `OrderStatus` `Cancelled` and then error 202 ("Order
  Canceled - reason:"), both on the order id, within 0.2 s. No `PendingCancel` status came first.
- Market order cancelled at once: `OrderStatus` `PendingCancel`, then the fill
  (`ExecutionData`, request id -1), `OrderStatus` `Filled`, and the commission report. The cancel
  lost the race; no 202 or other error followed.
- **Not probed:** a partial fill followed by a cancel of the rest, and a cancel during RTH.

### Modifying an order

*Live 2026-11-24 off-hours (04:25-04:36 ET), paper TWS, server 223; EUR.USD BUY 20,000 LMT 0.95
(Submitted), modified with `place_order` on the same id; #1282.*

| Modify sent                                                       | Reply                                                                                |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| limit 0.95 -> 0.96; quantity 20,000 -> 25,000; `DAY` -> `GTC`     | OpenOrder and OrderStatus Submitted (once or twice) with the new terms, same perm id |
| only `order_type` and `lmt_price` set (quantity 0)                | 321 "... The size value cannot be zero"                                              |
| action BUY -> SELL; contract EUR.USD -> GBP.USD                   | 105 "Order being modified does not match original order."                            |
| order type LMT -> STP                                             | 329 "Order modify failed. Cannot change to the new order type.STP"                   |
| id of a cancelled, filled (MKT) or rejected (201, Inactive) order | 104 "Cannot modify a filled order."                                                  |
| id of an order TWS had not acknowledged yet                       | 103 "Duplicate order id: 5"                                                          |

- Every refusal carried the order id, came with no status, and left the order working with its
  earlier terms (`open_orders` right after). A second cancel of a cancelled order drew 10148.
- **Held order** (`transmit` false): the place and a modify with a new price drew no frames and
  `open_orders` did not list it. The modify that set `transmit` released it with the newest price
  (Submitted).
- **Another client**: client 209 sending client 208's working order id placed a second, separate
  order (another perm id, both listed by `all_open_orders`); its cancel of that id before then drew
  10147.
- **Slow acknowledgment**: in one run TWS took 7 s before the OpenOrder of a new order, and in
  another sent none; modifies sent meanwhile drew 103, and the unanswered order's cancel
  drew OrderStatus ApiCancelled with perm id 0.
- **Subscriptions**: with every subscription on an order id receiving its frames (#1282), a
  refused modify's error ends the modify's own subscription and reaches the earlier `place_order`
  subscription as a notice, which keeps receiving the order's frames (201, 104 and 434 on
  2026-11-24 during US hours, on the build with #1282). An earlier build of #1282 ended the
  earlier subscriptions with the 105 as an error. On 6.0-dev before #1282, each `place_order` or
  `cancel_order` on the id ended the earlier subscription at once, and a refused modify then
  ended its own, leaving none.
- **Partly filled order** (*live 2026-11-24 16:14-16:21 UTC, US market open, paper TWS, server
  223; MSFT BUY LMT at or above the ask, modified with `place_order` on the same id after the
  first partial fill*):

  | Modify sent                                    | Reply                                                                       |
  | ---------------------------------------------- | --------------------------------------------------------------------------- |
  | limit to 480 (below the market), filled 100    | OpenOrder with the new price, status `PendingSubmit`, then `Submitted`      |
  | quantity 2000 -> filled + 100                  | OpenOrder and OrderStatus `PreSubmitted`, then `Submitted`, `remaining` 100 |
  | quantity below the filled quantity             | 355 "Order size does not conform to market rule." under order id 0          |
  | quantity equal to the filled quantity          | 434 "The order size cannot be zero." on the order id                        |
  | limit change while the order was still filling | fills at the old price, status `PendingSubmit`; once `Filled`, 201          |

  - The quantity is the order's total, filled shares included: after 2000 -> 299 with 199
    filled, the cancel reported `filled` 199 and `remaining` 100.
  - The 355 frame carried order id 0 (raw capture), so it reached no subscription on the
    order, only `order_update_stream`; the order kept its quantity. The 434 reached the
    modify's subscription as an error and the earlier `place_order` subscription as a notice.
  - Modify overtaken by fills: the OpenOrder that answered the modify echoed the new price with
    status `PendingSubmit` while seven more fills at the old price (493.45 to 493.47 against a
    new limit of 480) completed the order; after `Filled` came 201 "Order rejected -
    reason:Order is already filled" on the order id, and further modifies drew 104.
- **Not probed:** a modify of a partly filled order's limit back into the market; IB Gateway;
  futures orders.

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

### `reqAllOpenOrders` raises the caller's next valid order id

*Live 2026-11-23 about 18:35Z and 21:19-21:20Z (after the US close), paper TWS, server 223.*

- Client 9420 placed AAPL BUY 1 LMT 1.00 as order 500001. Client 9421 connected with next valid
  id 1, listed 500001 with `all_open_orders()`, then sent its own order with id 1: TWS refused
  it 3 ms later with 103 "Duplicate order id: 1", although 9421 had never used id 1. A `reqIds`
  (`next_valid_order_id()`) 5 s later returned 500002.
- Earlier run: client 9332 read next valid id 1, still 1 after `open_orders()`, and 500002 after
  `all_open_orders()` listed client 9331's open AAPL order 500001. A fresh connection with the
  same client id read 500002 again, so the raise persists across connections.
- Control: client 9422 listed with `open_orders()` (0 rows), sent id 1, and TWS accepted it
  (PreSubmitted); `reqIds` then returned 2.
- `reqCompletedOrders(apiOnly=false)` and `reqExecutions` left it at 1 on a reconnect. Clients
  9431 and 9435 read 1 at connect, listed `completed_orders(false)` (22 rows, including other
  clients' orders 700001 and 500001 in `order.order_id`), and read 1 again on reconnect.
  Client 9437 did the same around `executions()` (six executions, the highest order id
  1000320). A client that sent no request also read 1 both times.
- The builder's `submit()` took the stale id 1 from `Client::next_order_id()` and returned
  `Ok(OrderId(1))`; the 103 arrived as a notice bound to order 1 on `order_update_stream()`.
- Settles that `all_open_orders()` can leave this crate's order-id counter below the floor TWS
  enforces for duplicates; `next_valid_order_id()` returns the raised value.
- **Not probed:** whether `reqCompletedOrders` or `reqExecutions` raise the id within the same
  session (no `reqIds` between the listing and the disconnect); `reqOpenOrders` with rows to
  list (it listed nothing in both runs, so it had nothing to raise the id above); whether TWS
  sends a `nextValidId` frame after the listing; `reqCompletedOrders(apiOnly=true)`; client
  id 0; the Gateway.


### Order ids are per client id

*Live 2026-11-24 16:35 UTC, US market open, paper TWS, server 223; clients A and B on two
connections, AAPL BUY 1 LMT 1.00 (rests), every order cancelled afterwards.*

- Both read next valid id 1 at connect. A placed id 2 (Submitted); `reqIds` then returned 3 to A
  and still 1 to B.
- B sent id 2, A's working id: TWS placed a second order with its own perm id, and
  `all_open_orders` listed both, under A's and B's client ids. `reqIds` then returned 3 to B.
- B sent id 2 again with quantity 2: TWS modified B's order (same perm id, quantity 2).
- A sent id 1, which it had never used and which is below its highest id 2: 103 "Duplicate order
  id: 1", 3 ms later.
- Settles that the next valid id and the duplicate check run per client id. An id the client has
  used names that client's order with the id (a modify, or 104 once the order is finished); an
  unused id below the client's highest draws 103.
- **Not probed:** client id 0, which can see other clients' orders; the Gateway.

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


### Order rejections outside the 200-399 band

*Live 2026-11-23 23:12-23:23Z (Monday evening ET; US stocks closed, EUR.USD trading), paper TWS,
server 223.*

Each code below answered a new order's `placeOrder` with no OpenOrder and no OrderStatus. The
notice carried the order id as its request id and reached both the `place_order` subscription
(which ended on it) and `order_update_stream`. A `cancel_order` of the same id then drew 10147
("not found"), so TWS created no order.

| Code | Order | Message |
| ---- | ----- | ------- |
| 110 | AAPL BUY 1 LMT 1.001; EUR.USD BUY 20000 LMT 0.900001 | "The price does not conform to the minimum price variation for this contract." |
| 103 | an id re-sent after a 110 for it; an id below the highest this client had used | "Duplicate order id: 1" |
| 10244 | AAPL `cash_qty` 100 on LMT BUY, LMT SELL, STP BUY, MKT BUY and MKT SELL | "Cash Quantity cannot be used for this order" |
| 10317 | EUR.USD MKT BUY `cash_qty` 0.229 | "The Cash Quantity size of 0.229 does not conform to minimum variation of 0.01 for this contract" |

- Sequence for 110 (client 9602): 23:12:51.165 place sent; 23:12:51.335 notice 110 on both
  streams; 23:12:51.336 `open_orders` empty; cancel then 10147 in the same millisecond.
- 103 is not what reusing a finished order's id draws: an id whose order was cancelled drew 104
  ("Cannot modify a filled order."), and a working order's id is a modify. 103 arrived only for
  an id with no order behind it.
- 110 answers a new order only. An off-tick price on a modify of a working order drew 201 instead;
  see [refused modifies](#refused-modifies-leave-the-order-working).
- 10244 during US hours on AAPL, an AAPL option, ESZ6 and BTC: see
  [cash quantity](#cash-quantity).
- **Not probed:** 10244 on other stocks; the same codes on IB Gateway or a live account.

### Orders TWS discards (461 and 10311)

*Live 2026-11-24 22:03-22:23 UTC (SEHK and TSE closed), paper TWS, server 223.*

| Code | Order | Message |
| ---- | ----- | ------- |
| 461 | 700 SEHK BUY 150 LMT 320; 7203 SMART BUY 150 LMT 2000 (board lot 100) | "BUY 150 700 SEHK @ 320.00: Order size 150 is not correct; it should be a multiple of 100" |
| 10311 | 7203 TSEJ BUY 100 LMT 2000 | "This order will be directly routed to TSEJ. Direct routed orders may result in higher trade fees. Restriction is specified in Precautionary Settings of Global Configuration/API." |

- Each notice carried the order id and came on the `place_order` reply with no OpenOrder or
  OrderStatus before it, 0.2 s (10311) to 2.4 s (461) after the order was sent.
  `all_open_orders` afterwards listed neither order.
- A `cancel_order` sent at once drew OrderStatus `Cancelled`, 202 "Order Canceled - reason:Order
  was discarded." and 161 "Cancel attempted when order is not in a cancellable state", for both
  codes. An order TWS never created draws 10147 instead: TWS assigned a perm id and discarded
  the order, as it does with a 201.
- The same 7203 order with `SMART` and 100 shares rested (Submitted) and was cancelled normally.
  A size below the lot drew 388 and an off-tick price 110, both before the routing check.
- 10311 names a TWS setting, so whether it fires depends on the TWS configuration.
- **Not probed:** other exchanges for 10311; the other precautionary codes; TWS with the setting
  off; 461 on a modify.

### Refused modifies leave the order working

*Live 2026-11-23 23:12-23:23Z, paper TWS, server 223. AAPL BUY 1 LMT 1.00 resting off-hours
(Submitted), and EUR.USD SELL 20000 TRAIL (PreSubmitted, `why_held` "trigger").*

Each modify below (`place_order` with the working order's id) was refused. `open_orders` right
after listed the order with its earlier terms and status. No `Inactive` status and no other status
change arrived for any of them.

| Modify sent | Reply |
| ----------- | ----- |
| limit 1.00 -> 1.001 (off tick) | OpenOrder and OrderStatus Submitted (twice), then 201 "Order rejected - reason:Invalid Price" |
| quantity 1 -> 10,000,000 | OpenOrder and OrderStatus Submitted (twice), then 201 "... YOUR EQUITY WITH LOAN VALUE ... MUST EXCEED THE INITIAL MARGIN ..." |
| quantity 1 -> 0 | 321 "Error validating request.-'v' : cause - The size value cannot be zero:" |
| quantity 1 -> 0.5 | 10242 "Fractional-sized order cannot be modified via API. ..." with request id **0**, not the order id |
| action BUY -> SELL; contract AAPL -> MSFT | 105 "Order being modified does not match original order." |
| `cash_qty` 100 added | 10241 "Order Quantity is expressed in monetary terms. Modification is not supported via API. ..." |
| TRAIL `trail_stop_price` changed, trailing amount kept | 10067 "Stop price must be changed together with the trailing amount for a trailing order" |

- A modify sent with the id of a cancelled order (AAPL BUY 1 LMT 1.00, cancelled 5 s earlier) drew
  104 "Cannot modify a filled order." with no status; the order stayed Cancelled and
  `open_orders` stayed empty.
- The 201 and 321 codes are in the order-rejection band, yet here they reject the modification
  only. The order goes on working with its previous terms.
- For the TRAIL order, a modify of stop price and amount together, and of the amount alone, was
  accepted. Sequence for 10067 (client 9605): 23:14:00.602 modify sent; 23:14:00.604 notice 10067;
  23:14:00.606 `open_orders` showed the order PreSubmitted with stop 0.79045 and amount 0.3,
  unchanged.
- A downstream claim that a refused modify can make TWS report the working order as `Inactive`
  did not reproduce in these seven variants.
- **Not probed:** modifies during RTH, of a partly filled order, or of an order TWS holds
  (`why_held` "locate"); IB Gateway; live accounts.

### Cancel refusals

*Live 2026-11-23 23:12-23:23Z, paper TWS, server 223.*

Every refusal below carried the order id as its request id, reached the `cancel_order`
subscription (which ended on it) and `order_update_stream`, and came with no status.

| Order state at the cancel | Reply |
| ------------------------- | ----- |
| Filled (EUR.USD MKT) | 10148 "OrderId 1 that needs to be cancelled cannot be cancelled, state: Filled." |
| Cancelled (a second cancel) | 10148 "... state: Cancelled." |
| Never created (rejected with 110 or 10244, or an id never sent) | 10147 "OrderId 52 that needs to be cancelled is not found." |
| Held (`transmit` false) | OrderStatus Cancelled, 202 "Order Canceled - reason:Order was discarded.", then 161 "Cancel attempted when order is not in a cancellable state.  Order permId =..." |

- 161 does not mean the order is unaffected: in the held case the same cancel ended the order,
  and 161 followed the Cancelled status and 202 in the same millisecond. Earlier entries record
  161 for a global cancel of a PendingCancel order, where the order did not change; see
  [`ext_operator`](#ext_operator-and-manual_order_indicator-on-orders-and-cancels).
- A filled or cancelled order drew 10148, never 161.
- **Not probed:** a cancel of a partly filled order, or one sent while a fill is in flight.

### Cash quantity

*Live 2026-11-23 23:18-23:20Z, paper TWS, server 223; EUR.USD on IDEALPRO and AAPL on SMART.*

- **EUR.USD**: `cash_qty` 1000 with quantity 0 was accepted on MKT BUY, MKT SELL, LMT BUY (0.9)
  and LMT SELL (1.5). TWS echoed a quantity of 1100.39 and sent 399 "Order Message: SELL 1,000 USD
  EUR.USD Forex Warning: ... below the EUR 20000 IdealPro minimum ..." for the BUY, which states
  the order as its USD leg. The market orders filled 916.88 and 917.09 EUR.
- **Filled with remaining above 0**: those fills reported OrderStatus `Filled` with `filled`
  916.88 and `remaining` 183.51, measured against the echoed estimate.
- **AAPL**: every `cash_qty` order drew 10244 and was not created; see
  [order rejections](#order-rejections-outside-the-200-399-band). A downstream rule that cash
  quantity works on MKT BUY orders only matches neither instrument: EUR.USD took every type and
  side, AAPL none.
- Cash quantity on EUR.USD must be a multiple of 0.01 (10317), and a quantity of 0.21 EUR without
  `cash_qty` drew 10318 "This order doesn't support fractional quantity trading".
- **US hours, by security type** (*live 2026-11-24 16:28-16:31 UTC, US market open, paper TWS,
  server 223*): AAPL stock (LMT BUY at 1.00 and MKT BUY), an AAPL option (LMT BUY), ESZ6 (LMT
  BUY) and BTC on PAXOS (LMT BUY) each drew 10244 within 0.2 s. BTC MKT BUY with `cash_qty` 100
  passed that check: TWS echoed a quantity of 0.001678 BTC and then rejected it with 201 "The
  crypto buy order must be Minutes or IOC"; with `tif` IOC, it echoed 0.00167857 BTC and a limit
  of 59579.75, then 201 for the account's missing crypto trading permission. BTC MKT SELL and
  LMT BUY with IOC drew 10244. So for crypto, cash quantity depends on the order type and side.
- **Filled cash-quantity order, US hours** (*EUR.USD MKT BUY, `cash_qty` 1000*): 399 (the USD
  leg warning, twice), OpenOrder `PreSubmitted` with quantity 1093.82; OrderStatus
  `PreSubmitted` with `filled` 911.41 and `remaining` 182.41 before any execution; the
  ExecutionData (911.41 EUR at 1.0972); `Submitted`; `PendingCancel`; `Filled` with `filled`
  911.41 and `remaining` 182.41; OpenOrder with the 2 USD commission; the report. TWS cancels the
  rest of the estimate, and `Filled` keeps the cancelled remainder in `remaining`. The 911.41
  EUR could not be sold by quantity (10318); a SELL with `cash_qty` 1000 sold 911.57 and a BUY
  with `cash_qty` 0.18 bought 0.16 (`remaining` 0.04 after `Filled`).
- **Not probed:** an account with fractional-share or crypto trading permission, other currency
  pairs, and other order types on EUR.USD.

### `why_held` values

*Live 2026-11-23 23:13-23:21Z, paper TWS, server 223.*

- A simulated EUR.USD TRAIL or TRAIL LIMIT order reported PreSubmitted with `why_held` "trigger"
  until cancelled. The cancel's Cancelled status carried `why_held` "".
- Short sales of 1 share (LMT 9999, outside RTH, no position) of BYND, GME, CVNA, SPCE, UPST and
  LCID went PreSubmitted or Submitted with `why_held` "" and no `Inactive` status. The paper
  account did not produce a "locate" hold off-hours.
- **Not probed:** short sales during RTH, a stock the account cannot borrow, and live accounts.

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

### `Minutes` time in force on a crypto order

*Live 2026-11-25 11:01 UTC, paper TWS, server 223; the account has no crypto trading permission.*

- BTC.USD (`CRYPTO`, PAXOS) BUY 0.01 LMT 1000, `tif` `Minutes`: TWS took the time in force and
  echoed `Minutes` on the open order, then sent Inactive and 201 for the missing permission. The
  same order with `tif` DAY drew 201 "The crypto buy order must be Minutes or IOC." instead.
- `completed_orders` lists the rejected order with `tif` `Minutes`.
- **Not probed:** the 5-minute cancel of an unfilled `Minutes` order (needs crypto permission).

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

### Orders during an internet outage

*Live 2026-11-25, paper TWS, server 223; AAPL BUY 1 LMT 1.00 resting PreSubmitted; Wi-Fi off 15 s
and 2 min; orders placed 5 s after the cut, before TWS reported the loss; #1300.*

- A resting order stayed PreSubmitted. At the 6-minute restore TWS re-sent `openOrder` and
  `orderStatus` for every open order 2 s before the 1102, unrequested.
- An order placed during the outage got ApiPending; with the 1100 TWS sent 1104 "Pending to
  create 1 orders: <id>". TWS never created it: no status, no error, absent from
  `reqOpenOrders`, and a later cancel drew 10147 "not found". 2 of 2 runs.
- A cancel sent 5 s later returned ApiCancelled within 1 ms.
- **Not probed:** an order placed after the 1100; a fill during the outage; IB Gateway.
