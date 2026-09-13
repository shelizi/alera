use super::*;

#[test]
fn terminal_targets_include_later_states() {
    assert!(terminal_target_reached("accepted", "agent-ready"));
    assert!(terminal_target_reached("completed", "dispatch-accepted"));
    assert!(terminal_target_reached("stalled", "dispatch-accepted"));
    assert!(!terminal_target_reached(
        "dispatch_submitted_unconfirmed",
        "dispatch-accepted"
    ));
}

#[test]
fn parses_task_targets() {
    let targets = parse_task_targets(&json!({ "targets": ["completed", "failed"] })).unwrap();
    assert_eq!(
        targets,
        vec![
            OrchestrationTaskStatus::Completed,
            OrchestrationTaskStatus::Failed
        ]
    );
}
