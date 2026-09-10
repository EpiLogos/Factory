use epilogos_factory::core::run::{Run, RunRef, WorkflowUnitRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{
    ExecutableOrchestration, ExecutionLaunch, LegStatus, OrchestrationSnapshot, RetryGrant,
    ReturnedArtifact,
};
use epilogos_factory::workflow::{compile_workflow, workflow_source_digest, WorkflowSource};
use serde_json::json;
use std::collections::BTreeSet;

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");

fn engine() -> ExecutableOrchestration {
    let workflow = compile_workflow(serde_json::from_str(SOURCE).unwrap()).unwrap();
    let run = Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse::<RunRef>().unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap(),
        "durable native coordinator tests",
        "factory-test",
    )
    .unwrap();
    ExecutableOrchestration::new(workflow, run).unwrap()
}

fn unit(engine: &ExecutableOrchestration) -> WorkflowUnitRef {
    engine
        .workflow()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone()
}

fn launch(engine: &ExecutableOrchestration, reference: &str) -> ExecutionLaunch {
    let demand = ExecutionDemand {
        project_ref: engine.run().project_ref().to_string(),
        run_ref: engine.run().reference().to_string(),
        workflow_unit_ref: Some(unit(engine).to_string()),
        agency_ref: Some("agency:controlled-test".into()),
        profile_ref: None,
        use_type: "controlled-native-test".into(),
        required_capabilities: BTreeSet::new(),
        required_modalities: BTreeSet::new(),
        required_actions: BTreeSet::new(),
        required_tools: BTreeSet::new(),
        context_characteristics: BTreeSet::new(),
        independence_from: BTreeSet::new(),
        cost_ceiling_usd: None,
        latency_preference_ms: None,
        requires_local_materialisation: false,
    };
    let selection = AikitModelRosterSelection {
        roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
        model_ref: "model:controlled-test".into(),
        provider_ref: "provider:controlled-test".into(),
        ranking_policy: "controlled-test".into(),
        ranking_explanation: json!({"testOnly": true}),
        provenance: vec!["selection:controlled-test".into()],
    };
    ExecutionLaunch {
        execution_ref: reference.into(),
        disposition: accept_aikit_selection(demand, selection, "2026-09-10T17:00:00+01:00")
            .unwrap(),
        retry_grant: Some(RetryGrant::new("grant:bounded-test", 2).unwrap()),
    }
}

fn result(engine: &ExecutableOrchestration, execution: &str) -> ReturnedArtifact {
    let unit = engine.workflow().unit("inspect-source").unwrap();
    ReturnedArtifact {
        artifact_ref: format!("artifact:{execution}"),
        subject_ref: unit.subject_ref.to_string(),
        subject_revision: unit.basis_revision.clone(),
        producing_execution_ref: execution.into(),
        evidence_refs: BTreeSet::from(["evidence:controlled-test".into()]),
        semantic_difference: "controlled fixture result, not provider proof".into(),
    }
}

fn start(engine: &mut ExecutableOrchestration, execution: &str) -> WorkflowUnitRef {
    let unit = unit(engine);
    let launch = launch(engine, execution);
    engine.start_serial("journey:test", &unit, launch).unwrap();
    unit
}

fn reopen(engine: &ExecutableOrchestration) -> ExecutableOrchestration {
    let encoded = serde_json::to_vec(&engine.snapshot()).unwrap();
    let snapshot: OrchestrationSnapshot = serde_json::from_slice(&encoded).unwrap();
    snapshot
        .restore(engine.workflow().clone(), engine.run().clone())
        .unwrap()
}

#[test]
fn snapshot_roundtrip_uses_canonical_run_and_rejects_changed_source() {
    let mut engine = engine();
    start(&mut engine, "execution:first");
    let snapshot = engine.snapshot();
    assert_eq!(reopen(&engine).snapshot(), snapshot);
    let mut changed: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
    changed.source.revision = "different-authoring".into();
    changed.source.digest = workflow_source_digest(&changed).unwrap();
    assert!(snapshot
        .restore(compile_workflow(changed).unwrap(), engine.run().clone())
        .is_err());
    let mut corrupt = serde_json::to_value(&snapshot).unwrap();
    corrupt["retryGrants"]["grant:bounded-test"]["attemptsSpent"] = json!(0);
    let corrupt: OrchestrationSnapshot = serde_json::from_value(corrupt).unwrap();
    assert!(corrupt
        .restore(engine.workflow().clone(), engine.run().clone())
        .is_err());
}

#[test]
fn retry_uses_fresh_basis_and_old_return_cannot_replace_the_new_attempt() {
    let mut engine = engine();
    let unit = start(&mut engine, "execution:first");
    let late = result(&engine, "execution:first");
    engine.fail(&unit, "controlled failure").unwrap();
    let subject = engine
        .workflow()
        .unit("inspect-source")
        .unwrap()
        .subject_ref
        .to_string();
    engine.advance_subject(subject, "current-subject-revision");
    let mut next = launch(&engine, "execution:second");
    next.retry_grant = engine.retry_grant("grant:bounded-test").cloned();
    engine
        .retry("journey:test", &unit, "grant:bounded-test", next)
        .unwrap();
    assert_eq!(
        engine.leg(&unit).unwrap().delegation.basis_revision,
        "current-subject-revision"
    );
    engine.return_artifact(&unit, late.clone()).unwrap();
    engine.return_artifact(&unit, late).unwrap();
    let leg = engine.leg(&unit).unwrap();
    assert_eq!(leg.execution_ref, "execution:second");
    assert_eq!(leg.status, LegStatus::Active);
    assert!(leg.artifacts.is_empty());
    assert_eq!(leg.attempts[0].late_artifacts.len(), 1);
    assert_eq!(
        engine
            .retry_grant("grant:bounded-test")
            .unwrap()
            .attempts_spent,
        2
    );
    assert_eq!(reopen(&engine).snapshot(), engine.snapshot());
}

#[test]
fn serial_restart_cannot_bypass_the_retry_budget() {
    let mut engine = engine();
    let unit = start(&mut engine, "execution:first");
    engine.fail(&unit, "controlled failure").unwrap();
    let before = engine.snapshot();
    let second = launch(&engine, "execution:second");
    assert!(engine.start_serial("journey:test", &unit, second).is_err());
    assert_eq!(before, engine.snapshot());
}

#[test]
fn accepted_cancellation_and_late_artifact_are_not_quiescence() {
    let mut engine = engine();
    let unit = start(&mut engine, "execution:first");
    engine.request_cancellation(&unit).unwrap();
    engine.accept_cancellation(&unit).unwrap();
    let artifact = result(&engine, "execution:first");
    engine.return_artifact(&unit, artifact).unwrap();
    assert!(engine.incorporate_late_result(&unit).is_err());
    let mut next = launch(&engine, "execution:second");
    next.retry_grant = engine.retry_grant("grant:bounded-test").cloned();
    assert!(engine
        .retry("journey:test", &unit, "grant:bounded-test", next)
        .is_err());
    engine.mark_quiescent(&unit).unwrap();
    engine.incorporate_late_result(&unit).unwrap();
    assert_eq!(engine.leg(&unit).unwrap().status, LegStatus::Returned);
    assert_eq!(reopen(&engine).snapshot(), engine.snapshot());
}

#[test]
fn factory_reservation_binds_once_to_a_real_owner_execution_identity() {
    let mut engine = engine();
    let unit = start(&mut engine, "factory-attempt:reserved");
    engine
        .bind_execution_identity(
            &unit,
            "factory-attempt:reserved",
            "execution:owner-returned",
        )
        .unwrap();
    assert!(engine
        .bind_execution_identity(&unit, "factory-attempt:reserved", "execution:other")
        .is_err());
    assert_eq!(
        engine.leg(&unit).unwrap().attempts[0].execution_ref,
        "execution:owner-returned"
    );
    assert_eq!(reopen(&engine).snapshot(), engine.snapshot());
}
