use epilogos_factory::core::run::{ProjectRef, Run, RunRef, WorkflowUnitRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{
    ArchitectureFinding, BarrierState, ExecutableOrchestration, ExecutionLaunch, LegStatus,
    OrchestrationError, OwnerArchitectureObservationReceipt, RetryGrant, ReturnedArtifact,
    WholeRunState,
};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");

fn source() -> WorkflowSource {
    serde_json::from_str(FIXTURE).unwrap()
}

fn compiled() -> CompiledWorkflow {
    compile_workflow(source()).unwrap()
}

fn run() -> Run {
    Run::new(
        "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse::<RunRef>().unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW"
            .parse::<ProjectRef>()
            .unwrap(),
        "Factory orchestration acceptance",
        "factory-orchestrator",
    )
    .unwrap()
}

fn engine() -> ExecutableOrchestration {
    ExecutableOrchestration::new(compiled(), run()).unwrap()
}

fn unit(workflow: &CompiledWorkflow, key: &str) -> WorkflowUnitRef {
    workflow.unit(key).unwrap().reference.clone()
}

fn disposition(
    run: &Run,
    unit: Option<&WorkflowUnitRef>,
) -> epilogos_factory::execution_intelligence::ExecutionDisposition {
    accept_aikit_selection(
        ExecutionDemand {
            project_ref: run.project_ref().to_string(),
            run_ref: run.reference().to_string(),
            workflow_unit_ref: unit.map(ToString::to_string),
            agency_ref: Some("agency:factory-test".into()),
            profile_ref: Some("profile:orchestration".into()),
            use_type: "factory-orchestration-test".into(),
            required_capabilities: BTreeSet::new(),
            required_modalities: BTreeSet::new(),
            required_actions: BTreeSet::new(),
            required_tools: BTreeSet::new(),
            context_characteristics: BTreeSet::new(),
            independence_from: BTreeSet::new(),
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            requires_local_materialisation: false,
        },
        AikitModelRosterSelection {
            roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
            model_ref: "model:factory-test".into(),
            provider_ref: "provider:factory-test".into(),
            ranking_policy: "test-contract".into(),
            ranking_explanation: json!({"eligible": true}),
            provenance: vec!["selection:factory-test".into()],
        },
        "2026-09-08T20:00:00+01:00",
    )
    .unwrap()
}

fn launch(
    orchestration: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    execution_ref: &str,
) -> ExecutionLaunch {
    ExecutionLaunch {
        execution_ref: execution_ref.into(),
        disposition: disposition(orchestration.run(), Some(unit)),
        retry_grant: None,
    }
}

fn artifact(
    orchestration: &ExecutableOrchestration,
    unit: &WorkflowUnitRef,
    artifact_ref: &str,
    execution_ref: &str,
    evidence_ref: &str,
) -> ReturnedArtifact {
    let compiled = orchestration
        .workflow()
        .units
        .values()
        .find(|candidate| &candidate.reference == unit)
        .unwrap();
    ReturnedArtifact {
        artifact_ref: artifact_ref.into(),
        subject_ref: compiled.subject_ref.to_string(),
        subject_revision: compiled.basis_revision.clone(),
        producing_execution_ref: execution_ref.into(),
        evidence_refs: BTreeSet::from([evidence_ref.into()]),
        semantic_difference: format!("resolved {}", compiled.developmental_concern),
    }
}

fn complete_inspect(orchestration: &mut ExecutableOrchestration) -> WorkflowUnitRef {
    let inspect = unit(orchestration.workflow(), "inspect-source");
    let inspect_launch = launch(orchestration, &inspect, "execution:inspect");
    orchestration
        .start_serial("journey:factory-198", &inspect, inspect_launch)
        .unwrap();
    orchestration
        .return_artifact(
            &inspect,
            artifact(
                orchestration,
                &inspect,
                "artifact:inspect",
                "execution:inspect",
                "evidence:inspect",
            ),
        )
        .unwrap();
    inspect
}

fn start_fork(orchestration: &mut ExecutableOrchestration) -> (WorkflowUnitRef, WorkflowUnitRef) {
    complete_inspect(orchestration);
    let implementation = unit(orchestration.workflow(), "implement-compiler");
    let review = unit(orchestration.workflow(), "review-adversarially");
    let launches = BTreeMap::from([
        (
            implementation.clone(),
            launch(orchestration, &implementation, "execution:implement"),
        ),
        (
            review.clone(),
            launch(orchestration, &review, "execution:review"),
        ),
    ]);
    orchestration
        .fork(
            "journey:factory-198",
            &[implementation.clone(), review.clone()],
            launches,
        )
        .unwrap();
    (implementation, review)
}

#[test]
fn true_independent_fork_starts_both_legs_in_parallel() {
    let mut orchestration = engine();
    let (implementation, review) = start_fork(&mut orchestration);
    assert_eq!(
        orchestration.leg(&implementation).unwrap().status,
        LegStatus::Active
    );
    assert_eq!(
        orchestration.leg(&review).unwrap().status,
        LegStatus::Active
    );
}

#[test]
fn shared_writer_is_refused_before_any_child_is_started() {
    let mut source = source();
    source.units[1].dependencies.clear();
    source.units[2].dependencies.clear();
    source.units[1].independence_from = vec!["review-adversarially".into()];
    source.units[2].independence_from = vec!["implement-compiler".into()];
    source.units[1].permitted_effects = vec!["write:shared-subject".into()];
    source.units[2].permitted_effects = vec!["write:shared-subject".into()];
    source.units[2].subject_ref = source.units[1].subject_ref.clone();
    source.source.digest = workflow_source_digest(&source).unwrap();
    let workflow = compile_workflow(source).unwrap();
    let mut orchestration = ExecutableOrchestration::new(workflow, run()).unwrap();
    let implementation = unit(orchestration.workflow(), "implement-compiler");
    let review = unit(orchestration.workflow(), "review-adversarially");
    let launches = BTreeMap::from([
        (
            implementation.clone(),
            launch(&orchestration, &implementation, "execution:writer-a"),
        ),
        (
            review.clone(),
            launch(&orchestration, &review, "execution:writer-b"),
        ),
    ]);
    assert!(matches!(
        orchestration.fork("journey:factory-198", &[implementation, review], launches),
        Err(OrchestrationError::SharedWriterConflict { .. })
    ));
    assert!(orchestration.legs().is_empty());
}

#[test]
fn fork_preflight_rejects_shared_exhausted_grant_atomically() {
    let mut orchestration = engine();
    complete_inspect(&mut orchestration);
    let implementation = unit(orchestration.workflow(), "implement-compiler");
    let review = unit(orchestration.workflow(), "review-adversarially");
    let grant = RetryGrant::new("grant:single-fork", 1).unwrap();
    let launches = BTreeMap::from([
        (
            implementation.clone(),
            ExecutionLaunch {
                execution_ref: "execution:fork-a".into(),
                disposition: disposition(orchestration.run(), Some(&implementation)),
                retry_grant: Some(grant.clone()),
            },
        ),
        (
            review.clone(),
            ExecutionLaunch {
                execution_ref: "execution:fork-b".into(),
                disposition: disposition(orchestration.run(), Some(&review)),
                retry_grant: Some(grant),
            },
        ),
    ]);
    let run_revision = orchestration.run().revision();
    assert!(matches!(
        orchestration.fork(
            "journey:factory-198",
            &[implementation.clone(), review.clone()],
            launches
        ),
        Err(OrchestrationError::RetryExhausted(_))
    ));
    assert!(orchestration.leg(&implementation).is_none());
    assert!(orchestration.leg(&review).is_none());
    assert_eq!(orchestration.run().revision(), run_revision);
}

#[test]
fn every_child_receives_the_exact_delegated_contract() {
    let mut orchestration = engine();
    let (implementation, _) = start_fork(&mut orchestration);
    let delegation = orchestration.delegation(&implementation).unwrap();
    let compiled_unit = orchestration.workflow().unit("implement-compiler").unwrap();
    assert_eq!(delegation.parent_journey_ref, "journey:factory-198");
    assert_eq!(
        delegation.parent_run_ref,
        orchestration.run().reference().to_string()
    );
    assert_eq!(delegation.execution_unit_ref, implementation);
    assert_eq!(
        delegation.subject_ref,
        compiled_unit.subject_ref.to_string()
    );
    assert_eq!(delegation.basis_revision, compiled_unit.basis_revision);
    assert_eq!(
        delegation.permitted_effects,
        compiled_unit.permitted_effects
    );
    assert_eq!(
        delegation.verification_obligations,
        compiled_unit.verification_obligations
    );
    assert_eq!(delegation.return_address, compiled_unit.return_address);
    assert_eq!(delegation.stop_conditions, compiled_unit.stop_conditions);
}

#[test]
fn barrier_waits_for_every_required_leg() {
    let mut orchestration = engine();
    let (implementation, review) = start_fork(&mut orchestration);
    orchestration
        .return_artifact(
            &implementation,
            artifact(
                &orchestration,
                &implementation,
                "artifact:implementation",
                "execution:implement",
                "evidence:implementation",
            ),
        )
        .unwrap();
    let pending = orchestration
        .barrier_reading("implementation-reviewed")
        .unwrap();
    assert_eq!(pending.state, BarrierState::Pending);
    assert_eq!(pending.missing_units, BTreeSet::from([review.clone()]));
    orchestration
        .return_artifact(
            &review,
            artifact(
                &orchestration,
                &review,
                "artifact:review",
                "execution:review",
                "evidence:review",
            ),
        )
        .unwrap();
    assert_eq!(
        orchestration
            .barrier_reading("implementation-reviewed")
            .unwrap()
            .state,
        BarrierState::Complete
    );
}

#[test]
fn failed_leg_prevents_false_whole_completion() {
    let mut orchestration = engine();
    let (implementation, review) = start_fork(&mut orchestration);
    orchestration
        .fail(&review, "adversarial review found a contract break")
        .unwrap();
    assert_eq!(
        orchestration
            .barrier_reading("implementation-reviewed")
            .unwrap()
            .state,
        BarrierState::Failed
    );
    assert_eq!(orchestration.whole_run_state(), WholeRunState::Failed);
    assert_eq!(
        orchestration.leg(&implementation).unwrap().status,
        LegStatus::Active
    );
}

#[test]
fn cancellation_and_quiescence_are_distinct_and_block_the_barrier() {
    let mut orchestration = engine();
    let (_, review) = start_fork(&mut orchestration);
    orchestration.detach(&review).unwrap();
    assert_eq!(
        orchestration.leg(&review).unwrap().status,
        LegStatus::Detached
    );
    orchestration.request_cancellation(&review).unwrap();
    assert_eq!(
        orchestration.leg(&review).unwrap().status,
        LegStatus::CancelRequested
    );
    orchestration.accept_cancellation(&review).unwrap();
    assert_eq!(
        orchestration.leg(&review).unwrap().status,
        LegStatus::CancellationAccepted
    );
    orchestration.record_process_termination(&review).unwrap();
    assert_eq!(
        orchestration.leg(&review).unwrap().status,
        LegStatus::ProcessTerminated
    );
    orchestration.mark_quiescent(&review).unwrap();
    assert_eq!(
        orchestration.leg(&review).unwrap().status,
        LegStatus::Quiescent
    );
    assert_eq!(
        orchestration.leg(&review).unwrap().status_history,
        vec![
            LegStatus::Active,
            LegStatus::Detached,
            LegStatus::CancelRequested,
            LegStatus::CancellationAccepted,
            LegStatus::ProcessTerminated,
            LegStatus::Quiescent,
        ]
    );
    assert_eq!(
        orchestration
            .barrier_reading("implementation-reviewed")
            .unwrap()
            .state,
        BarrierState::Failed
    );
}

#[test]
fn accepted_cancellation_can_quiesce_without_a_termination_receipt() {
    let mut orchestration = engine();
    let (_, review) = start_fork(&mut orchestration);
    orchestration.request_cancellation(&review).unwrap();
    orchestration.accept_cancellation(&review).unwrap();
    orchestration.mark_quiescent(&review).unwrap();
    let leg = orchestration.leg(&review).unwrap();
    assert_eq!(leg.status, LegStatus::Quiescent);
    assert!(!leg.status_history.contains(&LegStatus::ProcessTerminated));
    assert_eq!(
        leg.status_history,
        vec![
            LegStatus::Active,
            LegStatus::CancelRequested,
            LegStatus::CancellationAccepted,
            LegStatus::Quiescent,
        ]
    );
}

#[test]
fn late_result_is_evidence_but_is_not_applied_to_moved_state() {
    let mut orchestration = engine();
    let (_, review) = start_fork(&mut orchestration);
    let subject = orchestration
        .workflow()
        .unit("review-adversarially")
        .unwrap()
        .subject_ref
        .to_string();
    let old_revision = orchestration
        .workflow()
        .unit("review-adversarially")
        .unwrap()
        .basis_revision
        .clone();
    orchestration.detach(&review).unwrap();
    orchestration.request_cancellation(&review).unwrap();
    orchestration.accept_cancellation(&review).unwrap();
    orchestration.record_process_termination(&review).unwrap();
    orchestration.mark_quiescent(&review).unwrap();
    orchestration.advance_subject(subject, "moved-revision-2");
    orchestration
        .return_artifact(
            &review,
            ReturnedArtifact {
                artifact_ref: "artifact:late-review".into(),
                subject_ref: orchestration
                    .workflow()
                    .unit("review-adversarially")
                    .unwrap()
                    .subject_ref
                    .to_string(),
                subject_revision: old_revision,
                producing_execution_ref: "execution:review".into(),
                evidence_refs: BTreeSet::from(["evidence:late-review".into()]),
                semantic_difference: "late review evidence".into(),
            },
        )
        .unwrap();
    assert_eq!(
        orchestration.leg(&review).unwrap().status,
        LegStatus::LateResult
    );
    assert_eq!(orchestration.leg(&review).unwrap().late_artifacts.len(), 1);
    assert!(matches!(
        orchestration.incorporate_late_result(&review),
        Err(OrchestrationError::StaleLateResult { .. })
    ));
    assert!(orchestration.leg(&review).unwrap().artifacts.is_empty());
}

#[test]
fn retry_preserves_the_same_spent_grant_and_cannot_replenish_it() {
    let mut orchestration = engine();
    let inspect = unit(orchestration.workflow(), "inspect-source");
    let grant = RetryGrant::new("grant:inspect", 2).unwrap();
    orchestration.open_retry_grant(grant.clone()).unwrap();
    let mut first = launch(&orchestration, &inspect, "execution:inspect-first");
    first.retry_grant = Some(grant);
    orchestration
        .start_serial("journey:factory-198", &inspect, first)
        .unwrap();
    orchestration
        .fail(&inspect, "temporary process failure")
        .unwrap();
    assert_eq!(
        orchestration
            .retry_grant("grant:inspect")
            .unwrap()
            .attempts_spent,
        1
    );
    let mut retry = launch(&orchestration, &inspect, "execution:inspect-retry");
    retry.retry_grant = Some(orchestration.retry_grant("grant:inspect").unwrap().clone());
    orchestration
        .retry("journey:factory-198", &inspect, "grant:inspect", retry)
        .unwrap();
    assert_eq!(
        orchestration.legs().get(&inspect).unwrap().attempts.len(),
        2
    );
    assert_eq!(
        orchestration.legs().get(&inspect).unwrap().attempts[0].execution_ref,
        "execution:inspect-first"
    );
    assert_eq!(
        orchestration.legs().get(&inspect).unwrap().attempts[0].status,
        LegStatus::Failed
    );
    assert_eq!(
        orchestration.legs().get(&inspect).unwrap().attempts[1].execution_ref,
        "execution:inspect-retry"
    );
    assert_eq!(
        orchestration
            .retry_grant("grant:inspect")
            .unwrap()
            .attempts_spent,
        2
    );
    orchestration.fail(&inspect, "retry also failed").unwrap();
    assert!(matches!(
        orchestration.retry(
            "journey:factory-198",
            &inspect,
            "grant:inspect",
            ExecutionLaunch {
                execution_ref: "execution:inspect-third".into(),
                disposition: disposition(orchestration.run(), Some(&inspect)),
                retry_grant: Some(orchestration.retry_grant("grant:inspect").unwrap().clone()),
            }
        ),
        Err(OrchestrationError::RetryExhausted(_))
    ));
    assert_eq!(
        orchestration.legs().get(&inspect).unwrap().attempts.len(),
        2
    );
}

#[test]
fn synthesis_requires_actual_artifacts_and_evidence() {
    let mut orchestration = engine();
    let (implementation, review) = start_fork(&mut orchestration);
    orchestration
        .return_artifact(
            &implementation,
            artifact(
                &orchestration,
                &implementation,
                "artifact:implementation",
                "execution:implement",
                "evidence:implementation",
            ),
        )
        .unwrap();
    orchestration
        .return_artifact(
            &review,
            artifact(
                &orchestration,
                &review,
                "artifact:review",
                "execution:review",
                "evidence:review",
            ),
        )
        .unwrap();
    let mut reviewer = disposition(orchestration.run(), None);
    reviewer.demand.independence_from =
        BTreeSet::from(["execution:implement".into(), "execution:review".into()]);
    orchestration
        .register_independent_reviewer(
            "execution:independent-review",
            &reviewer,
            BTreeSet::from([implementation.clone(), review.clone()]),
        )
        .unwrap();
    let record = orchestration.synthesize(
        "synthesis:implementation-reviewed",
        "implementation-reviewed",
        "execution:independent-review",
        BTreeSet::from(["artifact:implementation".into(), "artifact:review".into()]),
        BTreeSet::from(["evidence:implementation".into(), "evidence:review".into()]),
        "The implementation is accepted only after the review's identity and barrier findings agree.",
    ).unwrap();
    assert_eq!(record.artifact_refs.len(), 2);
    assert_eq!(record.evidence_refs.len(), 2);
    assert_eq!(
        record.reviewer_execution_ref,
        "execution:independent-review"
    );
}

#[test]
fn producer_cannot_satisfy_independent_review_lineage() {
    let mut orchestration = engine();
    let (implementation, _) = start_fork(&mut orchestration);
    let producer = disposition(orchestration.run(), None);
    assert!(matches!(
        orchestration.register_independent_reviewer(
            "execution:implement",
            &producer,
            BTreeSet::from([implementation.clone()])
        ),
        Err(OrchestrationError::ReviewerAlreadyProducer(_))
    ));
    let mut renamed_producer = disposition(orchestration.run(), None);
    renamed_producer.demand.independence_from.clear();
    assert!(matches!(
        orchestration.register_independent_reviewer(
            "execution:implement-restarted",
            &renamed_producer,
            BTreeSet::from([implementation])
        ),
        Err(OrchestrationError::ReviewerNotIndependent(_))
    ));
    assert!(orchestration
        .architecture_integrity()
        .findings
        .contains(&ArchitectureFinding::ProducerSelfReview));
}

#[test]
fn architecture_gate_catches_brittle_scripts_orphans_incomplete_barriers_and_concatentation() {
    let mut orchestration = engine();
    orchestration
        .admit_architecture_observation(OwnerArchitectureObservationReceipt {
            observation_ref: "observation:brittle-script".into(),
            owner_ref: "agency:architecture-assessment".into(),
            finding: ArchitectureFinding::BrittleWorkflowScripting,
            subject_ref: "script:hand-authored-dag".into(),
            evidence_refs: BTreeSet::from(["evidence:script-inspection".into()]),
        })
        .unwrap();
    orchestration
        .admit_architecture_observation(OwnerArchitectureObservationReceipt {
            observation_ref: "observation:orphan".into(),
            owner_ref: "agency:architecture-assessment".into(),
            finding: ArchitectureFinding::OrphanExecutionLeg,
            subject_ref: "execution:orphan".into(),
            evidence_refs: BTreeSet::from(["evidence:orphan-inspection".into()]),
        })
        .unwrap();
    orchestration
        .attempt_concatenated_synthesis("implementation-reviewed")
        .unwrap_err();
    let report = orchestration.architecture_integrity();
    assert_eq!(report.contract, "AG-006");
    assert!(report
        .findings
        .contains(&ArchitectureFinding::BrittleWorkflowScripting));
    assert!(report
        .findings
        .contains(&ArchitectureFinding::OrphanExecutionLeg));
    assert!(report
        .findings
        .contains(&ArchitectureFinding::IncompleteBarrier));
    assert!(report
        .findings
        .contains(&ArchitectureFinding::ConcatenationAsSynthesis));
    assert!(!report.passed());
}

#[test]
fn ordinary_serial_work_remains_valid_without_a_fork() {
    let mut source = source();
    source.units.truncate(1);
    source.barriers.clear();
    source.nesting.clear();
    source.source.digest = workflow_source_digest(&source).unwrap();
    let workflow = compile_workflow(source).unwrap();
    let mut orchestration = ExecutableOrchestration::new(workflow, run()).unwrap();
    let inspect = unit(orchestration.workflow(), "inspect-source");
    orchestration
        .start_serial(
            "journey:factory-198",
            &inspect,
            launch(&orchestration, &inspect, "execution:serial"),
        )
        .unwrap();
    orchestration
        .return_artifact(
            &inspect,
            artifact(
                &orchestration,
                &inspect,
                "artifact:serial",
                "execution:serial",
                "evidence:serial",
            ),
        )
        .unwrap();
    assert_eq!(
        orchestration.leg(&inspect).unwrap().status,
        LegStatus::Returned
    );
    assert_eq!(orchestration.whole_run_state(), WholeRunState::Complete);
}

#[test]
fn retry_revocation_is_visible_and_does_not_create_authority() {
    let mut orchestration = engine();
    let inspect = unit(orchestration.workflow(), "inspect-source");
    let grant = RetryGrant::new("grant:revoked", 2).unwrap();
    orchestration.open_retry_grant(grant.clone()).unwrap();
    orchestration.revoke_retry_grant("grant:revoked").unwrap();
    let mut launch = launch(&orchestration, &inspect, "execution:revoked");
    launch.retry_grant = Some(grant);
    assert!(matches!(
        orchestration.start_serial("journey:factory-198", &inspect, launch),
        Err(OrchestrationError::RetryRevoked(_))
    ));
    assert!(orchestration.legs().is_empty());
}
