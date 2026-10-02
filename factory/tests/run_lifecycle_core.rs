use epilogos_factory::core::identity::Revision;
use epilogos_factory::core::run::{
    Run, RunContractError, RunLifecycle, RunLifecycleCommand, RunLifecycleOutcome, RunRegistry,
};

fn run() -> Run {
    Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap(),
        "Native lifecycle recovery",
        "factory-coordinator-original",
    )
    .unwrap()
}

fn transition(run: &mut Run, id: &str, to: RunLifecycle) -> RunLifecycleCommand {
    let command = RunLifecycleCommand {
        command_id: id.into(),
        expected_revision: run.revision(),
        lifecycle: to,
    };
    assert!(matches!(
        run.apply_lifecycle_command(&run.mutation_authority(), command.clone())
            .unwrap(),
        RunLifecycleOutcome::Applied(_)
    ));
    command
}

#[test]
fn lifecycle_retains_exact_transition_receipts_across_restart() {
    let mut original = run();
    let activated = transition(&mut original, "activate", RunLifecycle::Active);
    transition(&mut original, "wait-for-choice", RunLifecycle::WaitingHuman);
    transition(&mut original, "resume-choice", RunLifecycle::Active);
    transition(&mut original, "begin-return", RunLifecycle::Finishing);

    let mut registry = RunRegistry::default();
    registry.insert(original.clone()).unwrap();
    let mut restarted = RunRegistry::from_json(&registry.to_json().unwrap()).unwrap();
    let restored = restarted.get_mut(original.reference()).unwrap();
    let before = restored.clone();
    let replay = restored
        .apply_lifecycle_command(&restored.mutation_authority(), activated.clone())
        .unwrap();
    let RunLifecycleOutcome::AlreadyApplied(receipt) = replay else {
        panic!("restart repeated a lifecycle mutation");
    };
    assert_eq!(receipt.command, activated);
    assert_eq!(receipt.previous_lifecycle, RunLifecycle::Seeded);
    assert_eq!(receipt.next_revision, Revision::new(2).unwrap());
    assert_eq!(restored, &before);
    transition(restored, "finish-return", RunLifecycle::Finished);
    transition(restored, "archive-return", RunLifecycle::Archived);
    assert_eq!(restored.lifecycle(), RunLifecycle::Archived);
    assert_eq!(restored.map(), original.map());
    let mut schema: serde_json::Value =
        serde_json::from_str(include_str!("../../contracts/factory/run-map.schema.json")).unwrap();
    schema["$ref"] = "#/$defs/run".into();
    jsonschema::Validator::new(&schema)
        .unwrap()
        .validate(&serde_json::to_value(restored).unwrap())
        .unwrap();
}

#[test]
fn changed_retry_identity_and_stale_revision_leave_run_unchanged() {
    let mut current = run();
    let activated = transition(&mut current, "activate", RunLifecycle::Active);
    let before = current.clone();
    let mut changed = activated;
    changed.lifecycle = RunLifecycle::Aborted;
    assert!(matches!(
        current.apply_lifecycle_command(&current.mutation_authority(), changed),
        Err(RunContractError::LifecycleCommandConflict(_))
    ));
    let stale = RunLifecycleCommand {
        command_id: "stale-finish".into(),
        expected_revision: Revision::INITIAL,
        lifecycle: RunLifecycle::Finishing,
    };
    assert!(matches!(
        current.apply_lifecycle_command(&current.mutation_authority(), stale),
        Err(RunContractError::RevisionConflict { .. })
    ));
    assert_eq!(current, before);
}

#[test]
fn coordinator_handover_fences_old_writer_and_preserves_transition_history() {
    let mut current = run();
    let old_authority = current.mutation_authority();
    let activated = transition(&mut current, "activate", RunLifecycle::Active);
    let successor = current
        .transfer_write_authority(
            &old_authority,
            current.revision(),
            "factory-coordinator-replacement",
        )
        .unwrap();
    let before = current.clone();
    assert!(matches!(
        current.apply_lifecycle_command(&old_authority, activated.clone()),
        Err(RunContractError::InvalidMutationAuthority)
    ));
    assert_eq!(current, before);
    assert!(matches!(
        current
            .apply_lifecycle_command(&successor, activated)
            .unwrap(),
        RunLifecycleOutcome::AlreadyApplied(_)
    ));
    let command = RunLifecycleCommand {
        command_id: "replacement-suspend".into(),
        expected_revision: current.revision(),
        lifecycle: RunLifecycle::Suspended,
    };
    current
        .apply_lifecycle_command(&successor, command)
        .unwrap();
    assert_eq!(current.lifecycle(), RunLifecycle::Suspended);
    assert_eq!(current.write_authority().epoch(), 2);
}

#[test]
fn skipped_and_terminal_transitions_are_refused_atomically() {
    let mut current = run();
    let before = current.clone();
    let skip = RunLifecycleCommand {
        command_id: "consumer-declared-success".into(),
        expected_revision: current.revision(),
        lifecycle: RunLifecycle::Finished,
    };
    assert!(matches!(
        current.apply_lifecycle_command(&current.mutation_authority(), skip),
        Err(RunContractError::InvalidLifecycleTransition { .. })
    ));
    assert_eq!(current, before);
    transition(&mut current, "abort", RunLifecycle::Aborted);
    transition(&mut current, "archive", RunLifecycle::Archived);
    let before = current.clone();
    let reopen = RunLifecycleCommand {
        command_id: "restart-does-not-reopen".into(),
        expected_revision: current.revision(),
        lifecycle: RunLifecycle::Active,
    };
    assert!(matches!(
        current.apply_lifecycle_command(&current.mutation_authority(), reopen),
        Err(RunContractError::InvalidLifecycleTransition { .. })
    ));
    assert_eq!(current, before);
}
