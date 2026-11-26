# Accounts wire facts

Positions, PnL and account updates by model. End-marker behavior for account streams is in
[protocol](protocol.md#end-markers-come-after-the-initial-dump-only). Format: [README](README.md).

### Flat position sends `"0"`

*Live 2026-10-26, paper Gateway, EUR.USD 25k buy then sell; #1088.*

- After closing, Position and PositionMulti frames carry `position = "0"`. TWS does not omit
  a zero quantity, so an absent `position` is not the flat case and decoding it as required
  does not end streams on a close.
- PnLSingle on the flat position also sent `"0"`.
- `pnl_single` for a never-held conid sends no frame at all (held ones get a 2150 notice
  first).
- **Not probed:** PortfolioValue (forex is not in the portfolio; needs STK in RTH).

### Unset PnL figures are omitted, not `f64::MAX`

*Live 2026-11-05 off-hours (16:51 ET), paper, server 226; #1170.*

- `pnl` on the account: the PnL frame carried all three figures (tags 2–4); `unrealizedPnL`
  was an explicit `0.0`.
- `pnl_single` on two flat futures positions (NQ, ES; `position = "0"`): each PnLSingle frame
  carried tags 1, 2, 3, 5, 6 and no tag 4, so `unrealizedPnL` is left out of the frame. `value`
  was an explicit `0.0`. Each frame came after a 2150 notice.
- No frame carried `f64::MAX`. The C# decoder substitutes `double.MaxValue` for a missing
  field (`HasUnrealizedPnL` false), so the sentinel C# callers see is the client's, not TWS's.
- **Not probed:** an open (non-flat) position, and RTH updates. Which figures TWS omits for a
  held position is unknown.

### `ledgerAndNLV = true` selects the ledger keys; a TWS setting decides the `$LEDGER-` prefix

*Live 2026-11-14 weekend (01:44 and 01:50 ET), paper, server 226; #1223.*

- `reqAccountUpdates` and `reqAccountUpdatesMulti` (one account, no model code, `ledgerAndNLV`
  `false` then `true`), each read to its end marker, run twice: once as the Gateway was, once
  after turning on Global Configuration → API → Settings → "Prepend '$LEDGER-' prefix to
  per-currency account values".
- `ledgerAndNLV = true`: 75 values, the same 25 ledger keys for each of BASE, EUR and USD
  (`CashBalance`, `AccruedCash`, `NetLiquidationByCurrency`, `ExchangeRate`, ...), and nothing
  else. `false`: those 75 plus the account-level keys, 216 values. Same counts with the
  setting off and on.
- Setting off: no key carries `$LEDGER-`. The ledger row and the account-level row of a key
  that is both collide: with `false`, `AccruedCash`/`USD` arrived twice, `31.99` (account
  level, equal to the BASE ledger row) and `212.85` (the USD ledger row). Key plus currency
  does not tell them apart.
- Setting on: exactly the 75 ledger rows carry `$LEDGER-` (`$LEDGER-AccruedCash`/`USD` =
  `212.85`); the account-level rows keep the bare key (`AccruedCash`/`USD` = `31.99`). BASE
  ledger rows are prefixed too.
- `reqAccountUpdates` is unaffected: 215 values, never a `$LEDGER-` key, and no duplicate
  key/currency pair under either setting.
- So the prefix is opt-in per Gateway/TWS install, not fixed protocol. The setting's default for
  new vs upgrading installs is IB's claim (10.47 release notes), not observed here.
- Earlier: 2026-11-05, a live account through a read-only Gateway (server 226, #1183) saw the
  prefix, so its setting was on; the `true`/`false` subset finding matched.
- **Not probed:** a model code, an FA master account, later update frames (only the initial
  dump to the end marker was read), whether the Gateway applies a changed setting to an
  already-open subscription (each run reconnected).

### Exchange on positions, executions and order echoes

*Live 2026-11-24 21:45-22:10 UTC (US closed), paper TWS, server 223; `positions`,
`executions` with an empty filter, and order replies.*

| Frame | `contract.exchange` | `contract.primaryExchange` |
| ----- | ------------------- | -------------------------- |
| Position, AAPL options (3 contracts) | `AMEX` | empty |
| Position, AAPL and MSFT stock | `NASDAQ` | empty |
| Position, SPY | `ARCA` | empty |
| Position, ES future and futures options | `CME` | empty |
| Execution (129 of the day's) | the executing venue: `IEX`, `IBKRATS`, `ARCA` for AAPL stock; `MX2`, `BOX`, `PHLX`, `ISE`, `EDGX` for AAPL options | empty |
| OpenOrder echo | the exchange the order was sent with: `SMART` for AAPL, VOD and 7203; `SEHK`, `HKFE` when sent there | as sent |

- A position names a listing exchange, not where the position traded: AAPL options came back
  with `AMEX` whatever venue filled them, stocks with their primary listing in `exchange` and an
  empty `primaryExchange`. Match a position to an order or execution by `conId`, not by
  exchange.
- An execution's `contract.exchange` equalled `execution.exchange` (the venue that filled) in all
  129. The pair from an exercise of an AAPL 2026-11-27 340 call (the option sold at 0, 100 AAPL
  bought at 340) reported `SMART` in both.
- 2026-11-25 (US RTH): AAPL echoes for orders sent to SMART and IEX carried that exchange and an
  empty `primaryExchange`; EUR.USD fills reported `IDEALPRO` in both exchange fields. A downstream
  adapter reported stock executions on `EUDARK`, `TGATE`, `FWB`, `IEX` and `ARCA` the same day;
  not re-probed (no stock fills placed).
- **Not probed:** positions in non-US contracts (none held); a stock whose primary listing is
  not its usual SMART destination.

### A second `reqAccountUpdates` for the subscribed account draws no reply

*Live 2026-11-25 off-hours, paper TWS, server 223.*

- `reqAccountUpdates(true, account)` while the first subscription for that account was live,
  written after the first dump's `AccountDownloadEnd`: no frame in the next 6 s. TWS does not
  send a second dump, unlike `reqPositions`, `reqOpenOrders` and `reqCompletedOrders`
  ([overlapping open-order requests](orders.md#overlapping-open-order-requests)). A second
  `account_updates` caller gets only later periodic updates, never the initial values.
- **Not probed:** a second request written before the first dump ends, and how long until the
  next periodic update reaches it.
