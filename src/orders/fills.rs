use std::collections::{HashMap, HashSet, VecDeque};
use std::mem;
use std::time::{Duration, Instant};

use super::{CommissionAndFeesReport, ExecutionData, Executions};
use crate::contracts::{ComboLeg, LegAction, SecurityType};

/// An execution with its commission report, joined by `execution_id`; for a combo, the
/// combo-level execution with the fills of its legs.
///
/// Built by [`FillMatcher`].
///
/// # Examples
///
/// ```
/// use ibapi::orders::{CommissionAndFeesReport, Execution, ExecutionData, Fill};
///
/// let fill = Fill {
///     execution: ExecutionData {
///         execution: Execution { execution_id: "0000e1a7.0001.01".to_string(), shares: 100.0, ..Default::default() },
///         ..Default::default()
///     },
///     commission: Some(CommissionAndFeesReport { execution_id: "0000e1a7.0001.01".to_string(), commission_and_fees: 1.25, ..Default::default() }),
///     legs: Vec::new(),
/// };
/// let fees = fill.commission.as_ref().map(|report| report.commission_and_fees);
/// println!("{} shares, commissions and fees {fees:?}", fill.execution.execution.shares);
/// ```
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Clone, Debug)]
pub struct Fill {
    /// The execution and its contract.
    pub execution: ExecutionData,
    /// The execution's commission report. `None` when the matcher's rule says TWS sends none
    /// for this execution (see [`FillMatcher::expects_report`]), or when the execution came
    /// from [`FillMatcher::take_expired`].
    pub commission: Option<CommissionAndFeesReport>,
    /// For a combo fill ([`FillMatcher::add_combo`], or any combo in a reply given to
    /// [`FillMatcher::pair_executions`]), the fills of the combo's legs in arrival order, each
    /// with its own commission report. `execution` is then the combo-level
    /// execution: security type [`Spread`](crate::contracts::SecurityType::Spread), the combo's
    /// quantity and net price, and no commission. Empty for any other fill.
    pub legs: Vec<Fill>,
}

impl Fill {
    /// The combo legs of a combo fill, one per leg execution, sorted by contract id; empty for
    /// any other fill.
    ///
    /// The combo-level execution carries the per-exchange placeholder BAG contract without its
    /// legs, so these legs are what identify the combo. Each is derived from its leg execution
    /// in this fill alone:
    ///
    /// - **Ratio**: the leg's shares in this fill over the combo-level execution's shares. TWS
    ///   sizes each leg execution of a partial fill to that fill (2 combos of a 1x2 fill the
    ///   ratio-2 leg with 4), so a partial fill gives the order's ratio.
    /// - **Action**: [`Buy`](LegAction::Buy) for a leg executed on the combo execution's side,
    ///   [`Sell`](LegAction::Sell) for the other side. That is the leg's action as the combo
    ///   order lists it, whether the combo was bought or sold: a sold calendar whose order buys
    ///   the back month executes a sale of the back month, and that leg comes back as `Buy`.
    /// - **Exchange**: the leg contract's exchange as the execution reports it: the venue that
    ///   executed the leg. For a combo routed to one exchange (`CME`) that is the order's
    ///   exchange; for a `SMART` combo it is the venue of that fill (`PHLX` on one fill, `EDGX`
    ///   on the next), while the order echo's legs say `SMART`.
    /// - **Contract id**: the leg contract's. Other fields keep their defaults.
    ///
    /// The order echoes TWS sends (`open_orders`, `completed_orders`) list a combo's legs
    /// sorted by contract id, so the contract id, ratio and action of each leg compare with an
    /// echo's `contract.combo_legs` leg by leg; the exchange does only for a combo routed to
    /// one exchange (live, 14 ES combo fills on `CME` and 2 AAPL option verticals on `SMART`:
    /// 2026-11-24, server 223; `docs/wire-facts/orders.md#combo-bag-executions-and-commissions`).
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::contracts::{Contract, LegAction};
    /// use ibapi::orders::{Execution, ExecutionData, ExecutionSide, Fill};
    ///
    /// let execution = |contract_id, side, shares| ExecutionData {
    ///     contract: Contract { contract_id, exchange: "CME".into(), ..Default::default() },
    ///     execution: Execution { side, shares, ..Default::default() },
    ///     ..Default::default()
    /// };
    /// let leg = |contract_id, side, shares| Fill { execution: execution(contract_id, side, shares), commission: None, legs: Vec::new() };
    ///
    /// // Two 1x2 calendars sold: the executions sell 4 of the back month and buy 2 of the front
    /// // month, so the order buys the back month at ratio 2 and sells the front month.
    /// let fill = Fill {
    ///     execution: execution(16472979, ExecutionSide::Sold, 2.0),
    ///     commission: None,
    ///     legs: vec![leg(649180695, ExecutionSide::Sold, 4.0), leg(495512563, ExecutionSide::Bought, 2.0)],
    /// };
    ///
    /// let legs = fill.combo_legs();
    /// assert_eq!((legs[0].contract_id, legs[0].ratio, legs[0].action), (495512563, 1, LegAction::Sell));
    /// assert_eq!((legs[1].contract_id, legs[1].ratio, legs[1].action), (649180695, 2, LegAction::Buy));
    /// assert_eq!(legs[1].exchange, "CME");
    /// ```
    pub fn combo_legs(&self) -> Vec<ComboLeg> {
        let combo = &self.execution.execution;
        let mut legs: Vec<ComboLeg> = self
            .legs
            .iter()
            .map(|leg| ComboLeg {
                contract_id: leg.execution.contract.contract_id,
                ratio: (leg.execution.execution.shares / combo.shares).round() as i32,
                action: if leg.execution.execution.side == combo.side {
                    LegAction::Buy
                } else {
                    LegAction::Sell
                },
                exchange: leg.execution.contract.exchange.to_string(),
                ..ComboLeg::default()
            })
            .collect();
        legs.sort_by_key(|leg| leg.contract_id);
        legs
    }
}

/// Executions and commission reports that found no partner, in arrival order, and combo fills
/// the matcher stopped waiting to complete.
///
/// Returned by [`FillMatcher::take_expired`] and [`FillMatcher::pair_executions`].
///
/// # Examples
///
/// ```
/// use ibapi::orders::{Executions, FillMatcher};
///
/// let rows: Vec<Executions> = Vec::new(); // an executions() reply, read to its end
/// let (_fills, unpaired) = FillMatcher::new().pair_executions(rows);
/// for execution in &unpaired.executions {
///     println!("no commission report for {}", execution.execution.execution_id);
/// }
/// for report in &unpaired.reports {
///     println!("no execution for report {}", report.execution_id);
/// }
/// ```
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default)]
pub struct Unpaired {
    /// Executions whose commission report did not arrive. From
    /// [`FillMatcher::pair_executions`], usually a fill so recent that TWS had not sent its
    /// report yet: a reply that lands between a live execution and its report lists the
    /// execution without one, and the report follows on the live stream after the reply's end
    /// (live 2026-11-24, paper TWS, server 223). Read the reply again, or wait for the report
    /// on the stream, before treating the commission as missing.
    pub executions: Vec<ExecutionData>,
    /// Commission reports whose execution did not arrive.
    pub reports: Vec<CommissionAndFeesReport>,
    /// Parts of combo fills ([`FillMatcher::add_combo`]) handed out incomplete: a combo fill
    /// with the legs that arrived, or, when the combo-level execution did not arrive, each leg
    /// fill on its own. Also a combo fill that a leg handed out here without its report
    /// completed, and a combo fill of a [`FillMatcher::pair_executions`] reply with a leg whose
    /// report the reply lacks; that leg has no commission.
    pub fills: Vec<Fill>,
}

/// What [`FillMatcher::push_report`] did with a commission report.
///
/// # Examples
///
/// ```
/// use ibapi::orders::{CommissionAndFeesReport, FillMatcher, ReportMatch};
///
/// let mut matcher = FillMatcher::new();
/// let report = CommissionAndFeesReport { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() };
/// match matcher.push_report(report) {
///     ReportMatch::Paired(fill) => println!("fill: {fill:?}"),
///     ReportMatch::Waiting => println!("execution not seen yet"),
///     ReportMatch::ComboPending => println!("leg paired, the combo fill is not complete"),
///     ReportMatch::Late(report) => println!("execution already handed out without it: {report:?}"),
///     ReportMatch::Duplicate(report) => println!("already seen: {report:?}"),
/// }
/// ```
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ReportMatch {
    /// The execution was waiting; here is the joined fill. When the execution was the last
    /// missing part of a combo fill, the combo fill.
    Paired(Fill),
    /// The execution has not arrived; the matcher holds the report until it does or
    /// [`FillMatcher::take_expired`] returns it.
    Waiting,
    /// The report paired with a leg of a combo ([`FillMatcher::add_combo`]); the matcher holds
    /// the leg fill until the combo fill is complete or [`FillMatcher::take_expired`] returns
    /// it.
    ComboPending,
    /// The first report for an execution the matcher handed out without a commission, by
    /// [`FillMatcher::take_expired`] or because its rule expects no report. The matcher
    /// remembers that it has now seen a report for the execution.
    Late(CommissionAndFeesReport),
    /// A report for an execution that already has one: paired, or answered as `Late` before.
    /// Also a second copy of a report that is still waiting. The matcher does not keep it.
    Duplicate(CommissionAndFeesReport),
}

/// Whether an execution the matcher handed out has had a report.
#[derive(Clone, Copy, Debug)]
enum Handout {
    Unreported { arrived: Instant },
    Reported,
}

/// The fills of one combo fill gathered so far: the combo-level execution and the legs whose
/// execution ids share `key` (see [`fill_key`]).
#[derive(Debug)]
struct ComboParts {
    perm_id: i64,
    key: String,
    execution: Option<Fill>,
    legs: Vec<Fill>,
    arrived: Instant,
}

impl ComboParts {
    /// Whether the combo-level execution has come and each leg has filled its quantity times
    /// its ratio.
    fn is_complete(&self, ratios: &[(i32, f64)]) -> bool {
        let Some(combo) = &self.execution else {
            return false;
        };
        let quantity = combo.execution.execution.shares;
        ratios.iter().all(|&(contract_id, ratio)| {
            let filled: f64 = self
                .legs
                .iter()
                .filter(|leg| leg.execution.contract.contract_id == contract_id)
                .map(|leg| leg.execution.execution.shares)
                .sum();
            filled >= quantity * ratio
        })
    }

    fn into_fills(self) -> Vec<Fill> {
        match self.execution {
            Some(combo) => vec![Fill { legs: self.legs, ..combo }],
            None => self.legs,
        }
    }
}

/// The part of an execution id that a combo-level execution shares with the leg executions of
/// the same fill: all but the last two dot-separated fields. TWS's ids have four fields
/// (`0000e1a7.6b0719db.02.01`), so the key is the first two (`0000e1a7.6b0719db`): the
/// combo-level execution and its legs differ only in the third field, and each fill of an order
/// gets a new second field. An id with three fields keys by its first field alone, and an id
/// with fewer than three fields is its own key, so it joins only an identical id. Callers also
/// require the same `perm_id`. See `docs/wire-facts/orders.md#combo-bag-executions-and-commissions`.
fn fill_key(execution_id: &str) -> &str {
    execution_id.rsplitn(3, '.').nth(2).unwrap_or(execution_id)
}

/// Joins each [`ExecutionData`] with its [`CommissionAndFeesReport`] as both arrive.
///
/// TWS sends an execution and its commission report as two messages that share only
/// `execution_id`, with other frames between them. Feed the matcher every `ExecutionData` and
/// `CommissionAndFeesReport` from [`OrderUpdate`](super::OrderUpdate),
/// [`PlaceOrder`](super::PlaceOrder) or [`Executions`] items, in either order: it returns the
/// [`Fill`] once both halves are known. For a whole [`executions`](crate::Client::executions)
/// reply, use [`pair_executions`](Self::pair_executions).
///
/// Some executions never get a report. The matcher returns those at once with no commission,
/// as its rule says (by default [`ExecutionData::expects_commission_report`]; set your own with
/// [`expects_report`](Self::expects_report)). For any other execution whose report does not
/// come, call [`take_expired`](Self::take_expired) periodically.
///
/// # Combos
///
/// For each fill of a combo (BAG) order TWS sends one execution for the combo itself, with the
/// combo's quantity and net price and no commission report, then one execution per leg with
/// the leg's quantity and price, each with its own report. Tell the matcher the order's
/// `perm_id` and legs with [`add_combo`](Self::add_combo) before its executions arrive, and it
/// returns each combo fill as one [`Fill`] whose [`legs`](Fill::legs) hold the leg fills, once
/// every leg has filled the combo quantity times its ratio and has its report. A combo the
/// matcher was not told about comes as separate fills: the combo-level execution without a
/// commission, and each leg.
///
/// [`pair_executions`](Self::pair_executions) needs no `add_combo`: a reply holds the whole
/// fill, so it puts each leg (same `perm_id`, execution id differing from the combo-level
/// execution's only in the last two fields) into the combo fill. The combo fill's
/// `commission` is `None` there too; the commissions are on the legs.
///
/// # What it holds
///
/// - Executions waiting for their report, and reports that came before their execution, until
///   they pair or `take_expired` returns them. One `max_wait` bounds both: a report precedes
///   its execution by milliseconds when it does at all, and a repeated report never waits
///   (see below).
/// - The parts of combo fills that are not complete, until they complete or `take_expired`
///   returns them; `max_wait` counts from the first part's arrival.
/// - The legs of the last [`memory`](Self::memory) combos given to `add_combo`.
/// - The ids of the executions it handed out, the last [`memory`](Self::memory) of them
///   ([`DEFAULT_MEMORY`](Self::DEFAULT_MEMORY) by default), oldest dropped first. A report
///   for one of these never waits: it is [`ReportMatch::Late`] the first time for an execution
///   handed out without a commission, and [`ReportMatch::Duplicate`] after that and for a
///   paired execution.
///
/// It is plain data: no I/O, no tasks, the same from sync and async code. Each method that
/// reads the clock has an `_at` twin that takes the time instead, for deterministic tests.
///
/// # Examples
///
/// ```no_run
/// # #[cfg(feature = "async")]
/// # async fn run(client: &ibapi::Client) -> Result<(), ibapi::Error> {
/// use futures::StreamExt;
/// use ibapi::orders::{FillMatcher, OrderUpdate, ReportMatch};
/// use ibapi::subscriptions::SubscriptionItem;
/// use std::time::Duration;
///
/// let mut stream = client.order_update_stream().await?;
/// let mut matcher = FillMatcher::new();
/// loop {
///     let item = match tokio::time::timeout(Duration::from_secs(1), stream.next()).await {
///         Ok(Some(item)) => item,
///         Ok(None) => break,
///         Err(_elapsed) => {
///             let unpaired = matcher.take_expired(Duration::from_secs(5));
///             for execution in unpaired.executions {
///                 println!("fill without commission: {execution:?}");
///             }
///             for fill in unpaired.fills {
///                 println!("incomplete combo fill: {fill:?}");
///             }
///             continue;
///         }
///     };
///     match item? {
///         SubscriptionItem::Data(OrderUpdate::ExecutionData(execution)) => {
///             if let Some(fill) = matcher.push_execution(execution) {
///                 println!("fill: {fill:?}");
///             }
///         }
///         SubscriptionItem::Data(OrderUpdate::CommissionAndFeesReport(report)) => match matcher.push_report(report) {
///             ReportMatch::Paired(fill) => println!("fill: {fill:?}"),
///             ReportMatch::Late(report) => println!("late commission: {report:?}"),
///             ReportMatch::Waiting | ReportMatch::ComboPending | ReportMatch::Duplicate(_) => {}
///         },
///         SubscriptionItem::Data(OrderUpdate::OpenOrder(order)) if !order.contract.combo_legs.is_empty() => {
///             matcher.add_combo(order.order.perm_id, &order.contract.combo_legs);
///         }
///         SubscriptionItem::Data(update) => println!("update: {update:?}"),
///         SubscriptionItem::Notice(notice) => println!("notice: {}", notice.message),
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct FillMatcher {
    expects_report: fn(&ExecutionData) -> bool,
    memory: usize,
    executions: Vec<(ExecutionData, Instant)>,
    reports: Vec<(CommissionAndFeesReport, Instant)>,
    handouts: HashMap<String, (u64, Handout)>,
    handout_order: VecDeque<(u64, String)>,
    handout_count: u64,
    combos: HashMap<i64, Vec<(i32, f64)>>,
    combo_order: VecDeque<i64>,
    combo_parts: Vec<ComboParts>,
}

impl Default for FillMatcher {
    fn default() -> Self {
        Self {
            expects_report: ExecutionData::expects_commission_report,
            memory: Self::DEFAULT_MEMORY,
            executions: Vec::new(),
            reports: Vec::new(),
            handouts: HashMap::new(),
            handout_order: VecDeque::new(),
            handout_count: 0,
            combos: HashMap::new(),
            combo_order: VecDeque::new(),
            combo_parts: Vec::new(),
        }
    }
}

impl FillMatcher {
    /// How many handed-out execution ids a matcher remembers unless
    /// [`memory`](Self::memory) says otherwise.
    pub const DEFAULT_MEMORY: usize = 10_000;

    /// Creates an empty matcher with the default rule and memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::FillMatcher;
    /// use std::time::Duration;
    ///
    /// let mut matcher = FillMatcher::new();
    /// let unpaired = matcher.take_expired(Duration::ZERO);
    /// assert!(unpaired.executions.is_empty() && unpaired.reports.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets how many handed-out execution ids the matcher remembers to recognize a late or
    /// repeated report; the oldest is dropped first. A report for a dropped id is treated as
    /// new. It also bounds the combos [`add_combo`](Self::add_combo) remembers, the oldest
    /// dropped first. `0` remembers none of either.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::FillMatcher;
    ///
    /// let matcher = FillMatcher::new().memory(50_000);
    /// ```
    pub fn memory(mut self, capacity: usize) -> Self {
        self.memory = capacity;
        self.evict();
        self
    }

    /// Sets the rule that says whether TWS sends a report for an execution. An execution it
    /// returns `false` for is a [`Fill`] with no commission as soon as it arrives, unless its
    /// report came first. The default is [`ExecutionData::expects_commission_report`], which
    /// covers only what was verified live.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{ExecutionData, FillMatcher, OptionExerciseType};
    ///
    /// // Also expect no report for any exercise type.
    /// let matcher = FillMatcher::new().expects_report(|data: &ExecutionData| {
    ///     data.expects_commission_report() && data.execution.option_exercise_type == OptionExerciseType::None
    /// });
    /// ```
    pub fn expects_report(mut self, rule: fn(&ExecutionData) -> bool) -> Self {
        self.expects_report = rule;
        self
    }

    /// Tells the matcher that order `perm_id` is a combo with these legs, so it returns each
    /// fill of the order as one combo [`Fill`] (see [Combos](Self#combos)). Call it before the
    /// order's executions arrive, with the legs of the open-order echo, which TWS sends before
    /// the first execution. An [`executions`](crate::Client::executions) reply needs no call:
    /// [`pair_executions`](Self::pair_executions) assembles its combo fills from the
    /// executions alone. Only each leg's `contract_id` and `ratio` are used. Calling it again
    /// for the same `perm_id` replaces the legs.
    ///
    /// The matcher remembers the last [`memory`](Self::memory) combos, the oldest dropped
    /// first; with `memory(0)` this has no effect.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::contracts::{ComboLeg, Contract, LegAction, SecurityType};
    /// use ibapi::orders::{Execution, ExecutionData, FillMatcher};
    ///
    /// let mut matcher = FillMatcher::new();
    /// let legs = vec![
    ///     ComboLeg { contract_id: 515416632, ratio: 1, action: LegAction::Buy, ..Default::default() },
    ///     ComboLeg { contract_id: 649180684, ratio: 1, action: LegAction::Sell, ..Default::default() },
    /// ];
    /// matcher.add_combo(1290491156, &legs);
    ///
    /// // The combo-level execution waits for its legs.
    /// let combo = ExecutionData {
    ///     contract: Contract { security_type: SecurityType::Spread, ..Default::default() },
    ///     execution: Execution { execution_id: "0000e1a7.6b0717ad.01.01".to_string(), perm_id: 1290491156, shares: 1.0, ..Default::default() },
    ///     ..Default::default()
    /// };
    /// assert!(matcher.push_execution(combo).is_none());
    /// ```
    pub fn add_combo(&mut self, perm_id: i64, legs: &[ComboLeg]) {
        if self.memory == 0 {
            return;
        }
        let ratios = legs.iter().map(|leg| (leg.contract_id, f64::from(leg.ratio))).collect();
        if self.combos.insert(perm_id, ratios).is_none() {
            self.combo_order.push_back(perm_id);
            self.evict();
        }
    }

    /// Adds an execution. Returns the [`Fill`] when its report has already arrived, or when
    /// the rule expects no report (then with `commission: None`); otherwise holds the
    /// execution and returns `None`. A part of a combo fill ([`add_combo`](Self::add_combo))
    /// returns `None` until it completes the combo fill, then the combo fill.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{CommissionAndFeesReport, Execution, ExecutionData, FillMatcher, ReportMatch};
    ///
    /// let mut matcher = FillMatcher::new();
    /// let report = CommissionAndFeesReport { execution_id: "0000e1a7.0001.01".to_string(), commission_and_fees: 1.25, ..Default::default() };
    /// assert!(matches!(matcher.push_report(report), ReportMatch::Waiting));
    ///
    /// let execution = ExecutionData {
    ///     execution: Execution { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() },
    ///     ..Default::default()
    /// };
    /// let fill = matcher.push_execution(execution).expect("report arrived first");
    /// assert_eq!(fill.commission.map(|report| report.commission_and_fees), Some(1.25));
    /// ```
    pub fn push_execution(&mut self, execution: ExecutionData) -> Option<Fill> {
        self.push_execution_at(execution, Instant::now())
    }

    /// [`push_execution`](Self::push_execution) with the arrival time given rather than read
    /// from the clock.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{Execution, ExecutionData, FillMatcher};
    /// use std::time::{Duration, Instant};
    ///
    /// let start = Instant::now();
    /// let mut matcher = FillMatcher::new();
    /// let execution = ExecutionData {
    ///     execution: Execution { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() },
    ///     ..Default::default()
    /// };
    /// assert!(matcher.push_execution_at(execution, start).is_none());
    /// assert_eq!(matcher.take_expired_at(Duration::from_secs(5), start + Duration::from_secs(5)).executions.len(), 1);
    /// ```
    pub fn push_execution_at(&mut self, execution: ExecutionData, now: Instant) -> Option<Fill> {
        let fill = self.pair_execution(execution, now)?;
        self.gather(fill, now)
    }

    fn pair_execution(&mut self, execution: ExecutionData, now: Instant) -> Option<Fill> {
        let execution_id = &execution.execution.execution_id;
        if let Some(index) = self.reports.iter().position(|(report, _)| report.execution_id == *execution_id) {
            let (report, _) = self.reports.remove(index);
            self.remember(execution_id.clone(), Handout::Reported);
            return Some(Fill {
                execution,
                commission: Some(report),
                legs: Vec::new(),
            });
        }
        if !(self.expects_report)(&execution) {
            self.remember(execution_id.clone(), Handout::Unreported { arrived: now });
            return Some(Fill {
                execution,
                commission: None,
                legs: Vec::new(),
            });
        }
        self.executions.push((execution, now));
        None
    }

    /// Returns `fill` unless it is a part of a combo fill [`add_combo`](Self::add_combo)
    /// announced; then holds it with the other parts and returns the combo fill once complete.
    /// `arrived` is when the fill's execution arrived.
    fn gather(&mut self, fill: Fill, arrived: Instant) -> Option<Fill> {
        let perm_id = fill.execution.execution.perm_id;
        let Some(ratios) = self.combos.get(&perm_id) else {
            return Some(fill);
        };
        let key = fill_key(&fill.execution.execution.execution_id);
        let index = match self.combo_parts.iter().position(|parts| parts.perm_id == perm_id && parts.key == key) {
            Some(index) => index,
            None => {
                self.combo_parts.push(ComboParts {
                    perm_id,
                    key: key.to_string(),
                    execution: None,
                    legs: Vec::new(),
                    arrived,
                });
                self.combo_parts.len() - 1
            }
        };
        let parts = &mut self.combo_parts[index];
        parts.arrived = parts.arrived.min(arrived);
        if fill.execution.contract.security_type == SecurityType::Spread {
            if parts.execution.is_some() {
                return Some(fill);
            }
            parts.execution = Some(fill);
        } else {
            parts.legs.push(fill);
        }
        if !parts.is_complete(ratios) {
            return None;
        }
        self.combo_parts.remove(index).into_fills().pop()
    }

    /// Adds a commission report: pairs it with its waiting execution, recognizes it as late or
    /// repeated, or holds it until its execution arrives. See [`ReportMatch`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{CommissionAndFeesReport, Execution, ExecutionData, FillMatcher, ReportMatch};
    ///
    /// let mut matcher = FillMatcher::new();
    /// let execution = ExecutionData {
    ///     execution: Execution { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() },
    ///     ..Default::default()
    /// };
    /// assert!(matcher.push_execution(execution).is_none());
    ///
    /// let report = CommissionAndFeesReport { execution_id: "0000e1a7.0001.01".to_string(), commission_and_fees: 1.25, ..Default::default() };
    /// match matcher.push_report(report.clone()) {
    ///     ReportMatch::Paired(fill) => println!("{} paid {:?}", fill.execution.execution.execution_id, fill.commission),
    ///     other => panic!("expected a fill, got {other:?}"),
    /// }
    /// assert!(matches!(matcher.push_report(report), ReportMatch::Duplicate(_)));
    /// ```
    pub fn push_report(&mut self, report: CommissionAndFeesReport) -> ReportMatch {
        self.push_report_at(report, Instant::now())
    }

    /// [`push_report`](Self::push_report) with the arrival time given rather than read from
    /// the clock.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{CommissionAndFeesReport, FillMatcher, ReportMatch};
    /// use std::time::{Duration, Instant};
    ///
    /// let start = Instant::now();
    /// let mut matcher = FillMatcher::new();
    /// let report = CommissionAndFeesReport { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() };
    /// assert!(matches!(matcher.push_report_at(report, start), ReportMatch::Waiting));
    /// assert_eq!(matcher.take_expired_at(Duration::from_secs(5), start + Duration::from_secs(5)).reports.len(), 1);
    /// ```
    pub fn push_report_at(&mut self, report: CommissionAndFeesReport, now: Instant) -> ReportMatch {
        if let Some(index) = self
            .executions
            .iter()
            .position(|(execution, _)| execution.execution.execution_id == report.execution_id)
        {
            let (execution, arrived) = self.executions.remove(index);
            self.remember(report.execution_id.clone(), Handout::Reported);
            let fill = Fill {
                execution,
                commission: Some(report),
                legs: Vec::new(),
            };
            return match self.gather(fill, arrived) {
                Some(fill) => ReportMatch::Paired(fill),
                None => ReportMatch::ComboPending,
            };
        }
        if let Some((_, handout)) = self.handouts.get_mut(&report.execution_id) {
            return match mem::replace(handout, Handout::Reported) {
                Handout::Unreported { .. } => ReportMatch::Late(report),
                Handout::Reported => ReportMatch::Duplicate(report),
            };
        }
        if self.reports.iter().any(|(waiting, _)| waiting.execution_id == report.execution_id) {
            return ReportMatch::Duplicate(report);
        }
        self.reports.push((report, now));
        ReportMatch::Waiting
    }

    /// Removes and returns the executions and reports that have waited at least `max_wait`
    /// for their partner, and the combo fills whose first part arrived at least `max_wait`
    /// ago, with the parts that came (in [`Unpaired::fills`]). A leg execution whose report
    /// has not come goes into its combo fill without a commission. The matcher remembers each
    /// returned execution, so its report, if it still comes, is [`ReportMatch::Late`].
    /// `Duration::ZERO` returns everything it holds.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{Execution, ExecutionData, Fill, FillMatcher};
    /// use std::time::Duration;
    ///
    /// let mut matcher = FillMatcher::new();
    /// let execution = ExecutionData {
    ///     execution: Execution { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() },
    ///     ..Default::default()
    /// };
    /// assert!(matcher.push_execution(execution).is_none());
    ///
    /// // No report came: emit the execution without a commission.
    /// for execution in matcher.take_expired(Duration::ZERO).executions {
    ///     let fill = Fill { execution, commission: None, legs: Vec::new() };
    ///     println!("fill without commission: {fill:?}");
    /// }
    /// ```
    pub fn take_expired(&mut self, max_wait: Duration) -> Unpaired {
        self.take_expired_at(max_wait, Instant::now())
    }

    /// [`take_expired`](Self::take_expired) as of `now` rather than the clock.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{Execution, ExecutionData, FillMatcher};
    /// use std::time::{Duration, Instant};
    ///
    /// let start = Instant::now();
    /// let mut matcher = FillMatcher::new();
    /// let execution = ExecutionData {
    ///     execution: Execution { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() },
    ///     ..Default::default()
    /// };
    /// matcher.push_execution_at(execution, start);
    ///
    /// let max_wait = Duration::from_secs(5);
    /// assert!(matcher.take_expired_at(max_wait, start + Duration::from_secs(4)).executions.is_empty());
    /// assert_eq!(matcher.take_expired_at(max_wait, start + max_wait).executions.len(), 1);
    /// ```
    pub fn take_expired_at(&mut self, max_wait: Duration, now: Instant) -> Unpaired {
        let expired = |at: &Instant| now.saturating_duration_since(*at) >= max_wait;
        let (executions, reports) = self.take_where(|_, at| expired(at), |_, at| expired(at));
        self.hand_out(executions, reports, |parts| expired(&parts.arrived))
    }

    /// Returns an execution that [`take_expired`](Self::take_expired) handed out to the
    /// matcher, to wait for its report again from its original arrival time, as if it had
    /// never been taken. For a consumer that took an execution to emit it without a
    /// commission and cannot emit it yet.
    ///
    /// Gives the execution back, unchanged, when the matcher has no record of handing it out
    /// without a report: a report for it has come since (as [`ReportMatch::Late`]; pair the
    /// two yourself), [`memory`](Self::memory) dropped the record, or the matcher never
    /// handed it out.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::orders::{CommissionAndFeesReport, Execution, ExecutionData, FillMatcher, ReportMatch};
    /// use std::time::Duration;
    ///
    /// let mut matcher = FillMatcher::new();
    /// let execution = ExecutionData {
    ///     execution: Execution { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() },
    ///     ..Default::default()
    /// };
    /// matcher.push_execution(execution);
    ///
    /// for execution in matcher.take_expired(Duration::ZERO).executions {
    ///     // Not ready to emit it: wait for its report a while longer.
    ///     assert!(matcher.put_back(execution).is_none());
    /// }
    /// let report = CommissionAndFeesReport { execution_id: "0000e1a7.0001.01".to_string(), ..Default::default() };
    /// assert!(matches!(matcher.push_report(report), ReportMatch::Paired(_)));
    /// ```
    pub fn put_back(&mut self, execution: ExecutionData) -> Option<ExecutionData> {
        let execution_id = &execution.execution.execution_id;
        let Some(&(_, Handout::Unreported { arrived })) = self.handouts.get(execution_id) else {
            return Some(execution);
        };
        self.handouts.remove(execution_id);
        let index = self.executions.partition_point(|(_, at)| *at <= arrived);
        self.executions.insert(index, (execution, arrived));
        None
    }

    /// Pairs the executions and commission reports of an
    /// [`executions`](crate::Client::executions) reply. Returns the fills, in the order their
    /// executions appear in the reply, and the executions and reports of the reply that found
    /// no partner. A report that comes after an execution the rule expects none for fills in
    /// that fill's commission; a repeated report is dropped. A first report for an execution
    /// the matcher handed out without one before the call is returned in
    /// [`Unpaired::reports`], after the reports that found no execution.
    ///
    /// Each combo fill is in the fills at its combo-level execution's place, with the leg
    /// fills of the reply in [`Fill::legs`]. A combo fill with a leg whose report the reply
    /// lacks is in [`Unpaired::fills`], that leg without a commission. A combo announced with
    /// [`add_combo`](Self::add_combo) completes by its legs' ratios instead, and is in
    /// [`Unpaired::fills`] when the reply does not complete it.
    ///
    /// Pass the whole reply: read it with `collect_to_end_within`, which fails rather than
    /// return a partial reply, so an execution whose report was cut off never shows up as
    /// unpaired. The matcher remembers every execution of the reply, so a later copy of one
    /// of its reports on a stream fed to the same matcher is [`ReportMatch::Duplicate`], or
    /// [`ReportMatch::Late`] for an unpaired execution. What it held before is untouched: an
    /// execution of the reply that the matcher was already waiting for (from a live stream)
    /// pairs with the reply's report and is in the fills, or, without one, stays held for the
    /// live report and is in neither the fills nor [`Unpaired`]. An unpaired execution is
    /// most often a fill whose report TWS has not sent yet; see [`Unpaired::executions`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "async")]
    /// # async fn run(client: &ibapi::Client) -> Result<(), ibapi::Error> {
    /// use ibapi::orders::{ExecutionFilter, FillMatcher};
    /// use std::time::Duration;
    ///
    /// let mut subscription = client.executions(ExecutionFilter::default()).await?;
    /// let rows = subscription.collect_to_end_within(Duration::from_secs(10)).await?;
    ///
    /// let mut matcher = FillMatcher::new();
    /// let (fills, unpaired) = matcher.pair_executions(rows);
    /// for fill in &fills {
    ///     let fees = fill.commission.as_ref().map(|report| report.commission_and_fees);
    ///     println!("{} {} @ {}: {fees:?}", fill.execution.contract.symbol, fill.execution.execution.shares, fill.execution.execution.price);
    /// }
    /// println!("{} without a report, {} reports without an execution", unpaired.executions.len(), unpaired.reports.len());
    /// // Keep feeding `matcher` from order_update_stream: reports it has seen come back as Duplicate.
    /// # Ok(())
    /// # }
    /// ```
    pub fn pair_executions(&mut self, rows: impl IntoIterator<Item = Executions>) -> (Vec<Fill>, Unpaired) {
        let now = Instant::now();
        let held: HashSet<String> = self
            .executions
            .iter()
            .map(|(execution, _)| execution.execution.execution_id.clone())
            .collect();
        let mut positions: HashMap<String, usize> = HashMap::new();
        let mut report_ids: HashSet<String> = HashSet::new();
        let mut fills: Vec<Fill> = Vec::new();
        let mut late: Vec<CommissionAndFeesReport> = Vec::new();
        for row in rows {
            match row {
                Executions::ExecutionData(execution) => {
                    let position = positions.len();
                    positions.entry(execution.execution.execution_id.clone()).or_insert(position);
                    if !held.contains(&execution.execution.execution_id) {
                        fills.extend(self.push_execution_at(execution, now));
                    }
                }
                Executions::CommissionAndFeesReport(report) => {
                    report_ids.insert(report.execution_id.clone());
                    match self.push_report_at(report, now) {
                        ReportMatch::Paired(fill) => fills.push(fill),
                        ReportMatch::Late(report) => match fills.iter_mut().find(|fill| fill.execution.execution.execution_id == report.execution_id)
                        {
                            Some(fill) => fill.commission = Some(report),
                            None => late.push(report),
                        },
                        ReportMatch::Waiting | ReportMatch::ComboPending | ReportMatch::Duplicate(_) => {}
                    }
                }
            }
        }
        fills.sort_by_key(|fill| positions.get(&fill.execution.execution.execution_id).copied().unwrap_or(usize::MAX));
        let (executions, reports) = self.take_where(
            |execution, _| {
                let execution_id = &execution.execution.execution_id;
                positions.contains_key(execution_id) && !held.contains(execution_id)
            },
            |report, _| report_ids.contains(&report.execution_id),
        );
        let mut unpaired = self.hand_out(executions, reports, |parts| {
            parts
                .execution
                .iter()
                .chain(&parts.legs)
                .any(|fill| positions.contains_key(&fill.execution.execution.execution_id))
        });
        unpaired.reports.extend(late);
        let fills = self.attach_legs(fills, &mut unpaired, &positions);
        (fills, unpaired)
    }

    /// Puts each leg fill of a reply into the combo fill of its combo-level execution, for the
    /// combos [`add_combo`](Self::add_combo) did not announce: a leg is an execution of another
    /// security type with the combo-level execution's `perm_id` and [`fill_key`]. A leg from
    /// `unpaired.executions` joins without a commission and moves its combo fill to
    /// `unpaired.fills`. Legs keep their order in the reply (`positions`).
    fn attach_legs(&self, fills: Vec<Fill>, unpaired: &mut Unpaired, positions: &HashMap<String, usize>) -> Vec<Fill> {
        let part_of = |execution: &ExecutionData| (execution.execution.perm_id, fill_key(&execution.execution.execution_id).to_string());
        let combo_keys: HashSet<(i64, String)> = fills
            .iter()
            .map(|fill| &fill.execution)
            .filter(|execution| execution.contract.security_type == SecurityType::Spread && !self.combos.contains_key(&execution.execution.perm_id))
            .map(part_of)
            .collect();
        if combo_keys.is_empty() {
            return fills;
        }
        let is_leg = |execution: &ExecutionData| execution.contract.security_type != SecurityType::Spread && combo_keys.contains(&part_of(execution));

        let mut legs: HashMap<(i64, String), Vec<Fill>> = HashMap::new();
        let mut incomplete: HashSet<(i64, String)> = HashSet::new();
        let mut kept = Vec::with_capacity(fills.len());
        for fill in fills {
            if is_leg(&fill.execution) {
                legs.entry(part_of(&fill.execution)).or_default().push(fill);
            } else {
                kept.push(fill);
            }
        }
        for execution in mem::take(&mut unpaired.executions) {
            if !is_leg(&execution) {
                unpaired.executions.push(execution);
                continue;
            }
            let key = part_of(&execution);
            incomplete.insert(key.clone());
            legs.entry(key).or_default().push(Fill {
                execution,
                commission: None,
                legs: Vec::new(),
            });
        }

        let mut assembled = Vec::with_capacity(kept.len());
        for mut fill in kept {
            if fill.execution.contract.security_type == SecurityType::Spread && fill.legs.is_empty() {
                let key = part_of(&fill.execution);
                if let Some(mut found) = legs.remove(&key) {
                    found.sort_by_key(|leg| positions.get(&leg.execution.execution.execution_id).copied());
                    fill.legs = found;
                }
                if incomplete.contains(&key) {
                    unpaired.fills.push(fill);
                    continue;
                }
            }
            assembled.push(fill);
        }
        assembled
    }

    fn take_where(
        &mut self,
        mut take_execution: impl FnMut(&ExecutionData, &Instant) -> bool,
        mut take_report: impl FnMut(&CommissionAndFeesReport, &Instant) -> bool,
    ) -> (Vec<(ExecutionData, Instant)>, Vec<CommissionAndFeesReport>) {
        let (taken, kept): (Vec<_>, Vec<_>) = mem::take(&mut self.executions)
            .into_iter()
            .partition(|(execution, at)| take_execution(execution, at));
        self.executions = kept;
        let (taken_reports, kept_reports): (Vec<_>, Vec<_>) =
            mem::take(&mut self.reports).into_iter().partition(|(report, at)| take_report(report, at));
        self.reports = kept_reports;

        for (execution, arrived) in &taken {
            self.remember(execution.execution.execution_id.clone(), Handout::Unreported { arrived: *arrived });
        }
        (taken, taken_reports.into_iter().map(|(report, _)| report).collect())
    }

    /// Builds the [`Unpaired`] for executions and reports [`take_where`](Self::take_where)
    /// took: puts each combo leg execution into its combo fill without a commission, then
    /// hands out the combo fills `give_up` selects.
    fn hand_out(
        &mut self,
        executions: Vec<(ExecutionData, Instant)>,
        reports: Vec<CommissionAndFeesReport>,
        mut give_up: impl FnMut(&ComboParts) -> bool,
    ) -> Unpaired {
        let mut unpaired = Unpaired {
            reports,
            ..Default::default()
        };
        for (execution, arrived) in executions {
            if self.combos.contains_key(&execution.execution.perm_id) {
                let fill = Fill {
                    execution,
                    commission: None,
                    legs: Vec::new(),
                };
                unpaired.fills.extend(self.gather(fill, arrived));
            } else {
                unpaired.executions.push(execution);
            }
        }
        let (given_up, kept): (Vec<_>, Vec<_>) = mem::take(&mut self.combo_parts).into_iter().partition(|parts| give_up(parts));
        self.combo_parts = kept;
        unpaired.fills.extend(given_up.into_iter().flat_map(ComboParts::into_fills));
        unpaired
    }

    fn remember(&mut self, execution_id: String, handout: Handout) {
        if self.memory == 0 {
            return;
        }
        self.handout_count += 1;
        let sequence = self.handout_count;
        self.handout_order.push_back((sequence, execution_id.clone()));
        self.handouts.insert(execution_id, (sequence, handout));
        self.evict();
    }

    fn evict(&mut self) {
        while self.combo_order.len() > self.memory {
            if let Some(perm_id) = self.combo_order.pop_front() {
                self.combos.remove(&perm_id);
            }
        }
        while self.handouts.len() > self.memory {
            let Some((sequence, execution_id)) = self.handout_order.pop_front() else {
                break;
            };
            if self.handouts.get(&execution_id).is_some_and(|(current, _)| *current == sequence) {
                self.handouts.remove(&execution_id);
            }
        }
        if self.handout_order.len() > 2 * self.memory {
            let handouts = &self.handouts;
            self.handout_order
                .retain(|(sequence, execution_id)| handouts.get(execution_id).is_some_and(|(current, _)| current == sequence));
        }
    }
}

#[cfg(test)]
#[path = "fills_tests.rs"]
mod tests;
