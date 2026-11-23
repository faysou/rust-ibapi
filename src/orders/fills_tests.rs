use super::*;
use crate::contracts::{LegAction, SecurityType};
use crate::orders::{Execution, OptionExerciseType};

const MAX_WAIT: Duration = Duration::from_secs(5);

fn execution(execution_id: &str) -> ExecutionData {
    ExecutionData {
        request_id: 9001,
        execution: Execution {
            execution_id: execution_id.to_string(),
            shares: 3.0,
            price: 5812.25,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn exercise(execution_id: &str) -> ExecutionData {
    let mut data = execution(execution_id);
    data.execution.option_exercise_type = OptionExerciseType::Exercise;
    data
}

fn combo(execution_id: &str) -> ExecutionData {
    let mut data = execution(execution_id);
    data.contract.security_type = SecurityType::Spread;
    data
}

fn no_report_for_combos(data: &ExecutionData) -> bool {
    data.contract.security_type != SecurityType::Spread
}

fn report(execution_id: &str, commission_and_fees: f64) -> CommissionAndFeesReport {
    CommissionAndFeesReport {
        execution_id: execution_id.to_string(),
        commission_and_fees,
        currency: "USD".to_string(),
        ..Default::default()
    }
}

fn assert_fill(fill: &Fill, execution_id: &str, commission_and_fees: Option<f64>) {
    assert_eq!(fill.execution.execution.execution_id, execution_id, "fill execution_id");
    assert_eq!(fill.execution.request_id, 9001, "fill request_id");
    assert_eq!(fill.execution.execution.shares, 3.0, "fill shares");
    match (&fill.commission, commission_and_fees) {
        (Some(report), Some(expected)) => {
            assert_eq!(report.execution_id, execution_id, "report execution_id");
            assert_eq!(report.commission_and_fees, expected, "report commission_and_fees");
            assert_eq!(report.currency, "USD", "report currency");
        }
        (None, None) => {}
        (actual, expected) => panic!("commission {actual:?}, expected {expected:?}"),
    }
}

fn ids(executions: &[ExecutionData]) -> Vec<&str> {
    executions.iter().map(|data| data.execution.execution_id.as_str()).collect()
}

fn report_ids(reports: &[CommissionAndFeesReport]) -> Vec<&str> {
    reports.iter().map(|report| report.execution_id.as_str()).collect()
}

fn fill_ids(fills: &[Fill]) -> Vec<(&str, Option<f64>)> {
    fills
        .iter()
        .map(|fill| {
            (
                fill.execution.execution.execution_id.as_str(),
                fill.commission.as_ref().map(|report| report.commission_and_fees),
            )
        })
        .collect()
}

fn assert_nothing_waits(matcher: &FillMatcher) {
    assert!(matcher.executions.is_empty(), "executions held: {:?}", matcher.executions);
    assert!(matcher.reports.is_empty(), "reports held: {:?}", matcher.reports);
}

fn assert_late(result: ReportMatch, execution_id: &str, commission_and_fees: f64) {
    match result {
        ReportMatch::Late(report) => {
            assert_eq!(report.execution_id, execution_id);
            assert_eq!(report.commission_and_fees, commission_and_fees);
        }
        other => panic!("expected Late, got {other:?}"),
    }
}

fn assert_duplicate(result: ReportMatch, execution_id: &str) {
    match result {
        ReportMatch::Duplicate(report) => assert_eq!(report.execution_id, execution_id),
        other => panic!("expected Duplicate, got {other:?}"),
    }
}

#[test]
fn execution_then_report_pairs() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    let pushed = matcher.push_execution_at(execution("e1"), now);
    let paired = matcher.push_report_at(report("e1", 1.25), now);

    assert!(pushed.is_none(), "execution alone must wait, got {pushed:?}");
    match paired {
        ReportMatch::Paired(fill) => assert_fill(&fill, "e1", Some(1.25)),
        other => panic!("expected Paired, got {other:?}"),
    }
    assert_nothing_waits(&matcher);
}

#[test]
fn report_then_execution_pairs() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    let pushed = matcher.push_report_at(report("e1", 1.25), now);
    let fill = matcher.push_execution_at(execution("e1"), now).expect("report was waiting");

    assert!(matches!(pushed, ReportMatch::Waiting), "report alone must wait, got {pushed:?}");
    assert_fill(&fill, "e1", Some(1.25));
    assert_nothing_waits(&matcher);
}

#[test]
fn interleaved_ids_pair_by_execution_id() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    assert!(matcher.push_execution_at(execution("e1"), now).is_none());
    assert!(matcher.push_execution_at(execution("e2"), now).is_none());
    let second = matcher.push_report_at(report("e2", 2.5), now);
    let first = matcher.push_report_at(report("e1", 1.25), now);

    match (second, first) {
        (ReportMatch::Paired(second), ReportMatch::Paired(first)) => {
            assert_fill(&second, "e2", Some(2.5));
            assert_fill(&first, "e1", Some(1.25));
        }
        other => panic!("expected two Paired, got {other:?}"),
    }
    assert_nothing_waits(&matcher);
}

#[test]
fn take_expired_waits_for_max_wait() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), start).is_none());
    assert!(matches!(matcher.push_report_at(report("r1", 0.5), start), ReportMatch::Waiting));
    assert!(matcher.push_execution_at(execution("e2"), start + Duration::from_secs(2)).is_none());

    let early = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT - Duration::from_millis(1));
    let due = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);

    assert!(early.executions.is_empty(), "nothing has waited 5 s yet: {:?}", ids(&early.executions));
    assert!(early.reports.is_empty(), "nothing has waited 5 s yet: {:?}", report_ids(&early.reports));
    assert_eq!(ids(&due.executions), vec!["e1"], "only e1 has waited 5 s");
    assert_eq!(report_ids(&due.reports), vec!["r1"], "r1 has waited 5 s");
    assert_eq!(matcher.executions.len(), 1, "e2 still waits");
    assert_eq!(matcher.executions[0].0.execution.execution_id, "e2");
}

#[test]
fn take_expired_zero_returns_everything() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), now).is_none());
    assert!(matches!(matcher.push_report_at(report("r1", 0.5), now), ReportMatch::Waiting));

    let unpaired = matcher.take_expired_at(Duration::ZERO, now);

    assert_eq!(ids(&unpaired.executions), vec!["e1"]);
    assert_eq!(report_ids(&unpaired.reports), vec!["r1"]);
    assert_nothing_waits(&matcher);
}

#[test]
fn report_after_take_is_late_then_duplicate_however_late() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), start).is_none());
    let taken = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);
    let much_later = start + Duration::from_secs(3600);
    matcher.take_expired_at(MAX_WAIT, much_later);

    let late = matcher.push_report_at(report("e1", 1.25), much_later);
    let again = matcher.push_report_at(report("e1", 1.25), much_later);
    let third = matcher.push_report_at(report("e1", 1.25), much_later + MAX_WAIT);

    assert_eq!(ids(&taken.executions), vec!["e1"]);
    assert_late(late, "e1", 1.25);
    assert_duplicate(again, "e1");
    assert_duplicate(third, "e1");
    assert_nothing_waits(&matcher);
}

#[test]
fn repeated_report_for_paired_execution_is_duplicate_and_never_held() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), now).is_none());
    assert!(matches!(matcher.push_report_at(report("e1", 1.25), now), ReportMatch::Paired(_)));

    let repeated = matcher.push_report_at(report("e1", 1.25), now);
    let leftover = matcher.take_expired_at(Duration::ZERO, now);

    assert_duplicate(repeated, "e1");
    assert!(
        leftover.reports.is_empty(),
        "a repeated report is never handed out: {:?}",
        leftover.reports
    );
    assert!(leftover.executions.is_empty());
}

#[test]
fn second_copy_of_a_waiting_report_is_duplicate() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    let first = matcher.push_report_at(report("e1", 1.25), now);
    let second = matcher.push_report_at(report("e1", 1.25), now);
    let fill = matcher.push_execution_at(execution("e1"), now).expect("report was waiting");

    assert!(matches!(first, ReportMatch::Waiting), "{first:?}");
    assert_duplicate(second, "e1");
    assert_fill(&fill, "e1", Some(1.25));
    assert_nothing_waits(&matcher);
}

#[test]
fn memory_drops_the_oldest_handout_first() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new().memory(2);
    for execution_id in ["e1", "e2", "e3"] {
        assert!(matcher.push_execution_at(execution(execution_id), now).is_none());
        assert!(matches!(matcher.push_report_at(report(execution_id, 1.0), now), ReportMatch::Paired(_)));
    }

    let forgotten = matcher.push_report_at(report("e1", 1.0), now);
    let remembered = matcher.push_report_at(report("e2", 1.0), now);

    assert!(matches!(forgotten, ReportMatch::Waiting), "e1 was dropped from memory: {forgotten:?}");
    assert_duplicate(remembered, "e2");
    assert_eq!(matcher.handouts.len(), 2, "memory holds at most 2 ids");
}

#[test]
fn memory_zero_remembers_nothing() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new().memory(0);
    assert!(matcher.push_execution_at(execution("e1"), now).is_none());
    matcher.take_expired_at(Duration::ZERO, now);

    let report = matcher.push_report_at(report("e1", 1.25), now);

    assert!(matches!(report, ReportMatch::Waiting), "{report:?}");
    assert!(matcher.handouts.is_empty() && matcher.handout_order.is_empty());
}

#[test]
fn shrinking_memory_drops_the_oldest_at_once() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();
    for execution_id in ["e1", "e2", "e3"] {
        assert!(matcher.push_execution_at(execution(execution_id), now).is_none());
    }
    matcher.take_expired_at(Duration::ZERO, now);

    let mut matcher = matcher.memory(1);

    assert!(matches!(matcher.push_report_at(report("e2", 1.0), now), ReportMatch::Waiting));
    assert_late(matcher.push_report_at(report("e3", 1.0), now), "e3", 1.0);
}

#[test]
fn handout_order_stays_bounded_through_put_back_cycles() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new().memory(4);
    assert!(matcher.push_execution_at(execution("e1"), start).is_none());

    for _ in 0..100 {
        let taken = matcher.take_expired_at(Duration::ZERO, start);
        let execution = taken.executions.into_iter().next().expect("e1 is due");
        assert!(matcher.put_back(execution).is_none());
    }

    assert!(matcher.handout_order.len() <= 9, "queue length {}", matcher.handout_order.len());
    assert!(matcher.handouts.is_empty(), "e1 waits again, so it is not a handout");
}

#[test]
fn put_back_keeps_the_original_arrival_time() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), start).is_none());
    assert!(matcher.push_execution_at(execution("e2"), start + Duration::from_secs(1)).is_none());
    let taken = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);

    let put_back = matcher.put_back(taken.executions.into_iter().next().expect("e1 is due"));
    let order: Vec<String> = matcher.executions.iter().map(|(data, _)| data.execution.execution_id.clone()).collect();
    let again = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);

    assert!(put_back.is_none());
    assert_eq!(order, vec!["e1", "e2"], "put back in arrival order");
    assert_eq!(ids(&again.executions), vec!["e1"], "its wait did not restart");
}

#[test]
fn put_back_execution_pairs_with_its_report() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), start).is_none());
    let taken = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);
    assert!(matcher.put_back(taken.executions.into_iter().next().expect("e1 is due")).is_none());

    let paired = matcher.push_report_at(report("e1", 1.25), start + MAX_WAIT);

    match paired {
        ReportMatch::Paired(fill) => assert_fill(&fill, "e1", Some(1.25)),
        other => panic!("expected Paired, got {other:?}"),
    }
    assert_nothing_waits(&matcher);
}

#[test]
fn put_back_gives_back_what_it_cannot_restore() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), now).is_none());
    let taken = matcher.take_expired_at(Duration::ZERO, now).executions;
    assert_late(matcher.push_report_at(report("e1", 1.25), now), "e1", 1.25);

    let reported = matcher.put_back(taken.into_iter().next().expect("e1 is due"));
    let never_taken = matcher.put_back(execution("e9"));

    assert_eq!(reported.map(|data| data.execution.execution_id), Some("e1".to_string()));
    assert_eq!(never_taken.map(|data| data.execution.execution_id), Some("e9".to_string()));
    assert_nothing_waits(&matcher);
}

#[test]
fn exercise_is_a_fill_without_commission_at_once() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    let fill = matcher.push_execution_at(exercise("x1"), now).expect("an exercise gets no report");
    let late = matcher.push_report_at(report("x1", 0.0), now);

    assert_fill(&fill, "x1", None);
    assert_eq!(fill.execution.execution.option_exercise_type, OptionExerciseType::Exercise);
    assert_late(late, "x1", 0.0);
}

#[test]
fn exercise_pairs_with_a_report_that_came_first() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matches!(matcher.push_report_at(report("x1", 0.75), now), ReportMatch::Waiting));

    let fill = matcher.push_execution_at(exercise("x1"), now).expect("report was waiting");

    assert_fill(&fill, "x1", Some(0.75));
    assert_nothing_waits(&matcher);
}

#[test]
fn caller_rule_returns_the_fill_at_once_and_remembers_it() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new().expects_report(no_report_for_combos);

    let fill = matcher.push_execution_at(combo("b1"), now).expect("the rule expects no report");
    let normal = matcher.push_execution_at(execution("e1"), now);
    let exercise_waits = matcher.push_execution_at(exercise("x1"), now);
    let late = matcher.push_report_at(report("b1", 0.5), now);
    let repeated = matcher.push_report_at(report("b1", 0.5), now);

    assert_fill(&fill, "b1", None);
    assert!(normal.is_none(), "{normal:?}");
    assert!(exercise_waits.is_none(), "the caller's rule replaces the default: {exercise_waits:?}");
    assert_late(late, "b1", 0.5);
    assert_duplicate(repeated, "b1");
}

#[test]
fn expects_commission_report_is_false_for_exercise_and_combo() {
    let cases = [
        (OptionExerciseType::None, SecurityType::Future, true),
        (OptionExerciseType::Exercise, SecurityType::Option, false),
        (OptionExerciseType::Lapse, SecurityType::Option, true),
        (OptionExerciseType::DoNothing, SecurityType::Option, true),
        (OptionExerciseType::Assigned, SecurityType::Option, true),
        (OptionExerciseType::AutoexerciseClearing, SecurityType::Option, true),
        (OptionExerciseType::Expired, SecurityType::Option, true),
        (OptionExerciseType::AutoexerciseTrading, SecurityType::Option, true),
        (OptionExerciseType::Netting, SecurityType::Option, true),
        (OptionExerciseType::Unknown(7), SecurityType::Option, true),
        (OptionExerciseType::None, SecurityType::Spread, false),
    ];
    for (option_exercise_type, security_type, expected) in cases {
        let mut data = execution("e1");
        data.execution.option_exercise_type = option_exercise_type;
        data.contract.security_type = security_type.clone();
        assert_eq!(data.expects_commission_report(), expected, "{option_exercise_type:?} {security_type:?}");
    }
}

#[test]
fn pair_executions_returns_fills_in_execution_order_and_leftovers() {
    let rows = vec![
        Executions::CommissionAndFeesReport(report("e2", 2.5)),
        Executions::ExecutionData(execution("e1")),
        Executions::ExecutionData(execution("e2")),
        Executions::ExecutionData(execution("e3")),
        Executions::CommissionAndFeesReport(report("e1", 1.25)),
        Executions::CommissionAndFeesReport(report("e1", 1.25)),
        Executions::ExecutionData(exercise("x1")),
        Executions::CommissionAndFeesReport(report("r9", 0.5)),
        Executions::CommissionAndFeesReport(report("x1", 0.25)),
    ];
    let mut matcher = FillMatcher::new();

    let (fills, unpaired) = matcher.pair_executions(rows);

    assert_eq!(
        fill_ids(&fills),
        vec![("e1", Some(1.25)), ("e2", Some(2.5)), ("x1", Some(0.25))],
        "execution order; the exercise's report fills in its commission; the repeated e1 report is dropped"
    );
    assert_eq!(ids(&unpaired.executions), vec!["e3"]);
    assert_eq!(report_ids(&unpaired.reports), vec!["r9"]);
    assert_nothing_waits(&matcher);
}

#[test]
fn pair_executions_applies_the_caller_rule() {
    let rows = vec![
        Executions::ExecutionData(combo("b1")),
        Executions::ExecutionData(execution("e1")),
        Executions::CommissionAndFeesReport(report("e1", 1.25)),
    ];
    let mut matcher = FillMatcher::new().expects_report(no_report_for_combos);

    let (fills, unpaired) = matcher.pair_executions(rows);

    assert_eq!(fill_ids(&fills), vec![("b1", None), ("e1", Some(1.25))]);
    assert!(unpaired.executions.is_empty() && unpaired.reports.is_empty());
}

#[test]
fn stream_copies_after_pair_executions_are_recognized() {
    let rows = vec![
        Executions::ExecutionData(execution("e1")),
        Executions::CommissionAndFeesReport(report("e1", 1.25)),
        Executions::ExecutionData(execution("e2")),
    ];
    let mut matcher = FillMatcher::new();
    let (_fills, unpaired) = matcher.pair_executions(rows);
    let now = Instant::now();

    let copy = matcher.push_report_at(report("e1", 1.25), now);
    let late = matcher.push_report_at(report("e2", 2.5), now);

    assert_eq!(ids(&unpaired.executions), vec!["e2"]);
    assert_duplicate(copy, "e1");
    assert_late(late, "e2", 2.5);
    assert_nothing_waits(&matcher);
}

#[test]
fn pair_executions_leaves_earlier_holdings_alone() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("old"), start).is_none());
    matcher.take_expired_at(Duration::ZERO, start);
    assert!(matcher.push_execution_at(execution("live"), start).is_none());
    let rows = vec![
        Executions::ExecutionData(execution("e1")),
        Executions::CommissionAndFeesReport(report("old", 0.5)),
    ];

    let (fills, unpaired) = matcher.pair_executions(rows);

    assert!(fills.is_empty(), "{fills:?}");
    assert_eq!(ids(&unpaired.executions), vec!["e1"], "the live execution is not part of the reply");
    assert_eq!(
        report_ids(&unpaired.reports),
        vec!["old"],
        "first report for an execution handed out before the call"
    );
    assert_eq!(matcher.executions.len(), 1);
    assert_eq!(matcher.executions[0].0.execution.execution_id, "live");
}

#[test]
fn pair_executions_of_empty_reply() {
    let (fills, unpaired) = FillMatcher::new().pair_executions(Vec::new());

    assert!(fills.is_empty());
    assert!(unpaired.executions.is_empty());
    assert!(unpaired.reports.is_empty());
}

const PERM_ID: i64 = 1290491156;
const ESZ6: i32 = 515416632;
const ESH7: i32 = 649180684;

fn combo_legs(near_ratio: i32, far_ratio: i32) -> Vec<ComboLeg> {
    vec![
        ComboLeg {
            contract_id: ESZ6,
            ratio: near_ratio,
            action: LegAction::Buy,
            ..Default::default()
        },
        ComboLeg {
            contract_id: ESH7,
            ratio: far_ratio,
            action: LegAction::Sell,
            ..Default::default()
        },
    ]
}

fn combo_matcher(near_ratio: i32, far_ratio: i32) -> FillMatcher {
    let mut matcher = FillMatcher::new();
    matcher.add_combo(PERM_ID, &combo_legs(near_ratio, far_ratio));
    matcher
}

fn bag(execution_id: &str, shares: f64) -> ExecutionData {
    let mut data = combo(execution_id);
    data.contract.contract_id = 16472979;
    data.execution.perm_id = PERM_ID;
    data.execution.shares = shares;
    data.execution.price = -69.65;
    data
}

fn leg(execution_id: &str, contract_id: i32, shares: f64) -> ExecutionData {
    let mut data = execution(execution_id);
    data.contract.security_type = SecurityType::Future;
    data.contract.contract_id = contract_id;
    data.execution.perm_id = PERM_ID;
    data.execution.shares = shares;
    data
}

/// (execution id, contract id, shares, commission) of each leg fill.
fn leg_fills(fill: &Fill) -> Vec<(&str, i32, f64, Option<f64>)> {
    fill.legs
        .iter()
        .map(|leg| {
            (
                leg.execution.execution.execution_id.as_str(),
                leg.execution.contract.contract_id,
                leg.execution.execution.shares,
                leg.commission.as_ref().map(|report| report.commission_and_fees),
            )
        })
        .collect()
}

fn assert_combo_fill(fill: &Fill, execution_id: &str, shares: f64) {
    assert_eq!(fill.execution.execution.execution_id, execution_id, "combo execution_id");
    assert_eq!(fill.execution.contract.security_type, SecurityType::Spread);
    assert_eq!(fill.execution.execution.shares, shares, "combo shares");
    assert_eq!(fill.execution.execution.price, -69.65, "combo price");
    assert!(fill.commission.is_none(), "a combo-level execution has no report: {:?}", fill.commission);
}

fn assert_no_combo_parts(matcher: &FillMatcher) {
    assert_nothing_waits(matcher);
    assert!(matcher.combo_parts.is_empty(), "combo parts held: {:?}", matcher.combo_parts);
}

/// Feeds `events` at `now`; returns the fills handed out, and how many `ComboPending`.
fn feed(matcher: &mut FillMatcher, events: Vec<Executions>, now: Instant) -> (Vec<Fill>, usize) {
    let mut fills = Vec::new();
    let mut pending = 0;
    for event in events {
        match event {
            Executions::ExecutionData(execution) => fills.extend(matcher.push_execution_at(execution, now)),
            Executions::CommissionAndFeesReport(report) => match matcher.push_report_at(report, now) {
                ReportMatch::Paired(fill) => fills.push(fill),
                ReportMatch::ComboPending => pending += 1,
                ReportMatch::Waiting => {}
                other => panic!("unexpected {other:?}"),
            },
        }
    }
    (fills, pending)
}

/// The calendar fill as TWS sent it live: combo-level execution, far leg, near leg, then the
/// leg reports.
fn calendar_events() -> Vec<Executions> {
    vec![
        Executions::ExecutionData(bag("0000e1a7.6b0717ad.01.01", 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.02.01", ESH7, 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.03.01", ESZ6, 1.0)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ad.02.01", 2.16)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ad.03.01", 2.17)),
    ]
}

#[test]
fn combo_fill_in_the_order_tws_sends_it() {
    let now = Instant::now();
    let mut matcher = combo_matcher(1, 1);

    let (fills, pending) = feed(&mut matcher, calendar_events(), now);

    assert_eq!(fills.len(), 1, "{fills:?}");
    assert_combo_fill(&fills[0], "0000e1a7.6b0717ad.01.01", 1.0);
    assert_eq!(
        leg_fills(&fills[0]),
        vec![
            ("0000e1a7.6b0717ad.02.01", ESH7, 1.0, Some(2.16)),
            ("0000e1a7.6b0717ad.03.01", ESZ6, 1.0, Some(2.17)),
        ]
    );
    assert_eq!(pending, 1, "the first leg report pairs and waits for the second");
    assert_no_combo_parts(&matcher);
}

fn permutations(items: Vec<usize>) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items];
    }
    let mut all = Vec::new();
    for index in 0..items.len() {
        let mut rest = items.clone();
        let first = rest.remove(index);
        for mut tail in permutations(rest) {
            tail.insert(0, first);
            all.push(tail);
        }
    }
    all
}

#[test]
fn combo_fill_in_every_arrival_order() {
    let now = Instant::now();
    for order in permutations((0..5).collect()) {
        let mut matcher = combo_matcher(1, 1);
        let mut events: Vec<Option<Executions>> = calendar_events().into_iter().map(Some).collect();
        let ordered: Vec<Executions> = order.iter().map(|&index| events[index].take().expect("each event once")).collect();

        let (fills, _pending) = feed(&mut matcher, ordered, now);

        assert_eq!(fills.len(), 1, "order {order:?}: {fills:?}");
        assert_combo_fill(&fills[0], "0000e1a7.6b0717ad.01.01", 1.0);
        let mut legs = leg_fills(&fills[0]);
        legs.sort_by_key(|leg| leg.1);
        assert_eq!(
            legs,
            vec![
                ("0000e1a7.6b0717ad.03.01", ESZ6, 1.0, Some(2.17)),
                ("0000e1a7.6b0717ad.02.01", ESH7, 1.0, Some(2.16)),
            ],
            "order {order:?}"
        );
        assert_no_combo_parts(&matcher);
    }
}

#[test]
fn ratio_leg_completes_at_quantity_times_ratio() {
    let now = Instant::now();
    let mut short = combo_matcher(1, 2);
    let mut full = combo_matcher(1, 2);
    let events = |far_shares: f64| {
        vec![
            Executions::ExecutionData(bag("0000e1a7.6b071816.01.01", 1.0)),
            Executions::ExecutionData(leg("0000e1a7.6b071816.02.01", ESH7, far_shares)),
            Executions::ExecutionData(leg("0000e1a7.6b071816.03.01", ESZ6, 1.0)),
            Executions::CommissionAndFeesReport(report("0000e1a7.6b071816.02.01", 4.32)),
            Executions::CommissionAndFeesReport(report("0000e1a7.6b071816.03.01", 2.16)),
        ]
    };

    let (short_fills, _) = feed(&mut short, events(1.0), now);
    let (full_fills, _) = feed(&mut full, events(2.0), now);

    assert!(short_fills.is_empty(), "1 of 2 far-leg shares is not complete: {short_fills:?}");
    assert_eq!(short.combo_parts.len(), 1);
    assert_eq!(full_fills.len(), 1, "{full_fills:?}");
    assert_eq!(
        leg_fills(&full_fills[0]),
        vec![
            ("0000e1a7.6b071816.02.01", ESH7, 2.0, Some(4.32)),
            ("0000e1a7.6b071816.03.01", ESZ6, 1.0, Some(2.16)),
        ]
    );
}

#[test]
fn each_partial_fill_is_its_own_combo_fill() {
    let now = Instant::now();
    let mut matcher = combo_matcher(1, 1);
    let mut events = Vec::new();
    for key in ["0000e1a7.6b0719db", "0000e1a7.6b0719de"] {
        events.push(Executions::ExecutionData(bag(&format!("{key}.01.01"), 2.0)));
        events.push(Executions::ExecutionData(leg(&format!("{key}.02.01"), ESH7, 2.0)));
        events.push(Executions::ExecutionData(leg(&format!("{key}.03.01"), ESZ6, 2.0)));
        events.push(Executions::CommissionAndFeesReport(report(&format!("{key}.02.01"), 4.32)));
        events.push(Executions::CommissionAndFeesReport(report(&format!("{key}.03.01"), 4.33)));
    }

    let (fills, pending) = feed(&mut matcher, events, now);

    let summary: Vec<(&str, Vec<&str>)> = fills
        .iter()
        .map(|fill| {
            (
                fill.execution.execution.execution_id.as_str(),
                fill.legs.iter().map(|leg| leg.execution.execution.execution_id.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("0000e1a7.6b0719db.01.01", vec!["0000e1a7.6b0719db.02.01", "0000e1a7.6b0719db.03.01"]),
            ("0000e1a7.6b0719de.01.01", vec!["0000e1a7.6b0719de.02.01", "0000e1a7.6b0719de.03.01"]),
        ]
    );
    assert_eq!(pending, 2);
    assert_no_combo_parts(&matcher);
}

#[test]
fn take_expired_hands_out_an_incomplete_combo_with_the_legs_that_came() {
    let start = Instant::now();
    let mut matcher = combo_matcher(1, 1);
    assert!(matcher.push_execution_at(bag("0000e1a7.6b0717ad.01.01", 1.0), start).is_none());
    assert!(matcher.push_execution_at(leg("0000e1a7.6b0717ad.02.01", ESH7, 1.0), start).is_none());
    assert!(matches!(
        matcher.push_report_at(report("0000e1a7.6b0717ad.02.01", 2.16), start),
        ReportMatch::ComboPending
    ));
    let later = start + Duration::from_secs(1);
    assert!(matcher.push_execution_at(leg("0000e1a7.6b0717ad.03.01", ESZ6, 1.0), later).is_none());

    let early = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT - Duration::from_millis(1));
    let due = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);
    let late = matcher.push_report_at(report("0000e1a7.6b0717ad.03.01", 2.17), start + MAX_WAIT);

    assert!(early.fills.is_empty() && early.executions.is_empty(), "{early:?}");
    assert_eq!(due.fills.len(), 1, "{:?}", due.fills);
    assert_combo_fill(&due.fills[0], "0000e1a7.6b0717ad.01.01", 1.0);
    assert_eq!(
        leg_fills(&due.fills[0]),
        vec![("0000e1a7.6b0717ad.02.01", ESH7, 1.0, Some(2.16))],
        "the near leg has waited only 4 s, so it is not handed out"
    );
    assert!(due.executions.is_empty(), "{:?}", due.executions);
    assert!(
        matches!(late, ReportMatch::ComboPending),
        "the near leg now waits as a part of its own: {late:?}"
    );
    assert_eq!(matcher.combo_parts.len(), 1);
    assert!(matcher.combo_parts[0].execution.is_none());
}

#[test]
fn leg_without_report_completes_its_combo_when_taken() {
    let start = Instant::now();
    let mut matcher = combo_matcher(1, 1);
    let events = vec![
        Executions::ExecutionData(bag("0000e1a7.6b0717ad.01.01", 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.02.01", ESH7, 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.03.01", ESZ6, 1.0)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ad.02.01", 2.16)),
    ];
    let (fills, _) = feed(&mut matcher, events, start);

    let due = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);
    let late = matcher.push_report_at(report("0000e1a7.6b0717ad.03.01", 2.17), start + MAX_WAIT);

    assert!(fills.is_empty(), "{fills:?}");
    assert_eq!(due.fills.len(), 1, "{:?}", due.fills);
    assert_combo_fill(&due.fills[0], "0000e1a7.6b0717ad.01.01", 1.0);
    assert_eq!(
        leg_fills(&due.fills[0]),
        vec![
            ("0000e1a7.6b0717ad.02.01", ESH7, 1.0, Some(2.16)),
            ("0000e1a7.6b0717ad.03.01", ESZ6, 1.0, None),
        ]
    );
    assert!(due.executions.is_empty(), "a combo leg goes into its combo fill: {:?}", due.executions);
    assert_late(late, "0000e1a7.6b0717ad.03.01", 2.17);
    assert_no_combo_parts(&matcher);
}

#[test]
fn legs_without_their_combo_execution_come_out_one_by_one() {
    let start = Instant::now();
    let mut matcher = combo_matcher(1, 1);
    let events = vec![
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.02.01", ESH7, 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.03.01", ESZ6, 1.0)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ad.02.01", 2.16)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ad.03.01", 2.17)),
    ];
    let (fills, pending) = feed(&mut matcher, events, start);

    let due = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);

    assert!(fills.is_empty(), "{fills:?}");
    assert_eq!(pending, 2);
    let legs: Vec<(&str, Option<f64>, usize)> = due
        .fills
        .iter()
        .map(|fill| {
            (
                fill.execution.execution.execution_id.as_str(),
                fill.commission.as_ref().map(|report| report.commission_and_fees),
                fill.legs.len(),
            )
        })
        .collect();
    assert_eq!(
        legs,
        vec![("0000e1a7.6b0717ad.02.01", Some(2.16), 0), ("0000e1a7.6b0717ad.03.01", Some(2.17), 0)]
    );
    assert_no_combo_parts(&matcher);
}

#[test]
fn combo_not_announced_comes_as_separate_fills() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    let (fills, pending) = feed(&mut matcher, calendar_events(), now);

    assert_eq!(
        fill_ids(&fills),
        vec![
            ("0000e1a7.6b0717ad.01.01", None),
            ("0000e1a7.6b0717ad.02.01", Some(2.16)),
            ("0000e1a7.6b0717ad.03.01", Some(2.17)),
        ],
        "the default rule expects no report for the combo-level execution"
    );
    assert!(fills.iter().all(|fill| fill.legs.is_empty()));
    assert_eq!(pending, 0);
    assert_no_combo_parts(&matcher);
}

#[test]
fn second_combo_execution_for_the_same_fill_passes_through() {
    let now = Instant::now();
    let mut matcher = combo_matcher(1, 1);
    assert!(matcher.push_execution_at(bag("0000e1a7.6b0717ad.01.01", 1.0), now).is_none());

    let again = matcher
        .push_execution_at(bag("0000e1a7.6b0717ad.01.01", 1.0), now)
        .expect("not held twice");

    assert_combo_fill(&again, "0000e1a7.6b0717ad.01.01", 1.0);
    assert!(again.legs.is_empty());
    assert_eq!(matcher.combo_parts.len(), 1);
}

#[test]
fn memory_bounds_the_combos_remembered() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new().memory(1);
    matcher.add_combo(PERM_ID - 1, &combo_legs(1, 1));
    matcher.add_combo(PERM_ID, &combo_legs(1, 1));
    matcher.add_combo(PERM_ID, &combo_legs(1, 2));
    let mut forgotten = bag("0000e1a7.6b071700.01.01", 1.0);
    forgotten.execution.perm_id = PERM_ID - 1;
    let mut none = FillMatcher::new().memory(0);
    none.add_combo(PERM_ID, &combo_legs(1, 1));

    let passed = matcher.push_execution_at(forgotten, now);
    let held = matcher.push_execution_at(bag("0000e1a7.6b071816.01.01", 1.0), now);
    let unannounced = none.push_execution_at(bag("0000e1a7.6b0717ad.01.01", 1.0), now);

    assert!(passed.is_some_and(|fill| fill.legs.is_empty()), "the oldest combo was dropped");
    assert!(held.is_none(), "the newest combo is remembered");
    assert_eq!(matcher.combos.get(&PERM_ID), Some(&vec![(ESZ6, 1.0), (ESH7, 2.0)]), "legs replaced");
    assert_eq!(matcher.combo_order.len(), 1);
    assert!(unannounced.is_some(), "memory(0) remembers no combo");
    assert!(none.combos.is_empty());
}

#[test]
fn pair_executions_assembles_combo_fills_of_a_reply() {
    let mut other = leg("0000e1a7.6b06c16e.01.01", ESH7, 1.0);
    other.execution.perm_id = 1094417381;
    let rows = vec![
        Executions::ExecutionData(other),
        Executions::ExecutionData(bag("0000e1a7.6b0717ad.01.01", 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.02.01", ESH7, 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ad.03.01", ESZ6, 1.0)),
        Executions::ExecutionData(bag("0000e1a7.6b0717ee.01.01", 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ee.02.01", ESH7, 1.0)),
        Executions::ExecutionData(leg("0000e1a7.6b0717ee.03.01", ESZ6, 1.0)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b06c16e.01.01", 2.24)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ad.02.01", 2.16)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ad.03.01", 2.17)),
        Executions::CommissionAndFeesReport(report("0000e1a7.6b0717ee.02.01", 2.18)),
    ];
    let mut matcher = combo_matcher(1, 1);

    let (fills, unpaired) = matcher.pair_executions(rows);

    assert_eq!(
        fill_ids(&fills),
        vec![("0000e1a7.6b06c16e.01.01", Some(2.24)), ("0000e1a7.6b0717ad.01.01", None)],
        "execution order; the combo fill stands at its combo-level execution"
    );
    assert_eq!(
        leg_fills(&fills[1]),
        vec![
            ("0000e1a7.6b0717ad.02.01", ESH7, 1.0, Some(2.16)),
            ("0000e1a7.6b0717ad.03.01", ESZ6, 1.0, Some(2.17)),
        ]
    );
    assert_eq!(unpaired.fills.len(), 1, "{:?}", unpaired.fills);
    assert_combo_fill(&unpaired.fills[0], "0000e1a7.6b0717ee.01.01", 1.0);
    assert_eq!(
        leg_fills(&unpaired.fills[0]),
        vec![
            ("0000e1a7.6b0717ee.02.01", ESH7, 1.0, Some(2.18)),
            ("0000e1a7.6b0717ee.03.01", ESZ6, 1.0, None),
        ],
        "the near leg's report is missing from the reply"
    );
    assert!(unpaired.executions.is_empty() && unpaired.reports.is_empty(), "{unpaired:?}");
    assert_no_combo_parts(&matcher);
}

#[test]
fn pair_executions_leaves_combo_parts_from_before_alone() {
    let now = Instant::now();
    let mut matcher = combo_matcher(1, 1);
    assert!(matcher.push_execution_at(bag("0000e1a7.6b0717ad.01.01", 1.0), now).is_none());

    let (fills, unpaired) = matcher.pair_executions(vec![Executions::ExecutionData(execution("e1"))]);

    assert!(fills.is_empty());
    assert_eq!(ids(&unpaired.executions), vec!["e1"]);
    assert!(unpaired.fills.is_empty(), "{:?}", unpaired.fills);
    assert_eq!(matcher.combo_parts.len(), 1);
}

#[test]
fn fill_key_drops_the_last_two_fields() {
    let cases = [
        ("0000e1a7.6b0719db.02.01", "0000e1a7.6b0719db"),
        ("0000e1a7.6b0719db.01.01", "0000e1a7.6b0719db"),
        ("a.b", "a.b"),
        ("e1", "e1"),
        ("", ""),
    ];
    for (execution_id, expected) in cases {
        assert_eq!(fill_key(execution_id), expected, "{execution_id}");
    }
}
