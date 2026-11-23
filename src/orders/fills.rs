use std::mem;
use std::time::{Duration, Instant};

use super::{CommissionAndFeesReport, ExecutionData, Executions};

/// An execution with its commission report, joined by `execution_id`.
///
/// Built by [`FillMatcher`] and [`pair_executions`].
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
/// };
/// let fees = fill.commission.as_ref().map(|report| report.commission_and_fees);
/// println!("{} shares, commissions and fees {fees:?}", fill.execution.execution.shares);
/// ```
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Clone, Debug)]
pub struct Fill {
    /// The execution and its contract.
    pub execution: ExecutionData,
    /// The execution's commission report. `None` when TWS sends none for this kind of
    /// execution (see [`Execution::expects_commission_report`](super::Execution::expects_commission_report)),
    /// or when [`FillMatcher::take_expired`] gave up waiting for it.
    pub commission: Option<CommissionAndFeesReport>,
}

/// Executions and commission reports that found no partner.
///
/// Returned by [`FillMatcher::take_expired`] and [`pair_executions`].
///
/// # Examples
///
/// ```
/// use ibapi::orders::{pair_executions, Executions};
///
/// let rows: Vec<Executions> = Vec::new(); // an executions() reply, read to its end
/// let (_fills, unpaired) = pair_executions(rows);
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
    /// Executions whose commission report did not arrive, in arrival order.
    pub executions: Vec<ExecutionData>,
    /// Commission reports whose execution did not arrive, in arrival order.
    pub reports: Vec<CommissionAndFeesReport>,
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
///     ReportMatch::Late(report) => println!("execution already taken without it: {report:?}"),
/// }
/// ```
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ReportMatch {
    /// The execution was waiting; here is the joined fill.
    Paired(Fill),
    /// The execution has not arrived; the matcher holds the report until it does or
    /// [`FillMatcher::take_expired`] returns it.
    Waiting,
    /// The execution was already handed out without a commission, by
    /// [`FillMatcher::take_expired`] or because TWS sends no report for its kind.
    Late(CommissionAndFeesReport),
}

/// Joins each [`ExecutionData`] with its [`CommissionAndFeesReport`] as both arrive on a stream.
///
/// TWS sends an execution and its commission report as two messages that share only
/// `execution_id`, with other frames between them. Feed the matcher every `ExecutionData` and
/// `CommissionAndFeesReport` from [`OrderUpdate`](super::OrderUpdate),
/// [`PlaceOrder`](super::PlaceOrder) or [`Executions`] items, in either order: it returns the
/// [`Fill`] once both halves are known.
///
/// Some executions never get a report. TWS sends none for an option exercise, and the matcher
/// returns those fills at once with no commission (see
/// [`Execution::expects_commission_report`](super::Execution::expects_commission_report)).
/// For any other execution whose report does not come, call
/// [`take_expired`](Self::take_expired) periodically: it returns the executions and reports that
/// have waited at least the duration you choose.
///
/// The matcher holds each execution and early report until it pairs or `take_expired` returns
/// it, and remembers the id of each execution handed out without a report until a later
/// `take_expired` call finds it at least that old. Calling `take_expired` regularly is what
/// bounds its size.
///
/// It is plain data: no I/O, no tasks, the same from sync and async code. To pair a whole
/// [`executions`](crate::Client::executions) reply, use [`pair_executions`].
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
///             for execution in matcher.take_expired(Duration::from_secs(5)).executions {
///                 println!("fill without commission: {execution:?}");
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
///             ReportMatch::Waiting => {}
///             ReportMatch::Late(report) => println!("late commission: {report:?}"),
///         },
///         SubscriptionItem::Data(update) => println!("update: {update:?}"),
///         SubscriptionItem::Notice(notice) => println!("notice: {}", notice.message),
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default)]
pub struct FillMatcher {
    executions: Vec<(ExecutionData, Instant)>,
    reports: Vec<(CommissionAndFeesReport, Instant)>,
    taken: Vec<(String, Instant)>,
}

impl FillMatcher {
    /// Creates an empty matcher.
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

    /// Adds an execution. Returns the [`Fill`] when its report has already arrived, or when
    /// TWS sends no report for this kind of execution (then with `commission: None`);
    /// otherwise holds the execution and returns `None`.
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

    fn push_execution_at(&mut self, execution: ExecutionData, now: Instant) -> Option<Fill> {
        let execution_id = &execution.execution.execution_id;
        if let Some(index) = self.reports.iter().position(|(report, _)| report.execution_id == *execution_id) {
            let (report, _) = self.reports.remove(index);
            return Some(Fill {
                execution,
                commission: Some(report),
            });
        }
        if !execution.execution.expects_commission_report() {
            self.taken.push((execution_id.clone(), now));
            return Some(Fill { execution, commission: None });
        }
        self.executions.push((execution, now));
        None
    }

    /// Adds a commission report: pairs it with its waiting execution, holds it until the
    /// execution arrives, or reports it as late when the execution was already handed out
    /// without a commission.
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
    /// match matcher.push_report(report) {
    ///     ReportMatch::Paired(fill) => println!("{} paid {:?}", fill.execution.execution.execution_id, fill.commission),
    ///     other => panic!("expected a fill, got {other:?}"),
    /// }
    /// ```
    pub fn push_report(&mut self, report: CommissionAndFeesReport) -> ReportMatch {
        self.push_report_at(report, Instant::now())
    }

    fn push_report_at(&mut self, report: CommissionAndFeesReport, now: Instant) -> ReportMatch {
        if let Some(index) = self
            .executions
            .iter()
            .position(|(execution, _)| execution.execution.execution_id == report.execution_id)
        {
            let (execution, _) = self.executions.remove(index);
            return ReportMatch::Paired(Fill {
                execution,
                commission: Some(report),
            });
        }
        if let Some(index) = self.taken.iter().position(|(execution_id, _)| *execution_id == report.execution_id) {
            self.taken.remove(index);
            return ReportMatch::Late(report);
        }
        self.reports.push((report, now));
        ReportMatch::Waiting
    }

    /// Removes and returns the executions and reports that have waited at least `max_wait`
    /// for their partner. A report for a returned execution that arrives later comes back from
    /// [`push_report`](Self::push_report) as [`ReportMatch::Late`], until a later call finds
    /// the hand-out at least `max_wait` old.
    ///
    /// `Duration::ZERO` returns everything the matcher holds.
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
    ///     let fill = Fill { execution, commission: None };
    ///     println!("fill without commission: {fill:?}");
    /// }
    /// ```
    pub fn take_expired(&mut self, max_wait: Duration) -> Unpaired {
        self.take_expired_at(max_wait, Instant::now())
    }

    fn take_expired_at(&mut self, max_wait: Duration, now: Instant) -> Unpaired {
        let expired = |at: &Instant| now.saturating_duration_since(*at) >= max_wait;

        self.taken.retain(|(_, at)| !expired(at));

        let (expired_executions, executions): (Vec<_>, Vec<_>) = mem::take(&mut self.executions).into_iter().partition(|(_, at)| expired(at));
        self.executions = executions;
        let (expired_reports, reports): (Vec<_>, Vec<_>) = mem::take(&mut self.reports).into_iter().partition(|(_, at)| expired(at));
        self.reports = reports;

        self.taken.extend(
            expired_executions
                .iter()
                .map(|(execution, _)| (execution.execution.execution_id.clone(), now)),
        );

        Unpaired {
            executions: expired_executions.into_iter().map(|(execution, _)| execution).collect(),
            reports: expired_reports.into_iter().map(|(report, _)| report).collect(),
        }
    }
}

/// Pairs the executions and commission reports of an [`executions`](crate::Client::executions)
/// reply. Returns the fills, in the order they paired, and what found no partner.
///
/// Pass the whole reply: read it with `collect_to_end_within`, which fails rather than return
/// a partial reply, so an execution whose report was cut off never shows up as unpaired. An
/// option exercise is a fill with no commission, since TWS sends none for it. A report for an
/// execution handed out that way is returned in [`Unpaired::reports`].
///
/// # Examples
///
/// ```no_run
/// # #[cfg(feature = "async")]
/// # async fn run(client: &ibapi::Client) -> Result<(), ibapi::Error> {
/// use ibapi::orders::{pair_executions, ExecutionFilter};
/// use std::time::Duration;
///
/// let mut subscription = client.executions(ExecutionFilter::default()).await?;
/// let rows = subscription.collect_to_end_within(Duration::from_secs(10)).await?;
///
/// let (fills, unpaired) = pair_executions(rows);
/// for fill in &fills {
///     let fees = fill.commission.as_ref().map(|report| report.commission_and_fees);
///     println!("{} {} @ {}: {fees:?}", fill.execution.contract.symbol, fill.execution.execution.shares, fill.execution.execution.price);
/// }
/// println!("{} without a report, {} reports without an execution", unpaired.executions.len(), unpaired.reports.len());
/// # Ok(())
/// # }
/// ```
pub fn pair_executions(rows: impl IntoIterator<Item = Executions>) -> (Vec<Fill>, Unpaired) {
    let mut matcher = FillMatcher::new();
    let mut fills = Vec::new();
    let mut late = Vec::new();
    for row in rows {
        match row {
            Executions::ExecutionData(execution) => fills.extend(matcher.push_execution(execution)),
            Executions::CommissionAndFeesReport(report) => match matcher.push_report(report) {
                ReportMatch::Paired(fill) => fills.push(fill),
                ReportMatch::Waiting => {}
                ReportMatch::Late(report) => late.push(report),
            },
        }
    }
    let mut unpaired = matcher.take_expired(Duration::ZERO);
    unpaired.reports.extend(late);
    (fills, unpaired)
}

#[cfg(test)]
#[path = "fills_tests.rs"]
mod tests;
