use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_runtime::{
    AttemptStart, AttemptTrackingFact, ExecutionBody, ExecutionBudget, FactoryAttemptActionRequest,
    FactoryAttemptOperation, FactoryAttemptReading, FactoryAttemptSeed, OwnerOperationPhase,
    OwnerOperationReceipt, PlacementProtection, ReadableReturn, ReresolutionRecord,
    SituatedExecutionDisposition, SituatedParticipant, VerificationOutcome, VerificationReceipt,
    FACTORY_ATTEMPT_ACTION, FACTORY_ATTEMPT_CAPABILITY_REF,
};
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{LegStatus, RetryGrant, ReturnedArtifact};
use epilogos_factory::project_development_store::read_developmental_state;
use epilogos_factory::sensing::{Observation, Policy};
use epilogos_factory::sensing_sources::verify_current;
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";

#[test]
fn unit_decision_survives_fresh_clients_and_role_labels_cannot_resume_it() {
    use epilogos_factory::core::run::{RunLifecycle, RunLifecycleCommand};
    use epilogos_factory::run_lifecycle::{
        UnitDecisionBasis, UnitDecisionOutcome, UnitDecisionRequest, UnitDecisionResponse,
    };
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    let unit = workflow.unit("inspect-source").unwrap();
    let current = read_developmental_state(&fixture.state).unwrap();
    let run = current.build.run(fixture.run.reference()).unwrap();
    fixture
        .action(FactoryAttemptOperation::TransitionRun {
            command: RunLifecycleCommand {
                command_id: "actual-owner-activate".into(),
                expected_revision: run.revision(),
                lifecycle: RunLifecycle::Active,
            },
            authority: run.mutation_authority(),
            closure: None,
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:decision-unit".into(),
            task_ref: "task:decision-unit".into(),
            parent_journey_ref: "journey:decision-case".into(),
            workflow_unit_ref: unit.reference.clone(),
            disposition: disposition(&fixture.run, &workflow, "inspect-source", None),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    let basis = UnitDecisionBasis {
        workflow_unit_ref: unit.reference.clone(),
        attempt_ref: "attempt:decision-unit".into(),
        execution_ref: fixture.reading().legs[&unit.reference]
            .execution_ref
            .clone(),
        subject_ref: unit.subject_ref.to_string(),
        subject_revision: unit.basis_revision.clone(),
        workflow_source_ref: workflow.source.reference.to_string(),
        workflow_source_revision: workflow.source.revision.clone(),
        workflow_source_digest: workflow.source.digest.clone(),
        resolver_ref: "human:controlled-owner".into(),
        controlled: true,
    };
    fixture
        .action(FactoryAttemptOperation::RequestUnitDecision {
            request: UnitDecisionRequest {
                human_request_ref: "human-request:unit-controlled".into(),
                decision_ref: "decision:unit-controlled".into(),
                question: "Resume this bounded controlled unit?".into(),
                why_human: "Controlled protocol verification explicitly requires a human response."
                    .into(),
                basis,
                evidence_refs: BTreeSet::from(["evidence:decision-cut".into()]),
            },
        })
        .unwrap();
    let waiting = fixture.reading();
    assert_eq!(waiting.lifecycle, RunLifecycle::WaitingHuman);
    assert_ne!(
        waiting.whole_run_state,
        epilogos_factory::orchestration::WholeRunState::Complete
    );
    let response = UnitDecisionResponse {
        response_ref: "response:human-actual-channel".into(),
        resolver_ref: "human:controlled-owner".into(),
        channel_receipt_ref: "channel:controlled-test-human-reply".into(),
        source_revision: "reply-source:1".into(),
        outcome: UnitDecisionOutcome::Resume,
        evidence_refs: BTreeSet::from(["evidence:reply".into()]),
        controlled: true,
    };
    let operation = FactoryAttemptOperation::ResolveUnitDecision {
        human_request_ref: "human-request:unit-controlled".into(),
        response,
    };
    assert!(
        fixture.action(operation.clone()).is_err(),
        "an Agent role label cannot answer a human request"
    );
    let mut human = request(fixture.run.reference().clone(), waiting.revision, operation);
    human.caller.caller_ref = "human:controlled-owner".into();
    human.caller.lineage = vec!["human:controlled-owner".into()];
    human.caller.projection_kind = FactoryActionProjectionKind::DesktopHuman;
    let bytes = std::fs::read(&fixture.state).unwrap();
    assert!(!fixture.raw_action(&human).status.success());
    assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
    let after = fixture.reading();
    assert_eq!(after.lifecycle, RunLifecycle::WaitingHuman);
    let revision = after.revision;
    assert!(!fixture.raw_action(&human).status.success());
    assert_eq!(
        fixture.reading().revision,
        revision,
        "repeating an unadmitted response must not change native state"
    );
    let canonical = read_developmental_state(&fixture.state).unwrap();
    let reading = canonical.run_reading(fixture.run.reference()).unwrap();
    assert!(reading.human_requests[0].unit_decision_response.is_none());
    assert_eq!(reading.native_attempts.as_ref().unwrap(), &after);
    let build = canonical.build_snapshot(fixture.run.reference()).unwrap();
    assert_eq!(build.view.native_attempts.as_ref().unwrap(), &after);
    assert_eq!(
        fixture.reading().legs[&unit.reference].status,
        LegStatus::Active
    );
}

#[test]
fn owner_lifecycle_cannot_complete_from_the_present_subset_of_units() {
    use epilogos_factory::core::run::{RunLifecycle, RunLifecycleCommand};
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:subset".into(),
            task_ref: "task:subset".into(),
            parent_journey_ref: "journey:subset".into(),
            workflow_unit_ref: workflow.unit("inspect-source").unwrap().reference.clone(),
            disposition: disposition(&fixture.run, &workflow, "inspect-source", None),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    complete(
        &fixture,
        &workflow,
        "attempt:subset",
        "inspect-source",
        "execution:subset",
    );
    let current = read_developmental_state(&fixture.state).unwrap();
    let run = current.build.run(fixture.run.reference()).unwrap();
    fixture
        .action(FactoryAttemptOperation::TransitionRun {
            command: RunLifecycleCommand {
                command_id: "activate-subset".into(),
                expected_revision: run.revision(),
                lifecycle: RunLifecycle::Active,
            },
            authority: run.mutation_authority(),
            closure: None,
        })
        .unwrap();
    let current = read_developmental_state(&fixture.state).unwrap();
    let run = current.build.run(fixture.run.reference()).unwrap();
    let before = std::fs::read(&fixture.state).unwrap();
    assert!(fixture
        .action(FactoryAttemptOperation::TransitionRun {
            command: RunLifecycleCommand {
                command_id: "false-finish".into(),
                expected_revision: run.revision(),
                lifecycle: RunLifecycle::Finishing,
            },
            authority: run.mutation_authority(),
            closure: None
        })
        .is_err());
    assert_eq!(
        std::fs::read(&fixture.state).unwrap(),
        before,
        "refused closure cannot partially write"
    );
    let reading = fixture.reading();
    assert!(reading.required_units.len() > reading.current_returned_units.len());
    assert_eq!(
        reading.whole_run_state,
        epilogos_factory::orchestration::WholeRunState::Incomplete
    );
}

fn controlled_decision(fixture: &Fixture, workflow: &CompiledWorkflow, bound: bool) {
    use epilogos_factory::run_lifecycle::{UnitDecisionBasis, UnitDecisionRequest};
    owner_transition(
        fixture,
        "decision-activate",
        epilogos_factory::core::run::RunLifecycle::Active,
        None,
    )
    .unwrap();
    let unit = workflow.unit("inspect-source").unwrap();
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:pending-decision".into(),
            task_ref: "task:pending-decision".into(),
            parent_journey_ref: "journey:controlled-decision".into(),
            workflow_unit_ref: unit.reference.clone(),
            disposition: disposition(&fixture.run, workflow, "inspect-source", None),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    if bound {
        fixture
            .action(FactoryAttemptOperation::BindDispatch {
                attempt_ref: "attempt:pending-decision".into(),
                execution_ref: "execution:decision-bound".into(),
                receipt: owner_receipt(
                    "aikit/session-space",
                    "delivery:pending-decision",
                    "receipt:pending-decision",
                    OwnerOperationPhase::Returned,
                    set(["evidence:controlled-owner-returned"]),
                    BTreeSet::new(),
                ),
            })
            .unwrap();
        fixture
            .action(FactoryAttemptOperation::RecordVerification {
                attempt_ref: "attempt:pending-decision".into(),
                verification: verification(
                    workflow,
                    "inspect-source",
                    "verification:decision-bound",
                ),
            })
            .unwrap();
    }
    let basis = UnitDecisionBasis {
        workflow_unit_ref: unit.reference.clone(),
        attempt_ref: "attempt:pending-decision".into(),
        execution_ref: fixture.reading().legs[&unit.reference]
            .execution_ref
            .clone(),
        subject_ref: unit.subject_ref.to_string(),
        subject_revision: unit.basis_revision.clone(),
        workflow_source_ref: workflow.source.reference.to_string(),
        workflow_source_revision: workflow.source.revision.clone(),
        workflow_source_digest: workflow.source.digest.clone(),
        resolver_ref: "human:controlled-owner".into(),
        controlled: true,
    };
    fixture
        .action(FactoryAttemptOperation::RequestUnitDecision {
            request: UnitDecisionRequest {
                human_request_ref: "human-request:pending-decision".into(),
                decision_ref: "decision:pending-decision".into(),
                question: "Resume this controlled unit?".into(),
                why_human: "Controlled human protocol exercise".into(),
                basis,
                evidence_refs: set(["evidence:decision-cut"]),
            },
        })
        .unwrap();
}

fn controlled_human_response(fixture: &Fixture) -> FactoryAttemptActionRequest {
    use epilogos_factory::run_lifecycle::{UnitDecisionOutcome, UnitDecisionResponse};
    let mut response = request(
        fixture.run.reference().clone(),
        fixture.reading().revision,
        FactoryAttemptOperation::ResolveUnitDecision {
            human_request_ref: "human-request:pending-decision".into(),
            response: UnitDecisionResponse {
                response_ref: "response:controlled-human".into(),
                resolver_ref: "human:controlled-owner".into(),
                channel_receipt_ref: "channel:controlled-human-reply".into(),
                source_revision: "reply:1".into(),
                outcome: UnitDecisionOutcome::Resume,
                evidence_refs: set(["evidence:controlled-reply"]),
                controlled: true,
            },
        },
    );
    response.caller.caller_ref = "human:controlled-owner".into();
    response.caller.lineage = vec!["human:controlled-owner".into()];
    response.caller.projection_kind = FactoryActionProjectionKind::DesktopHuman;
    response
}

#[test]
fn pending_decision_blocks_binding_and_an_otherwise_verified_return_without_losing_bytes() {
    for bound in [false, true] {
        let fixture = Fixture::new(source());
        let workflow = fixture.workflow();
        controlled_decision(&fixture, &workflow, bound);
        let operation = if bound {
            return_operation(
                &workflow,
                "attempt:pending-decision",
                "inspect-source",
                "execution:decision-bound",
                "artifact:decision-bound",
            )
        } else {
            FactoryAttemptOperation::BindDispatch {
                attempt_ref: "attempt:pending-decision".into(),
                execution_ref: "execution:decision-bound".into(),
                receipt: owner_receipt(
                    "aikit/session-space",
                    "delivery:pending-decision",
                    "receipt:pending-decision",
                    OwnerOperationPhase::Returned,
                    set(["evidence:controlled-owner-returned"]),
                    BTreeSet::new(),
                ),
            }
        };
        let before = std::fs::read(&fixture.state).unwrap();
        assert!(fixture
            .action(operation)
            .unwrap_err()
            .contains("awaits its canonical human decision"));
        assert_eq!(std::fs::read(&fixture.state).unwrap(), before);
        assert!(!fixture
            .raw_action(&controlled_human_response(&fixture))
            .status
            .success());
        assert_eq!(std::fs::read(&fixture.state).unwrap(), before);
        assert_eq!(
            fixture.reading().legs[&workflow.unit("inspect-source").unwrap().reference].status,
            LegStatus::Active
        );
    }
}

#[test]
fn human_response_cannot_resume_an_affected_unit_after_its_subject_has_changed() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    controlled_decision(&fixture, &workflow, true);
    let original = read_developmental_state(&fixture.state)
        .unwrap()
        .run_reading(fixture.run.reference())
        .unwrap()
        .human_requests[0]
        .clone();
    fixture
        .action(FactoryAttemptOperation::AdvanceSubject {
            subject_ref: workflow
                .unit("inspect-source")
                .unwrap()
                .subject_ref
                .to_string(),
            revision: "explicit-new-candidate".into(),
        })
        .unwrap();
    let before = std::fs::read(&fixture.state).unwrap();
    assert!(!fixture
        .raw_action(&controlled_human_response(&fixture))
        .status
        .success());
    assert_eq!(std::fs::read(&fixture.state).unwrap(), before);
    let retained = read_developmental_state(&fixture.state)
        .unwrap()
        .run_reading(fixture.run.reference())
        .unwrap();
    assert_eq!(retained.human_requests[0], original);
    assert!(retained.human_requests[0].unit_decision_response.is_none());
}

#[test]
fn a_human_request_cannot_substitute_another_workflow_source_digest() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    controlled_decision(&fixture, &workflow, false);
    let retained = read_developmental_state(&fixture.state)
        .unwrap()
        .run_reading(fixture.run.reference())
        .unwrap()
        .human_requests[0]
        .clone();
    let mut basis = retained.unit_decision_basis.unwrap();
    basis.workflow_source_digest = "0".repeat(64);
    let before = std::fs::read(&fixture.state).unwrap();
    assert!(fixture
        .action(FactoryAttemptOperation::RequestUnitDecision {
            request: epilogos_factory::run_lifecycle::UnitDecisionRequest {
                human_request_ref: "human-request:foreign-source".into(),
                decision_ref: "decision:foreign-source".into(),
                question: "A different source cannot substitute for this unit".into(),
                why_human: "Controlled negative case".into(),
                basis,
                evidence_refs: set(["evidence:foreign-source-declaration"]),
            }
        })
        .unwrap_err()
        .contains("different workflow basis"));
    assert_eq!(std::fs::read(&fixture.state).unwrap(), before);
}

fn owner_transition(
    fixture: &Fixture,
    id: &str,
    lifecycle: epilogos_factory::core::run::RunLifecycle,
    closure: Option<epilogos_factory::run_lifecycle::RunClosureBasis>,
) -> Result<Value, String> {
    let state = read_developmental_state(&fixture.state).unwrap();
    let run = state.build.run(fixture.run.reference()).unwrap();
    fixture.action(FactoryAttemptOperation::TransitionRun {
        command: epilogos_factory::core::run::RunLifecycleCommand {
            command_id: id.into(),
            expected_revision: run.revision(),
            lifecycle,
        },
        authority: run.mutation_authority(),
        closure,
    })
}

#[test]
fn obsolete_unit_decision_retirement_retains_its_basis_and_refuses_late_human_reply() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    controlled_decision(&fixture, &workflow, true);
    let retire = FactoryAttemptOperation::RetireUnitDecision {
        human_request_ref: "human-request:pending-decision".into(),
        reason: "Affected subject changed under explicit owner advance".into(),
        replacement_basis_refs: set(["source:explicit-new-candidate"]),
    };
    let before = std::fs::read(&fixture.state).unwrap();
    assert!(
        fixture.action(retire.clone()).is_err(),
        "current decisions cannot be retired as bookkeeping"
    );
    assert_eq!(std::fs::read(&fixture.state).unwrap(), before);
    fixture
        .action(FactoryAttemptOperation::AdvanceSubject {
            subject_ref: workflow
                .unit("inspect-source")
                .unwrap()
                .subject_ref
                .to_string(),
            revision: "explicit-new-candidate".into(),
        })
        .unwrap();
    let old_basis = read_developmental_state(&fixture.state)
        .unwrap()
        .run_reading(fixture.run.reference())
        .unwrap()
        .human_requests[0]
        .unit_decision_basis
        .clone();
    fixture.action(retire).unwrap();
    let retired = read_developmental_state(&fixture.state)
        .unwrap()
        .run_reading(fixture.run.reference())
        .unwrap();
    assert_eq!(retired.human_requests[0].unit_decision_basis, old_basis);
    assert!(retired.human_requests[0].unit_decision_response.is_none());
    assert!(retired.human_requests[0].unit_decision_retirement.is_some());
    assert_eq!(
        fixture.reading().lifecycle,
        epilogos_factory::core::run::RunLifecycle::Active
    );
    let bytes = std::fs::read(&fixture.state).unwrap();
    assert!(!fixture
        .raw_action(&controlled_human_response(&fixture))
        .status
        .success());
    assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
}

#[test]
fn one_unit_decision_keeps_an_independent_ready_frontier_active() {
    let mut source = source();
    source.units[2].dependencies.clear();
    source.units[2].independence_from.clear();
    source.source.digest = workflow_source_digest(&source).unwrap();
    let fixture = Fixture::new(source);
    let workflow = fixture.workflow();
    controlled_decision(&fixture, &workflow, false);
    assert_eq!(
        fixture.reading().lifecycle,
        epilogos_factory::core::run::RunLifecycle::Active
    );
    let next = start(
        &fixture,
        &workflow,
        "review-adversarially",
        "attempt:independent-ready",
    );
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: next.attempt_ref,
            task_ref: next.task_ref,
            parent_journey_ref: "journey:controlled-decision".into(),
            workflow_unit_ref: next.workflow_unit_ref,
            disposition: next.disposition,
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    assert_eq!(
        fixture.reading().legs[&workflow.unit("review-adversarially").unwrap().reference].status,
        LegStatus::Active
    );
    let before = fixture.reading();
    complete(
        &fixture,
        &workflow,
        "attempt:independent-ready",
        "review-adversarially",
        "execution:independent-ready",
    );
    let after = fixture.reading();
    assert_eq!(
        after.lifecycle,
        epilogos_factory::core::run::RunLifecycle::WaitingHuman
    );
    assert_eq!(
        after.legs[&workflow.unit("review-adversarially").unwrap().reference].status,
        LegStatus::Returned
    );
    assert!(after.run_revision > before.run_revision);
    assert!(after.topology_revision > before.topology_revision);
    let native = read_developmental_state(&fixture.state).unwrap();
    assert_eq!(
        native
            .run_reading(fixture.run.reference())
            .unwrap()
            .native_attempts
            .as_ref(),
        Some(&after)
    );
}

#[test]
fn a_controlled_cancel_label_without_native_review_cannot_change_cancellation_or_lifecycle() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    controlled_decision(&fixture, &workflow, true);
    let mut human = controlled_human_response(&fixture);
    if let FactoryAttemptOperation::ResolveUnitDecision { response, .. } = &mut human.operation {
        response.outcome = epilogos_factory::run_lifecycle::UnitDecisionOutcome::Cancel;
    }
    let bytes = std::fs::read(&fixture.state).unwrap();
    assert!(!fixture.raw_action(&human).status.success());
    assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
    let after = fixture.reading();
    assert_eq!(
        after.lifecycle,
        epilogos_factory::core::run::RunLifecycle::WaitingHuman
    );
    assert_eq!(
        after.legs[&workflow.unit("inspect-source").unwrap().reference].status,
        LegStatus::Active
    );
    let bytes = std::fs::read(&fixture.state).unwrap();
    assert!(!fixture.raw_action(&human).status.success());
    assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
    let native = read_developmental_state(&fixture.state).unwrap();
    assert!(native
        .run_reading(fixture.run.reference())
        .unwrap()
        .human_requests[0]
        .unit_decision_response
        .is_none());
}

fn closure_source() -> WorkflowSource {
    use epilogos_factory::workflow::WorkflowBarrierSource;
    use epilogos_factory::workflow_inputs::WorkflowInputSource;
    let mut source = source();
    source.units.retain(|unit| unit.key != "implement-compiler");
    source.nesting.clear();
    source.barriers = vec![WorkflowBarrierSource {
        key: "native-closure".into(),
        waits_for: vec!["inspect-source".into()],
        releases: vec!["integrate-return".into()],
    }];
    for unit in &mut source.units {
        unit.subject_ref = format!("subject:controlled-{}", unit.key).parse().unwrap();
        unit.agent_requirements.agent_refs = vec![format!("agent:controlled-{}", unit.key)];
        unit.agent_requirements.agency_refs = vec![format!("agency:controlled-{}", unit.key)];
        unit.permitted_effects =
            vec!["read controlled source and return contracted difference".into()];
        unit.independence_from.clear();
        unit.dependencies = match unit.key.as_str() {
            "inspect-source" => vec![],
            "review-adversarially" => vec!["inspect-source".into()],
            _ => vec!["inspect-source".into(), "review-adversarially".into()],
        };
        unit.inputs = unit
            .dependencies
            .iter()
            .map(|key| WorkflowInputSource {
                predecessor: key.clone(),
                receiving_context_ref: format!("context:closure-{}-from-{key}", unit.key),
            })
            .collect();
    }
    source.source.digest = workflow_source_digest(&source).unwrap();
    source
}

fn closure_contribution(
    fixture: &Fixture,
    workflow: &CompiledWorkflow,
    key: &str,
    generation: u8,
) -> String {
    use epilogos_factory::workflow_inputs::SelectedWorkflowInput;
    let unit = workflow.unit(key).unwrap();
    let grant = RetryGrant::new(format!("grant:closure-{key}"), 2).unwrap();
    let mut selected = disposition(&fixture.run, workflow, key, Some(&grant));
    let reading = fixture.reading();
    selected.selected_inputs = unit
        .inputs
        .iter()
        .map(|input| {
            let leg = &reading.legs[&input.predecessor];
            selected
                .context_refs
                .insert(input.receiving_context_ref.clone());
            SelectedWorkflowInput {
                predecessor: input.predecessor.clone(),
                execution_ref: leg.execution_ref.clone(),
                receiving_context_ref: input.receiving_context_ref.clone(),
                artifacts: leg.artifacts.clone(),
            }
        })
        .collect();
    if key == "review-adversarially" {
        selected.selection.demand.independence_from.insert(
            reading.legs[&workflow.unit("inspect-source").unwrap().reference]
                .execution_ref
                .clone(),
        );
    }
    let attempt = format!("attempt:closure-{key}-{generation}");
    if generation == 1 {
        fixture
            .action(FactoryAttemptOperation::StartSerial {
                attempt_ref: attempt.clone(),
                task_ref: format!("task:closure-{key}"),
                parent_journey_ref: "journey:controlled-closure".into(),
                workflow_unit_ref: unit.reference.clone(),
                disposition: selected,
                retry_grant: Some(grant),
                tracking: vec![],
                place_grant: None,
            })
            .unwrap();
    } else {
        let mut failed = verification(workflow, key, &format!("verification:reject-{key}-1"));
        failed.outcome = VerificationOutcome::Failed;
        fixture
            .action(FactoryAttemptOperation::RecordVerification {
                attempt_ref: format!("attempt:closure-{key}-1"),
                verification: failed,
            })
            .unwrap();
        let rejected = fixture.reading();
        let prior = rejected
            .attempts
            .iter()
            .find(|record| record.attempt_ref == format!("attempt:closure-{key}-1"))
            .unwrap();
        let mut retained_evidence: BTreeSet<String> = prior
            .dispatch
            .iter()
            .chain(prior.observations.iter())
            .flat_map(|receipt| receipt.partial_effect_refs.iter().cloned())
            .collect();
        for verification in &prior.verifications {
            retained_evidence.insert(verification.verification_ref.clone());
            retained_evidence.extend(verification.evidence_refs.iter().cloned());
        }
        for artifact in rejected.legs[&unit.reference].artifacts.iter().chain(
            selected
                .selected_inputs
                .iter()
                .flat_map(|input| input.artifacts.iter()),
        ) {
            retained_evidence.insert(artifact.artifact_ref.clone());
            retained_evidence.extend(artifact.evidence_refs.iter().cloned());
        }
        let resolution = ReresolutionRecord {
            resolution_ref: format!("resolution:closure-{key}-{generation}"),
            reason: "Rejected retained result; re-resolve against the exact current predecessor artifacts".into(),
            source_revision: selected.participant.source_revision.clone(),
            evidence_refs: retained_evidence,
            replacement_now_ref: None,
            replacement_material_ref: None,
            replacement_harness_ref: None,
        };
        fixture
            .action(FactoryAttemptOperation::Retry {
                attempt_ref: attempt.clone(),
                task_ref: format!("task:closure-{key}"),
                parent_journey_ref: "journey:controlled-closure".into(),
                workflow_unit_ref: unit.reference.clone(),
                grant_ref: grant.grant_ref,
                disposition: selected,
                tracking: vec![],
                reresolution: Some(resolution),
                place_grant: None,
            })
            .unwrap();
    }
    complete(
        fixture,
        workflow,
        &attempt,
        key,
        &format!("execution:closure-{key}-{generation}"),
    );
    if key == "review-adversarially" {
        fixture
            .action(FactoryAttemptOperation::RegisterIndependentReview {
                attempt_ref: attempt.clone(),
                review_of: BTreeSet::from([workflow
                    .unit("inspect-source")
                    .unwrap()
                    .reference
                    .clone()]),
            })
            .unwrap();
    }
    attempt
}

fn closure_synthesis(
    fixture: &Fixture,
    producer: &str,
    reviewer: &str,
    result: &str,
    generation: u8,
) {
    fixture
        .action(FactoryAttemptOperation::Synthesize {
            attempt_ref: result.into(),
            reviewer_attempt_ref: reviewer.into(),
            synthesis_ref: format!("synthesis:closure-{generation}"),
            barrier_key: "native-closure".into(),
            result_artifact_ref: format!("artifact:{result}"),
            artifact_refs: set([format!("artifact:{producer}")]),
        })
        .unwrap();
}

fn closure_receiving(
    fixture: &Fixture,
    result: &str,
    generation: u8,
) -> epilogos_factory::run_lifecycle::RunClosureBasis {
    let receiving = format!("receiving:controlled-closure-{generation}");
    fixture
        .action(FactoryAttemptOperation::AttachReceiving {
            attempt_ref: result.into(),
            receiving_ref: receiving.clone(),
            source_revision: format!("controlled-receiving-basis-{generation}"),
            evidence_refs: set([format!("evidence:controlled-receiving-{generation}")]),
        })
        .unwrap();
    epilogos_factory::run_lifecycle::RunClosureBasis {
        final_attempt_ref: result.into(),
        reviewer_attempt_refs: set([format!("attempt:closure-review-adversarially-{generation}")]),
        receiving_ref: receiving,
    }
}

#[test]
fn native_review_and_synthesis_do_not_treat_a_receiving_label_as_closure_proof() {
    use epilogos_factory::core::run::RunLifecycle;
    let fixture = Fixture::new(closure_source());
    let workflow = fixture.workflow();
    owner_transition(&fixture, "closure-activate", RunLifecycle::Active, None).unwrap();
    let producer = closure_contribution(&fixture, &workflow, "inspect-source", 1);
    let reviewer = closure_contribution(&fixture, &workflow, "review-adversarially", 1);
    let result = closure_contribution(&fixture, &workflow, "integrate-return", 1);
    closure_synthesis(&fixture, &producer, &reviewer, &result, 1);
    owner_transition(&fixture, "closure-finishing", RunLifecycle::Finishing, None).unwrap();
    let bytes = std::fs::read(&fixture.state).unwrap();
    assert!(owner_transition(
        &fixture,
        "missing-receiving-finish",
        RunLifecycle::Finished,
        Some(epilogos_factory::run_lifecycle::RunClosureBasis {
            final_attempt_ref: result.clone(),
            reviewer_attempt_refs: set([reviewer.clone()]),
            receiving_ref: "receiving:absent".into(),
        })
    )
    .is_err());
    assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
    let basis = closure_receiving(&fixture, &result, 1);
    let bytes = std::fs::read(&fixture.state).unwrap();
    assert!(fixture
        .action(FactoryAttemptOperation::RecordObservation {
            attempt_ref: result.clone(),
            receipt: OwnerOperationReceipt {
                owner_ref: "central".into(),
                contract: "central.receiving-reading/v1".into(),
                operation_ref: "central.receiving:unadmitted-label".into(),
                receipt_ref: "native-receiving-admission:caller-json-cannot-write-this".into(),
                source_revision: "controlled-reading-1".into(),
                phase: OwnerOperationPhase::Observed,
                evidence_refs: set(["evidence:controlled-label"]),
                partial_effect_refs: BTreeSet::new(),
                payload: json!({"factoryActionDigest":"caller-authored-digest"}),
            },
        })
        .unwrap_err()
        .contains("only by the canonical live-owner transaction"));
    assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
    assert!(owner_transition(
        &fixture,
        "closure-finished",
        RunLifecycle::Finished,
        Some(basis),
    )
    .is_err());
    assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
    let reading = fixture.reading();
    assert_eq!(reading.lifecycle, RunLifecycle::Finishing);
    assert_eq!(
        reading.whole_run_state,
        epilogos_factory::orchestration::WholeRunState::Complete
    );
    let native = read_developmental_state(&fixture.state).unwrap();
    assert_eq!(
        native
            .run_reading(fixture.run.reference())
            .unwrap()
            .native_attempts
            .as_ref(),
        Some(&reading)
    );
    assert_eq!(
        native
            .build_snapshot(fixture.run.reference())
            .unwrap()
            .view
            .native_attempts
            .as_ref(),
        Some(&reading)
    );
}

#[test]
fn an_old_synthesis_cannot_close_replaced_producer_or_reviewer_material() {
    use epilogos_factory::core::run::RunLifecycle;
    for replace_producer in [false, true] {
        let fixture = Fixture::new(closure_source());
        let workflow = fixture.workflow();
        owner_transition(&fixture, "candidate-activate", RunLifecycle::Active, None).unwrap();
        let old_producer = closure_contribution(&fixture, &workflow, "inspect-source", 1);
        let old_reviewer = closure_contribution(&fixture, &workflow, "review-adversarially", 1);
        let old_result = closure_contribution(&fixture, &workflow, "integrate-return", 1);
        closure_synthesis(&fixture, &old_producer, &old_reviewer, &old_result, 1);
        let producer = if replace_producer {
            closure_contribution(&fixture, &workflow, "inspect-source", 2)
        } else {
            old_producer
        };
        let reviewer = closure_contribution(&fixture, &workflow, "review-adversarially", 2);
        let result = closure_contribution(&fixture, &workflow, "integrate-return", 2);
        let basis = closure_receiving(&fixture, &result, 2);
        assert_eq!(
            fixture.reading().whole_run_state,
            epilogos_factory::orchestration::WholeRunState::Complete
        );
        owner_transition(
            &fixture,
            "replacement-finishing",
            RunLifecycle::Finishing,
            None,
        )
        .unwrap();
        let bytes = std::fs::read(&fixture.state).unwrap();
        assert!(owner_transition(
            &fixture,
            "old-synthesis-finish",
            RunLifecycle::Finished,
            Some(basis.clone())
        )
        .unwrap_err()
        .contains("exact current producer/reviewer material"));
        assert_eq!(std::fs::read(&fixture.state).unwrap(), bytes);
        closure_synthesis(&fixture, &producer, &reviewer, &result, 2);
        assert!(owner_transition(
            &fixture,
            "replacement-finished",
            RunLifecycle::Finished,
            Some(basis),
        )
        .is_err());
        assert_eq!(fixture.reading().lifecycle, RunLifecycle::Finishing);
        assert!(!fixture.reading().completion_verified);
    }
}

#[test]
fn seeded_finished_history_and_archived_returned_legs_do_not_create_completion_admission() {
    use epilogos_factory::core::run::RunLifecycle;
    // Core lifecycle is a preserved public contract, but its imported history
    // does not contain an actual native attempt/receiving closure admission.
    for archived in [false, true] {
        let fixture = Fixture::with_seed_lifecycle(
            source(),
            if archived {
                vec![
                    RunLifecycle::Active,
                    RunLifecycle::Finishing,
                    RunLifecycle::Finished,
                    RunLifecycle::Archived,
                ]
            } else {
                vec![
                    RunLifecycle::Active,
                    RunLifecycle::Finishing,
                    RunLifecycle::Finished,
                ]
            },
        );
        let reading = fixture.reading();
        assert_eq!(
            reading.lifecycle,
            if archived {
                RunLifecycle::Archived
            } else {
                RunLifecycle::Finished
            }
        );
        assert!(reading.attempts.is_empty());
        assert!(!reading.completion_verified);
    }
    let fixture = Fixture::new(closure_source());
    let workflow = fixture.workflow();
    owner_transition(&fixture, "unreceived-activate", RunLifecycle::Active, None).unwrap();
    closure_contribution(&fixture, &workflow, "inspect-source", 1);
    closure_contribution(&fixture, &workflow, "review-adversarially", 1);
    closure_contribution(&fixture, &workflow, "integrate-return", 1);
    assert_eq!(
        fixture.reading().whole_run_state,
        epilogos_factory::orchestration::WholeRunState::Complete
    );
    owner_transition(&fixture, "unreceived-abort", RunLifecycle::Aborted, None).unwrap();
    owner_transition(&fixture, "unreceived-archive", RunLifecycle::Archived, None).unwrap();
    let reading = fixture.reading();
    assert_eq!(reading.lifecycle, RunLifecycle::Archived);
    assert_eq!(
        reading.whole_run_state,
        epilogos_factory::orchestration::WholeRunState::Complete
    );
    assert_eq!(reading.required_units, reading.current_returned_units);
    assert!(!reading.completion_verified);
}

#[test]
fn rejected_return_reopens_its_unit_and_blocks_dependents_after_restart() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    let unit = workflow.unit("inspect-source").unwrap().reference.clone();
    let grant = RetryGrant::new("grant:rejection-repair", 2).unwrap();
    let selected = disposition(&fixture.run, &workflow, "inspect-source", Some(&grant));
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:rejected".into(),
            task_ref: "task:rejected".into(),
            parent_journey_ref: "journey:rejection".into(),
            workflow_unit_ref: unit.clone(),
            disposition: selected.clone(),
            retry_grant: Some(grant.clone()),
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    complete(
        &fixture,
        &workflow,
        "attempt:rejected",
        "inspect-source",
        "execution:rejected",
    );
    let mut rejection = verification(
        &workflow,
        "inspect-source",
        "verification:independent-rejection",
    );
    rejection.outcome = VerificationOutcome::Failed;
    fixture
        .action(FactoryAttemptOperation::RecordVerification {
            attempt_ref: "attempt:rejected".into(),
            verification: rejection,
        })
        .unwrap();
    // Every invocation is a fresh native CLI process over the retained owner state.
    let reopened = fixture.reading();
    assert_eq!(reopened.legs[&unit].status, LegStatus::Failed);
    assert_eq!(reopened.legs[&unit].artifacts.len(), 1);
    assert!(reopened.attempts[0].readable_return.is_some());
    assert!(
        fixture
            .action(FactoryAttemptOperation::StartSerial {
                attempt_ref: "attempt:blocked".into(),
                task_ref: "task:blocked".into(),
                parent_journey_ref: "journey:rejection".into(),
                workflow_unit_ref: workflow
                    .unit("implement-compiler")
                    .unwrap()
                    .reference
                    .clone(),
                disposition: disposition(&fixture.run, &workflow, "implement-compiler", None),
                retry_grant: None,
                tracking: vec![],
                place_grant: None,
            })
            .is_err(),
        "rejected predecessor must not admit dependent work"
    );
    // Correction needs a fresh, bounded attempt. A later assessment of the
    // rejected predecessor must not downgrade that replacement execution.
    fixture
        .action(FactoryAttemptOperation::Retry {
            attempt_ref: "attempt:corrected".into(),
            task_ref: "task:rejected".into(),
            parent_journey_ref: "journey:rejection".into(),
            workflow_unit_ref: unit.clone(),
            grant_ref: grant.grant_ref,
            disposition: selected,
            tracking: vec![],
            reresolution: None,
            place_grant: None,
        })
        .unwrap();
    complete(
        &fixture,
        &workflow,
        "attempt:corrected",
        "inspect-source",
        "execution:corrected",
    );
    let mut historical = verification(
        &workflow,
        "inspect-source",
        "verification:historical-unknown",
    );
    historical.outcome = VerificationOutcome::Unknown;
    historical.source_revision = "aikit-journal:historical-attempt:19".into();
    fixture
        .action(FactoryAttemptOperation::RecordVerification {
            attempt_ref: "attempt:rejected".into(),
            verification: historical,
        })
        .unwrap();
    let corrected = fixture.reading();
    assert_eq!(corrected.legs[&unit].status, LegStatus::Returned);
    assert_eq!(corrected.legs[&unit].execution_ref, "execution:corrected");
    assert_eq!(corrected.legs[&unit].attempts.len(), 2);
    assert_eq!(corrected.legs[&unit].attempts[0].artifacts.len(), 1);
    assert_eq!(corrected.legs[&unit].attempts[0].status, LegStatus::Failed);
}

#[test]
fn subject_advance_reopens_returned_owner_state_and_preserves_its_bytes() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    let unit = workflow.unit("inspect-source").unwrap();
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:old-subject".into(),
            task_ref: "task:subject".into(),
            parent_journey_ref: "journey:subject".into(),
            workflow_unit_ref: unit.reference.clone(),
            disposition: disposition(&fixture.run, &workflow, "inspect-source", None),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    complete(
        &fixture,
        &workflow,
        "attempt:old-subject",
        "inspect-source",
        "execution:old-subject",
    );
    let original = fixture.reading().legs[&unit.reference].artifacts.clone();
    fixture
        .action(FactoryAttemptOperation::AdvanceSubject {
            subject_ref: unit.subject_ref.to_string(),
            revision: "subject:new-candidate".into(),
        })
        .unwrap();
    let restarted = fixture.reading();
    assert!(
        restarted.source_current,
        "workflow freshness is distinct from subject currency"
    );
    assert_eq!(restarted.legs[&unit.reference].status, LegStatus::Failed);
    assert_eq!(restarted.legs[&unit.reference].artifacts, original);
    assert!(restarted.attempts[0].readable_return.is_some());
}

#[test]
fn failed_owner_dispatch_remains_the_same_signal_after_later_verification() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:failed-dispatch".into(),
            task_ref: "task:failed-dispatch".into(),
            parent_journey_ref: "journey:failed-dispatch".into(),
            workflow_unit_ref: workflow.unit("inspect-source").unwrap().reference.clone(),
            disposition: disposition(&fixture.run, &workflow, "inspect-source", None),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::BindDispatch {
            attempt_ref: "attempt:failed-dispatch".into(),
            execution_ref: "execution:failed-dispatch".into(),
            receipt: owner_receipt(
                "aikit/session-space",
                "delivery:failed-dispatch",
                "receipt:failed-dispatch",
                OwnerOperationPhase::Failed,
                set(["evidence:native-provider-failure"]),
                BTreeSet::new(),
            ),
        })
        .unwrap();
    let policy_path = fixture._dir.path().join("policy.json");
    std::fs::write(&policy_path, serde_json::to_vec(&json!({
        "schema":"factory.sensing-policy/v1","version":1,"project_world_ref":PROJECT,
        "sources":[{"id":"attempts","provider":"factory","scope":PROJECT,
                    "source_ref":"factory:attempts:controlled","arguments":{"kind":"attempts"}}],
        "workflows":{"collect":{"enabled":true,"sources":["attempts"]}}
    })).unwrap()).unwrap();
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let since = (now - chrono::Duration::hours(1)).to_rfc3339();
    let until = (now + chrono::Duration::hours(1)).to_rfc3339();
    let collect = || {
        let output = run_factory(
            &[
                "telemetry".into(),
                "collect".into(),
                fixture.state.display().to_string(),
                "--policy".into(),
                policy_path.display().to_string(),
                "--since".into(),
                since.clone(),
                "--until".into(),
                until.clone(),
                "--json".into(),
            ],
            None,
        );
        assert_success(&output);
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let first = collect();
    assert_eq!(first["collection"]["coverage"][0]["state"], "complete");
    assert_eq!(
        first["collection"]["signal_refs"].as_array().unwrap().len(),
        1
    );
    let signal_ref = first["collection"]["signal_refs"][0].as_str().unwrap();
    let read_signal = || {
        let output = run_factory(
            &[
                "telemetry".into(),
                "signal".into(),
                fixture.state.display().to_string(),
                signal_ref.into(),
                "--json".into(),
            ],
            None,
        );
        assert_success(&output);
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let first_signal = read_signal();
    let original_observation: Observation =
        serde_json::from_value(first_signal["signal"]["observation"].clone()).unwrap();
    let policy: Policy = serde_json::from_slice(&std::fs::read(&policy_path).unwrap()).unwrap();
    verify_current(
        &original_observation,
        &read_developmental_state(&fixture.state).unwrap(),
        &policy,
    )
    .unwrap();
    assert_eq!(
        first_signal["signal"]["observation"]["source_ref"],
        "attempt:failed-dispatch"
    );
    assert!(first_signal["signal"]["observation"]["relation_refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reference| reference == "receipt:failed-dispatch"));
    assert!(
        fixture.reading().attempts[0]
            .failure_evidence_refs
            .is_empty(),
        "failed dispatch has no explicit Fail mutation"
    );
    let stats = run_factory(
        &[
            "telemetry".into(),
            "stats".into(),
            fixture.state.display().to_string(),
            "--template".into(),
            "attempts-by-agency".into(),
            "--json".into(),
        ],
        None,
    );
    assert_success(&stats);
    let stats: Value = serde_json::from_slice(&stats.stdout).unwrap();
    assert_eq!(stats["denominator"]["attempts"], 1);
    assert_eq!(
        stats["groups"]
            .as_object()
            .unwrap()
            .values()
            .map(|group| group["withFailureEvidence"].as_u64().unwrap())
            .sum::<u64>(),
        1
    );

    std::thread::sleep(std::time::Duration::from_millis(10));
    let mut later = verification(&workflow, "inspect-source", "verification:later-failed");
    later.outcome = VerificationOutcome::Failed;
    fixture
        .action(FactoryAttemptOperation::RecordVerification {
            attempt_ref: "attempt:failed-dispatch".into(),
            verification: later,
        })
        .unwrap();
    let next = collect();
    assert_eq!(
        next["collection"]["signal_refs"].as_array().unwrap().len(),
        2
    );
    let after_signal = read_signal();
    assert_eq!(
        after_signal["signal"]["observation"]["source_revision"],
        first_signal["signal"]["observation"]["source_revision"],
        "a later verification must not rewrite the failure source revision"
    );
    assert!(
        after_signal["signal"]["prior_observations"].is_null()
            || after_signal["signal"]["prior_observations"]
                .as_array()
                .is_some_and(Vec::is_empty)
    );
    let timed = fixture.reading().attempts.remove(0);
    assert!(timed
        .verification_recorded_at
        .contains_key("verification:later-failed"));
    verify_current(
        &original_observation,
        &read_developmental_state(&fixture.state).unwrap(),
        &policy,
    )
    .unwrap();
    fixture
        .action(FactoryAttemptOperation::RecordObservation {
            attempt_ref: "attempt:failed-dispatch".into(),
            receipt: owner_receipt(
                "aikit/session-space",
                "delivery:later-failure",
                "receipt:later-failure",
                OwnerOperationPhase::Failed,
                set(["evidence:second-native-failure"]),
                BTreeSet::new(),
            ),
        })
        .unwrap();
    assert!(
        verify_current(
            &original_observation,
            &read_developmental_state(&fixture.state).unwrap(),
            &policy
        )
        .is_err(),
        "new failure evidence must invalidate the old source revision"
    );
}

#[test]
fn native_receipt_admission_times_survive_restart_and_bound_later_verification() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:timed".into(),
            task_ref: "task:timed".into(),
            parent_journey_ref: "journey:timed".into(),
            workflow_unit_ref: workflow.unit("inspect-source").unwrap().reference.clone(),
            disposition: disposition(&fixture.run, &workflow, "inspect-source", None),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        })
        .unwrap();
    complete(
        &fixture,
        &workflow,
        "attempt:timed",
        "inspect-source",
        "execution:timed",
    );
    let first = fixture.reading().attempts.remove(0);
    assert!(first.attempt_recorded_at.is_some());
    assert!(first.return_recorded_at.is_some());
    assert_eq!(first.verification_count_at_return, Some(1));
    assert_eq!(first.verification_recorded_at.len(), 1);

    let before = run_factory(
        &[
            "telemetry".into(),
            "stats".into(),
            fixture.state.display().to_string(),
            "--template".into(),
            "return-to-verification".into(),
            "--json".into(),
        ],
        None,
    );
    assert_success(&before);
    let before: Value = serde_json::from_slice(&before.stdout).unwrap();
    assert_eq!(before["denominator"]["withTimedLaterVerification"], 0);
    assert!(before["meanDurationMs"].is_null());

    fixture
        .action(FactoryAttemptOperation::RecordVerification {
            attempt_ref: "attempt:timed".into(),
            verification: verification(&workflow, "inspect-source", "verification:timed:later"),
        })
        .unwrap();
    let reopened = fixture.reading().attempts.remove(0);
    assert_eq!(reopened.verification_recorded_at.len(), 2);
    let after = run_factory(
        &[
            "telemetry".into(),
            "stats".into(),
            fixture.state.display().to_string(),
            "--template".into(),
            "return-to-verification".into(),
            "--drill-down".into(),
            "--json".into(),
        ],
        None,
    );
    assert_success(&after);
    let after: Value = serde_json::from_slice(&after.stdout).unwrap();
    assert_eq!(after["denominator"]["withTimedLaterVerification"], 1);
    assert_eq!(
        after["verificationTargets"][0]["laterVerificationRef"],
        "verification:timed:later"
    );
    assert!(after["verificationTargets"][0]["durationMs"].is_number());
}

#[test]
fn public_cli_restart_readback_rejects_stale_revision_and_retains_tracking_return_links() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    let unit = workflow.unit("inspect-source").unwrap().reference.clone();
    let first = fixture.reading();
    assert_eq!(first.revision, 1);

    let tracking = vec![
        fact(
            "fact:now",
            "now",
            "central",
            "now:factory-221",
            "central-r150",
        ),
        fact(
            "fact:source",
            "source-revision",
            "central",
            "source:factory",
            "central-r153",
        ),
        fact(
            "fact:usage",
            "resource-usage",
            "workcell",
            "usage:world-1",
            "f3a5be9fc751ee94b78aff11411e0cde65a46e4c",
        ),
        fact(
            "fact:model",
            "model-usage",
            "actuation",
            "usage:model-1",
            "actuation-current",
        ),
    ];
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:inspect-1".into(),
            task_ref: "task:inspect".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: unit.clone(),
            disposition: disposition(&fixture.run, &workflow, "inspect-source", None),
            retry_grant: None,
            tracking: tracking.clone(),
            place_grant: None,
        })
        .unwrap();

    // Every public CLI call is a fresh process, so this read proves persisted
    // restart/readback rather than retaining an in-memory coordinator.
    let reopened = fixture.reading();
    assert_eq!(reopened.attempts.len(), 1);
    assert_eq!(reopened.attempts[0].tracking, tracking);
    assert_eq!(reopened.legs[&unit].status, LegStatus::Active);

    let stale = request(
        fixture.run.reference().clone(),
        1,
        FactoryAttemptOperation::RecordTracking {
            attempt_ref: "attempt:inspect-1".into(),
            fact: fact("fact:stale", "now", "central", "now:stale", "central-r150"),
        },
    );
    let stale_output = fixture.raw_action(&stale);
    assert!(!stale_output.status.success());
    assert_eq!(fixture.reading().attempts[0].tracking, tracking);

    complete(
        &fixture,
        &workflow,
        "attempt:inspect-1",
        "inspect-source",
        "execution:inspect-owner",
    );
    fixture
        .action(FactoryAttemptOperation::AttachReceiving {
            attempt_ref: "attempt:inspect-1".into(),
            receiving_ref: "central-receiving:inspect-1".into(),
            source_revision: "central-152-pr".into(),
            evidence_refs: set(["evidence:receiving-ledger"]),
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::AttachArchive {
            attempt_ref: "attempt:inspect-1".into(),
            archive_ref: "archive:day-2026-09-10".into(),
            regression_observation_ref: Some("regression:inspect-1".into()),
        })
        .unwrap();
    let reading = fixture.reading();
    let attempt = &reading.attempts[0];
    let readable = attempt.readable_return.as_ref().unwrap();
    assert_eq!(
        readable.receiving_ref.as_deref(),
        Some("central-receiving:inspect-1")
    );
    assert_eq!(
        readable.receiving_source_revision.as_deref(),
        Some("central-152-pr")
    );
    assert!(readable.archive_refs.contains("archive:day-2026-09-10"));
    assert!(readable
        .regression_observation_refs
        .contains("regression:inspect-1"));
}

#[test]
fn public_cli_preserves_uncertain_partial_effects_reconciliation_and_bounded_retry() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    let unit = workflow.unit("inspect-source").unwrap().reference.clone();
    let grant = RetryGrant::new("grant:inspect", 2).unwrap();
    let disposition = disposition(&fixture.run, &workflow, "inspect-source", Some(&grant));
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:first".into(),
            task_ref: "task:inspect".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: unit.clone(),
            disposition: disposition.clone(),
            retry_grant: Some(grant.clone()),
            tracking: Vec::new(),
            place_grant: None,
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::BindDispatch {
            attempt_ref: "attempt:first".into(),
            execution_ref: "execution:first-owner".into(),
            receipt: owner_receipt(
                "aikit/session-space",
                "delivery:first",
                "aikit-delivery:first",
                OwnerOperationPhase::Uncertain,
                set(["evidence:lost-ack"]),
                set(["effect:possible-provider-turn"]),
            ),
        })
        .unwrap();

    let fail_while_uncertain = fixture.action(FactoryAttemptOperation::Fail {
        attempt_ref: "attempt:first".into(),
        reason: "transport outcome unresolved".into(),
        evidence_refs: set(["effect:possible-provider-turn"]),
    });
    assert!(fail_while_uncertain.is_err());
    let still_uncertain = fixture.reading();
    assert_eq!(still_uncertain.legs[&unit].status, LegStatus::Active);
    assert!(still_uncertain.attempts[0].failure_recorded_at.is_none());
    assert!(still_uncertain.attempts[0]
        .dispatch
        .as_ref()
        .unwrap()
        .partial_effect_refs
        .contains("effect:possible-provider-turn"));

    fixture
        .action(FactoryAttemptOperation::RecordObservation {
            attempt_ref: "attempt:first".into(),
            receipt: owner_receipt(
                "aikit/session-space",
                "delivery:first",
                "aikit-reconcile:first",
                OwnerOperationPhase::ReconciledNoReplay,
                set(["evidence:operator-native-reconcile"]),
                BTreeSet::new(),
            ),
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::Fail {
            attempt_ref: "attempt:first".into(),
            reason: "reconciled as no replay; attempt failed".into(),
            evidence_refs: set([
                "evidence:operator-native-reconcile",
                "effect:possible-provider-turn",
            ]),
        })
        .unwrap();

    fixture
        .action(FactoryAttemptOperation::Retry {
            attempt_ref: "attempt:second".into(),
            task_ref: "task:inspect".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: unit.clone(),
            grant_ref: grant.grant_ref.clone(),
            disposition: disposition.clone(),
            tracking: Vec::new(),
            reresolution: Some(ReresolutionRecord {
                resolution_ref: "resolution:after-uncertain".into(),
                reason: "owner delivery reconciled without replay".into(),
                source_revision: disposition.participant.source_revision.clone(),
                evidence_refs: set([
                    "evidence:operator-native-reconcile",
                    "effect:possible-provider-turn",
                ]),
                replacement_now_ref: None,
                replacement_material_ref: None,
                replacement_harness_ref: None,
            }),
            place_grant: None,
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::Fail {
            attempt_ref: "attempt:second".into(),
            reason: "controlled second failure".into(),
            evidence_refs: set(["evidence:second-failure"]),
        })
        .unwrap();

    let third = fixture.action(FactoryAttemptOperation::Retry {
        attempt_ref: "attempt:third".into(),
        task_ref: "task:inspect".into(),
        parent_journey_ref: "journey:221".into(),
        workflow_unit_ref: unit.clone(),
        grant_ref: grant.grant_ref,
        disposition,
        tracking: Vec::new(),
        reresolution: None,
        place_grant: None,
    });
    assert!(third.is_err());
    let reading = fixture.reading();
    assert_eq!(reading.attempts.len(), 2);
    assert_eq!(reading.legs[&unit].status, LegStatus::Failed);
    for attempt in &reading.attempts {
        let recorded = attempt
            .failure_recorded_at
            .as_deref()
            .expect("native failure admission time");
        chrono::DateTime::parse_from_rfc3339(recorded).expect("persisted RFC3339 time");
    }
    assert!(reading.attempts[0]
        .failure_evidence_refs
        .contains("effect:possible-provider-turn"));
    let policy_path = fixture._dir.path().join("retry-policy.json");
    std::fs::write(
        &policy_path,
        serde_json::to_vec(&json!({
            "schema":"factory.sensing-policy/v1","version":1,"project_world_ref":PROJECT,
            "sources":[{"id":"attempts","provider":"factory","scope":PROJECT,
                        "source_ref":"factory:attempts:retry","arguments":{"kind":"attempts"}}],
            "workflows":{"collect":{"enabled":true,"sources":["attempts"]}}
        }))
        .unwrap(),
    )
    .unwrap();
    let output = run_factory(
        &[
            "telemetry".into(),
            "collect".into(),
            fixture.state.display().to_string(),
            "--policy".into(),
            policy_path.display().to_string(),
            "--json".into(),
        ],
        None,
    );
    assert_success(&output);
    let collected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut found_retry = false;
    for signal_ref in collected["collection"]["signal_refs"].as_array().unwrap() {
        let output = run_factory(
            &[
                "telemetry".into(),
                "signal".into(),
                fixture.state.display().to_string(),
                signal_ref.as_str().unwrap().into(),
                "--json".into(),
            ],
            None,
        );
        assert_success(&output);
        let signal: Value = serde_json::from_slice(&output.stdout).unwrap();
        if signal["signal"]["observation"]["source_ref"] == "factory:attempt-retry:attempt:second" {
            found_retry = true;
            let refs = signal["signal"]["observation"]["relation_refs"]
                .as_array()
                .unwrap();
            assert!(refs.iter().any(|reference| reference == "attempt:first"));
            assert!(refs.iter().any(|reference| reference == "attempt:second"));
            assert!(refs.iter().any(|reference| reference == "grant:inspect"));
        }
    }
    assert!(
        found_retry,
        "native retry must be a source-qualified observation"
    );
}

#[test]
fn public_cli_enforces_fork_barrier_and_shared_writer_rules_atomically() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    let inspect = workflow.unit("inspect-source").unwrap().reference.clone();
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:inspect".into(),
            task_ref: "task:inspect".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: inspect,
            disposition: disposition(&fixture.run, &workflow, "inspect-source", None),
            retry_grant: None,
            tracking: Vec::new(),
            place_grant: None,
        })
        .unwrap();
    complete(
        &fixture,
        &workflow,
        "attempt:inspect",
        "inspect-source",
        "execution:inspect",
    );

    let implement = start(
        &fixture,
        &workflow,
        "implement-compiler",
        "attempt:implement",
    );
    let review = start(
        &fixture,
        &workflow,
        "review-adversarially",
        "attempt:review",
    );
    fixture
        .action(FactoryAttemptOperation::StartFork {
            parent_journey_ref: "journey:221".into(),
            attempts: vec![implement, review],
        })
        .unwrap();

    let integrate = workflow.unit("integrate-return").unwrap().reference.clone();
    let blocked = fixture.action(FactoryAttemptOperation::StartSerial {
        attempt_ref: "attempt:integrate-too-early".into(),
        task_ref: "task:integrate".into(),
        parent_journey_ref: "journey:221".into(),
        workflow_unit_ref: integrate.clone(),
        disposition: disposition(&fixture.run, &workflow, "integrate-return", None),
        retry_grant: None,
        tracking: Vec::new(),
        place_grant: None,
    });
    assert!(blocked.is_err());

    complete(
        &fixture,
        &workflow,
        "attempt:implement",
        "implement-compiler",
        "execution:implement",
    );
    assert!(fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:integrate-still-early".into(),
            task_ref: "task:integrate".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: integrate.clone(),
            disposition: disposition(&fixture.run, &workflow, "integrate-return", None),
            retry_grant: None,
            tracking: Vec::new(),
            place_grant: None,
        })
        .is_err());
    complete(
        &fixture,
        &workflow,
        "attempt:review",
        "review-adversarially",
        "execution:review",
    );
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:integrate".into(),
            task_ref: "task:integrate".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: integrate,
            disposition: disposition(&fixture.run, &workflow, "integrate-return", None),
            retry_grant: None,
            tracking: Vec::new(),
            place_grant: None,
        })
        .unwrap();

    // Rebuild the same authored graph with the independent review also writing
    // the shared subject. The public fork must fail before either child is stored.
    let mut conflicting = source();
    conflicting.source.revision = "source-revision-writer-conflict".into();
    conflicting
        .units
        .iter_mut()
        .find(|unit| unit.key == "review-adversarially")
        .unwrap()
        .permitted_effects = vec!["write adversarial review".into()];
    conflicting.source.digest = workflow_source_digest(&conflicting).unwrap();
    let conflict = Fixture::new(conflicting);
    let conflict_workflow = conflict.workflow();
    let inspect = conflict_workflow
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    conflict
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:inspect".into(),
            task_ref: "task:inspect".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: inspect,
            disposition: disposition(&conflict.run, &conflict_workflow, "inspect-source", None),
            retry_grant: None,
            tracking: Vec::new(),
            place_grant: None,
        })
        .unwrap();
    complete(
        &conflict,
        &conflict_workflow,
        "attempt:inspect",
        "inspect-source",
        "execution:inspect",
    );
    let before = conflict.reading();
    let result = conflict.action(FactoryAttemptOperation::StartFork {
        parent_journey_ref: "journey:221".into(),
        attempts: vec![
            start(
                &conflict,
                &conflict_workflow,
                "implement-compiler",
                "attempt:writer-a",
            ),
            start(
                &conflict,
                &conflict_workflow,
                "review-adversarially",
                "attempt:writer-b",
            ),
        ],
    });
    assert!(result.is_err());
    let after = conflict.reading();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.attempts.len(), before.attempts.len());
}

#[test]
fn provider_return_requires_factory_verification_and_historical_late_return_stays_historical() {
    let fixture = Fixture::new(source());
    let workflow = fixture.workflow();
    let unit = workflow.unit("inspect-source").unwrap().reference.clone();
    let grant = RetryGrant::new("grant:late", 2).unwrap();
    let disposition = disposition(&fixture.run, &workflow, "inspect-source", Some(&grant));
    fixture
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:old".into(),
            task_ref: "task:inspect".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: unit.clone(),
            disposition: disposition.clone(),
            retry_grant: Some(grant.clone()),
            tracking: Vec::new(),
            place_grant: None,
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::BindDispatch {
            attempt_ref: "attempt:old".into(),
            execution_ref: "execution:old".into(),
            receipt: owner_receipt(
                "aikit/session-space",
                "delivery:old",
                "aikit-delivery:old",
                OwnerOperationPhase::Returned,
                set(["evidence:provider-turn-ended"]),
                BTreeSet::new(),
            ),
        })
        .unwrap();

    let no_verification = fixture.action(return_operation(
        &workflow,
        "attempt:old",
        "inspect-source",
        "execution:old",
        "artifact:old-premature",
    ));
    assert!(no_verification.is_err());
    assert_eq!(fixture.reading().legs[&unit].status, LegStatus::Active);

    fixture
        .action(FactoryAttemptOperation::Fail {
            attempt_ref: "attempt:old".into(),
            reason: "Factory verification did not pass".into(),
            evidence_refs: set(["evidence:verification-failed"]),
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::Retry {
            attempt_ref: "attempt:new".into(),
            task_ref: "task:inspect".into(),
            parent_journey_ref: "journey:221".into(),
            workflow_unit_ref: unit.clone(),
            grant_ref: grant.grant_ref,
            disposition,
            tracking: Vec::new(),
            reresolution: None,
            place_grant: None,
        })
        .unwrap();

    // A real provider completion for the old execution can arrive after the
    // retry. It remains attached to that historical execution and cannot satisfy
    // the current attempt or barrier.
    fixture
        .action(FactoryAttemptOperation::RecordVerification {
            attempt_ref: "attempt:old".into(),
            verification: verification(&workflow, "inspect-source", "verification:late-old"),
        })
        .unwrap();
    fixture
        .action(return_operation(
            &workflow,
            "attempt:old",
            "inspect-source",
            "execution:old",
            "artifact:late-old",
        ))
        .unwrap();
    let reading = fixture.reading();
    assert_eq!(reading.legs[&unit].status, LegStatus::Active);
    assert_eq!(
        reading.legs[&unit].execution_ref,
        "factory-attempt:attempt:new"
    );
    let old = reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == "attempt:old")
        .unwrap();
    let current = reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == "attempt:new")
        .unwrap();
    assert_eq!(
        old.readable_return.as_ref().unwrap().return_ref,
        "return:artifact:late-old"
    );
    assert!(current.readable_return.is_none());
    assert_eq!(reading.legs[&unit].attempts[0].late_artifacts.len(), 1);
}

struct Fixture {
    _dir: TempDir,
    state: std::path::PathBuf,
    run: Run,
    source: WorkflowSource,
}

impl Fixture {
    fn new(source: WorkflowSource) -> Self {
        Self::with_seed_lifecycle(source, vec![])
    }

    fn with_seed_lifecycle(
        source: WorkflowSource,
        lifecycle: Vec<epilogos_factory::core::run::RunLifecycle>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("attempt-state.json");
        let mut run = Run::new(
            RUN.parse::<RunRef>().unwrap(),
            PROJECT.parse().unwrap(),
            "public native attempt tests",
            "factory-attempt-public-test",
        )
        .unwrap();
        for (index, target) in lifecycle.into_iter().enumerate() {
            run.apply_lifecycle_command(
                &run.mutation_authority(),
                epilogos_factory::core::run::RunLifecycleCommand {
                    command_id: format!("imported-core-lifecycle-{index}"),
                    expected_revision: run.revision(),
                    lifecycle: target,
                },
            )
            .unwrap();
        }
        let seed = FactoryAttemptSeed {
            run: run.clone(),
            workflow_source: source.clone(),
        };
        let output = run_factory(
            &[
                "attempt".into(),
                "init".into(),
                state.display().to_string(),
                "-".into(),
                "--json".into(),
            ],
            Some(&serde_json::to_string(&seed).unwrap()),
        );
        assert_success(&output);
        Self {
            _dir: dir,
            state,
            run,
            source,
        }
    }

    fn workflow(&self) -> CompiledWorkflow {
        compile_workflow(self.source.clone()).unwrap()
    }

    fn reading(&self) -> FactoryAttemptReading {
        let output = run_factory(
            &[
                "attempt".into(),
                "read".into(),
                self.state.display().to_string(),
                "--json".into(),
            ],
            None,
        );
        assert_success(&output);
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn action(&self, operation: FactoryAttemptOperation) -> Result<Value, String> {
        let reading = self.reading();
        let request = request(self.run.reference().clone(), reading.revision, operation);
        let output = self.raw_action(&request);
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
    }

    fn raw_action(&self, request: &FactoryAttemptActionRequest) -> Output {
        run_factory(
            &[
                "attempt".into(),
                "action".into(),
                self.state.display().to_string(),
                "-".into(),
                "--json".into(),
            ],
            Some(&serde_json::to_string(request).unwrap()),
        )
    }
}

fn run_factory(args: &[String], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "factory command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn source() -> WorkflowSource {
    serde_json::from_str(SOURCE).unwrap()
}

fn request(
    run_ref: RunRef,
    expected_revision: u64,
    operation: FactoryAttemptOperation,
) -> FactoryAttemptActionRequest {
    FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: "projection:attempt-public-test".into(),
        caller: FactoryActionCaller {
            caller_ref: "agent:attempt-public-test".into(),
            projection_kind: FactoryActionProjectionKind::Headless,
            lineage: vec![
                "agency:attempt-public-test".into(),
                "agent:attempt-public-test".into(),
            ],
        },
        run_ref,
        expected_revision,
        authority: ProjectedFactoryActionAuthority {
            authority_ref: "authority:attempt-public-test".into(),
            native_owner: "factory".into(),
            capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
            capability_granted: true,
            action_authorised: true,
        },
        operation,
    }
}

fn start(
    fixture: &Fixture,
    workflow: &CompiledWorkflow,
    key: &str,
    attempt_ref: &str,
) -> AttemptStart {
    AttemptStart {
        attempt_ref: attempt_ref.into(),
        task_ref: format!("task:{key}"),
        workflow_unit_ref: workflow.unit(key).unwrap().reference.clone(),
        disposition: disposition(&fixture.run, workflow, key, None),
        retry_grant: None,
        tracking: Vec::new(),
        place_grant: None,
    }
}

fn disposition(
    run: &Run,
    workflow: &CompiledWorkflow,
    key: &str,
    retry: Option<&RetryGrant>,
) -> SituatedExecutionDisposition {
    let unit = workflow.unit(key).unwrap();
    let agency_ref = unit
        .agent_requirements
        .agency_refs
        .iter()
        .next()
        .cloned()
        .unwrap_or_else(|| "agency:controlled-test".into());
    let demand = ExecutionDemand {
        project_ref: run.project_ref().to_string(),
        run_ref: run.reference().to_string(),
        workflow_unit_ref: Some(unit.reference.to_string()),
        agency_ref: Some(agency_ref.clone()),
        profile_ref: None,
        use_type: "controlled-native-test".into(),
        required_capabilities: unit.capability_refs.clone(),
        required_modalities: BTreeSet::from(["text".into()]),
        required_actions: BTreeSet::new(),
        required_tools: BTreeSet::new(),
        context_characteristics: BTreeSet::from(["public-command-path".into()]),
        independence_from: unit
            .independence_from
            .iter()
            .map(ToString::to_string)
            .collect(),
        cost_ceiling_usd: None,
        latency_preference_ms: None,
        requires_local_materialisation: unit
            .permitted_effects
            .iter()
            .any(|effect| effect.to_ascii_lowercase().contains("write ")),
    };
    let selection = AikitModelRosterSelection {
        roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
        model_ref: "model:controlled-test".into(),
        provider_ref: "provider:controlled-test".into(),
        ranking_policy: "controlled-test".into(),
        ranking_explanation: json!({"testOnly": true}),
        provenance: vec!["selection:controlled-test".into()],
    };
    let selection = accept_aikit_selection(demand, selection, "2026-09-10T20:00:00+01:00").unwrap();
    let placement = PlacementProtection {
        now_ref: "now:factory-221".into(),
        now_path: "/controlled/project/NOW".into(),
        policy_ref: "placement-policy:controlled".into(),
        policy_revision: "central-150-pr".into(),
        authority_ref: "authority:central-placement".into(),
        writable_paths: BTreeSet::from(["factory/src".into(), "factory/tests".into()]),
        protected_paths: BTreeSet::from(["docs/positions".into()]),
        required_coverage: BTreeSet::from(["source-protection".into(), "single-writer".into()]),
        effective_coverage: BTreeSet::from(["source-protection".into(), "single-writer".into()]),
        write_boundary_ref: Some("workcell-write-boundary:controlled".into()),
        material_receipt_ref: Some("workcell-world:controlled".into()),
    };
    SituatedExecutionDisposition {
        selected_inputs: Vec::new(),
        selection,
        participant: SituatedParticipant {
            agent_ref: unit
                .agent_requirements
                .agent_refs
                .iter()
                .next()
                .cloned()
                .unwrap_or_else(|| "agent:controlled-test".into()),
            agency_ref,
            world_binding_ref: "world-binding:controlled".into(),
            profile_ref: None,
            position_ref: None,
            source_ref: workflow.source.reference.to_string(),
            source_revision: workflow.source.revision.clone(),
            source_digest: format!("blake3:{}", workflow.source.digest),
        },
        context_refs: BTreeSet::from(["context:controlled".into()]),
        praxis_refs: unit.praxis_refs.clone(),
        capability_refs: unit.capability_refs.clone(),
        body: ExecutionBody {
            model_ref: "model:controlled-test".into(),
            provider_ref: "provider:controlled-test".into(),
            route_ref: "route:controlled".into(),
            harness_ref: "harness:controlled".into(),
            harness_composition_ref: "harness-composition:controlled".into(),
            agent_session_ref: format!("agent-session:{key}"),
            session_space_ref: "session-space:controlled".into(),
            material_world_ref: Some("world:controlled".into()),
            workcell_ref: Some("workcell:controlled".into()),
        },
        placement: Some(placement),
        permitted_effects: unit.permitted_effects.clone(),
        verification_obligations: unit.verification_obligations.clone(),
        return_address: unit.return_address.clone(),
        stop_conditions: unit.stop_conditions.clone(),
        escalation_conditions: unit.escalation_conditions.clone(),
        budget: ExecutionBudget {
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            wall_clock_timeout_ms: Some(120_000),
            retry_grant_ref: retry.map(|grant| grant.grant_ref.clone()),
            maximum_attempts: retry.map(|grant| grant.attempts_allowed),
        },
    }
}

fn complete(
    fixture: &Fixture,
    workflow: &CompiledWorkflow,
    attempt_ref: &str,
    key: &str,
    execution_ref: &str,
) {
    let operation_ref = format!("delivery:{attempt_ref}");
    fixture
        .action(FactoryAttemptOperation::BindDispatch {
            attempt_ref: attempt_ref.into(),
            execution_ref: execution_ref.into(),
            receipt: owner_receipt(
                "aikit/session-space",
                &operation_ref,
                &format!("receipt:{attempt_ref}"),
                OwnerOperationPhase::Returned,
                set(["evidence:provider-turn-ended"]),
                BTreeSet::new(),
            ),
        })
        .unwrap();
    fixture
        .action(FactoryAttemptOperation::RecordVerification {
            attempt_ref: attempt_ref.into(),
            verification: verification(workflow, key, &format!("verification:{attempt_ref}")),
        })
        .unwrap();
    fixture
        .action(return_operation(
            workflow,
            attempt_ref,
            key,
            execution_ref,
            &format!("artifact:{attempt_ref}"),
        ))
        .unwrap();
}

fn verification(
    workflow: &CompiledWorkflow,
    key: &str,
    verification_ref: &str,
) -> VerificationReceipt {
    VerificationReceipt {
        verification_ref: verification_ref.into(),
        owner_ref: "factory/verification".into(),
        source_revision: workflow.source.revision.clone(),
        outcome: VerificationOutcome::Passed,
        obligations: workflow.unit(key).unwrap().verification_obligations.clone(),
        evidence_refs: set([format!("evidence:{verification_ref}")]),
    }
}

fn return_operation(
    workflow: &CompiledWorkflow,
    attempt_ref: &str,
    key: &str,
    execution_ref: &str,
    artifact_ref: &str,
) -> FactoryAttemptOperation {
    let unit = workflow.unit(key).unwrap();
    let evidence = set([format!("evidence:{artifact_ref}")]);
    FactoryAttemptOperation::ReturnArtifact {
        attempt_ref: attempt_ref.into(),
        artifact: ReturnedArtifact {
            artifact_ref: artifact_ref.into(),
            subject_ref: unit.subject_ref.to_string(),
            subject_revision: unit.basis_revision.clone(),
            producing_execution_ref: execution_ref.into(),
            evidence_refs: evidence.clone(),
            semantic_difference: format!("controlled difference for {artifact_ref}"),
        },
        readable_return: ReadableReturn {
            return_ref: format!("return:{artifact_ref}"),
            summary: format!("controlled readable Return for {artifact_ref}"),
            artifact_refs: set([artifact_ref.to_string()]),
            evidence_refs: evidence,
            receiving_ref: None,
            receiving_source_revision: None,
            archive_refs: BTreeSet::new(),
            regression_observation_refs: BTreeSet::new(),
        },
    }
}

fn owner_receipt(
    owner_ref: &str,
    operation_ref: &str,
    receipt_ref: &str,
    phase: OwnerOperationPhase,
    evidence_refs: BTreeSet<String>,
    partial_effect_refs: BTreeSet<String>,
) -> OwnerOperationReceipt {
    OwnerOperationReceipt {
        owner_ref: owner_ref.into(),
        contract: "aikit.encounter-delivery/v1".into(),
        operation_ref: operation_ref.into(),
        receipt_ref: receipt_ref.into(),
        source_revision: "3d23d1eefbb999b0a5058ed0b4ca98dd2575b632".into(),
        phase,
        evidence_refs,
        partial_effect_refs,
        payload: json!({"controlledTest": true, "phase": format!("{phase:?}")}),
    }
}

fn fact(
    fact_ref: &str,
    kind: &str,
    owner_ref: &str,
    subject_ref: &str,
    source_revision: &str,
) -> AttemptTrackingFact {
    AttemptTrackingFact {
        fact_ref: fact_ref.into(),
        kind: kind.into(),
        owner_ref: owner_ref.into(),
        subject_ref: subject_ref.into(),
        source_revision: source_revision.into(),
        evidence_refs: set([format!("evidence:{fact_ref}")]),
    }
}

fn set<const N: usize>(values: [impl Into<String>; N]) -> BTreeSet<String> {
    values.into_iter().map(Into::into).collect()
}
