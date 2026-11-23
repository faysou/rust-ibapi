use super::*;
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

fn assert_empty(matcher: &FillMatcher) {
    assert!(matcher.executions.is_empty(), "executions held: {:?}", matcher.executions);
    assert!(matcher.reports.is_empty(), "reports held: {:?}", matcher.reports);
    assert!(matcher.taken.is_empty(), "taken ids held: {:?}", matcher.taken);
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
    assert_empty(&matcher);
}

#[test]
fn report_then_execution_pairs() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    let pushed = matcher.push_report_at(report("e1", 1.25), now);
    let fill = matcher.push_execution_at(execution("e1"), now).expect("report was waiting");

    assert!(matches!(pushed, ReportMatch::Waiting), "report alone must wait, got {pushed:?}");
    assert_fill(&fill, "e1", Some(1.25));
    assert_empty(&matcher);
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
    assert_empty(&matcher);
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
fn report_after_take_is_late() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), start).is_none());
    let taken = matcher.take_expired_at(MAX_WAIT, start + MAX_WAIT);

    let late = matcher.push_report_at(report("e1", 1.25), start + MAX_WAIT + Duration::from_secs(1));
    let again = matcher.push_report_at(report("e1", 1.25), start + MAX_WAIT + Duration::from_secs(2));

    assert_eq!(ids(&taken.executions), vec!["e1"]);
    match late {
        ReportMatch::Late(report) => {
            assert_eq!(report.execution_id, "e1");
            assert_eq!(report.commission_and_fees, 1.25);
        }
        other => panic!("expected Late, got {other:?}"),
    }
    assert!(
        matches!(again, ReportMatch::Waiting),
        "a taken id answers Late once, then is forgotten: {again:?}"
    );
}

#[test]
fn taken_id_is_forgotten_after_another_max_wait() {
    let start = Instant::now();
    let taken_at = start + MAX_WAIT;
    let mut matcher = FillMatcher::new();
    assert!(matcher.push_execution_at(execution("e1"), start).is_none());
    matcher.take_expired_at(MAX_WAIT, taken_at);

    let still_held = matcher.take_expired_at(MAX_WAIT, taken_at + MAX_WAIT - Duration::from_millis(1));
    let held_ids = matcher.taken.len();
    matcher.take_expired_at(MAX_WAIT, taken_at + MAX_WAIT);
    let after = matcher.push_report_at(report("e1", 1.25), taken_at + MAX_WAIT);

    assert!(still_held.executions.is_empty() && still_held.reports.is_empty());
    assert_eq!(held_ids, 1, "the taken id is kept until it is max_wait old");
    assert!(
        matches!(after, ReportMatch::Waiting),
        "a forgotten id pairs like any early report: {after:?}"
    );
}

#[test]
fn take_expired_bounds_every_store() {
    let start = Instant::now();
    let mut matcher = FillMatcher::new();
    for i in 0..100 {
        let at = start + Duration::from_millis(i);
        assert!(matcher.push_execution_at(execution(&format!("e{i}")), at).is_none());
        assert!(matches!(matcher.push_report_at(report(&format!("r{i}"), 0.5), at), ReportMatch::Waiting));
    }
    assert!(matcher.push_execution_at(exercise("x1"), start).is_some());

    let first = matcher.take_expired_at(MAX_WAIT, start + Duration::from_secs(10));
    let held_after_first = (matcher.executions.len(), matcher.reports.len(), matcher.taken.len());
    matcher.take_expired_at(MAX_WAIT, start + Duration::from_secs(20));

    assert_eq!(first.executions.len(), 100);
    assert_eq!(first.reports.len(), 100);
    assert_eq!(ids(&first.executions)[..3], ["e0", "e1", "e2"], "arrival order");
    assert_eq!(held_after_first, (0, 0, 100), "the exercise id is forgotten; the 100 taken ids are kept");
    assert_empty(&matcher);
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
    assert!(matcher.executions.is_empty() && matcher.reports.is_empty());
}

#[test]
fn exercise_is_a_fill_without_commission_at_once() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();

    let fill = matcher.push_execution_at(exercise("x1"), now).expect("an exercise gets no report");
    let late = matcher.push_report_at(report("x1", 0.0), now);

    assert_fill(&fill, "x1", None);
    assert_eq!(fill.execution.execution.option_exercise_type, OptionExerciseType::Exercise);
    assert!(matches!(late, ReportMatch::Late(_)), "a report for an exercise is late: {late:?}");
}

#[test]
fn exercise_pairs_with_a_report_that_came_first() {
    let now = Instant::now();
    let mut matcher = FillMatcher::new();
    assert!(matches!(matcher.push_report_at(report("x1", 0.75), now), ReportMatch::Waiting));

    let fill = matcher.push_execution_at(exercise("x1"), now).expect("report was waiting");

    assert_fill(&fill, "x1", Some(0.75));
    assert_empty(&matcher);
}

#[test]
fn expects_commission_report_is_false_only_for_exercise() {
    let cases = [
        (OptionExerciseType::None, true),
        (OptionExerciseType::Exercise, false),
        (OptionExerciseType::Lapse, true),
        (OptionExerciseType::DoNothing, true),
        (OptionExerciseType::Assigned, true),
        (OptionExerciseType::AutoexerciseClearing, true),
        (OptionExerciseType::Expired, true),
        (OptionExerciseType::AutoexerciseTrading, true),
        (OptionExerciseType::Netting, true),
        (OptionExerciseType::Unknown(7), true),
    ];
    for (option_exercise_type, expected) in cases {
        let execution = Execution {
            option_exercise_type,
            ..Default::default()
        };
        assert_eq!(execution.expects_commission_report(), expected, "{option_exercise_type:?}");
    }
}

#[test]
fn pair_executions_splits_fills_and_leftovers() {
    let rows = vec![
        Executions::CommissionAndFeesReport(report("e2", 2.5)),
        Executions::ExecutionData(execution("e1")),
        Executions::ExecutionData(execution("e2")),
        Executions::ExecutionData(execution("e3")),
        Executions::CommissionAndFeesReport(report("e1", 1.25)),
        Executions::ExecutionData(exercise("x1")),
        Executions::CommissionAndFeesReport(report("r9", 0.5)),
        Executions::CommissionAndFeesReport(report("x1", 0.0)),
    ];

    let (fills, unpaired) = pair_executions(rows);

    assert_eq!(fills.len(), 3, "fills: {fills:?}");
    assert_fill(&fills[0], "e2", Some(2.5));
    assert_fill(&fills[1], "e1", Some(1.25));
    assert_fill(&fills[2], "x1", None);
    assert_eq!(ids(&unpaired.executions), vec!["e3"]);
    assert_eq!(report_ids(&unpaired.reports), vec!["r9", "x1"], "waiting reports, then late ones");
}

#[test]
fn pair_executions_of_empty_reply() {
    let (fills, unpaired) = pair_executions(Vec::new());

    assert!(fills.is_empty());
    assert!(unpaired.executions.is_empty());
    assert!(unpaired.reports.is_empty());
}
