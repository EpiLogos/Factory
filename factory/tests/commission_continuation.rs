//! Exercise real native provider transactions and restart readback. These cases
//! commission work; they never manufacture an executed or accepted worker result.

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_native_store::FileAttemptStore;
use epilogos_factory::attempt_runtime::{
    ExecutionBody, ExecutionBudget, FactoryAttemptActionRequest, FactoryAttemptOperation,
    OwnerOperationPhase, OwnerOperationReceipt, ReadableReturn, SituatedExecutionDisposition,
    SituatedParticipant, VerificationOutcome, VerificationReceipt, FACTORY_ATTEMPT_ACTION,
    FACTORY_ATTEMPT_CAPABILITY_REF,
};
use epilogos_factory::commission::{
    BoundedRootAct, CommissionContinuationRelation, FactoryAdmissionStatus,
    FactoryCommissionRequest, FactoryDevelopmentalMutation, FactoryDevelopmentalMutationRequest,
    FactoryMutationSource, FactoryParticipantRequirement, FACTORY_COMMISSION_REQUEST,
    FACTORY_DEVELOPMENTAL_MUTATION_REQUEST,
};
use epilogos_factory::core::identity::Revision;
use epilogos_factory::core::run::{RunLifecycle, RunLifecycleCommand, RunRef};
use epilogos_factory::developmental_read::{
    FactoryDevelopmentalFileProvider, FactoryDevelopmentalState,
};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{LegStatus, ReturnedArtifact, WholeRunState};
use epilogos_factory::project_development_store::read_developmental_state;
use epilogos_factory::run_lifecycle::{
    UnitDecisionBasis, UnitDecisionOutcome, UnitDecisionRequest, UnitDecisionResponse,
};
use epilogos_factory::workflow::{compile_workflow, workflow_source_digest, WorkflowSource};
use epilogos_factory::workflow_authoring::load_workflow;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
// Exact one-Run extraction of the retained pre-guard owner document. It keeps
// the original source, compiled map, Commission, Journey, custody and empty
// coordinator; it does not manufacture historical admission or worker success.
const HISTORICAL_OWNER: &str =
    include_str!("../../contracts/factory/fixtures/plural-flow-historical-owner.json");
const COMMISSION: &str = "commission:continuation-owner-regression";
const SUCCESSOR: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBE";

struct World {
    _directory: tempfile::TempDir,
    path: PathBuf,
    original: WorkflowSource,
    predecessor: RunRef,
    commission_ref: String,
}

fn request(
    id: &str,
    source: &WorkflowSource,
    mutation: FactoryDevelopmentalMutation,
) -> FactoryDevelopmentalMutationRequest {
    FactoryDevelopmentalMutationRequest {
        contract: FACTORY_DEVELOPMENTAL_MUTATION_REQUEST.into(),
        mutation_ref: format!("mutation:{id}"),
        occurrence_ref: format!("occurrence:{id}"),
        source: FactoryMutationSource {
            owner: "factory".into(),
            reference: source.source.reference.to_string(),
            revision: source.source.revision.clone(),
            standing: "owner-native-observation".into(),
        },
        observed_at: "2026-10-01T00:10:00Z".into(),
        mutation,
    }
}

impl World {
    fn new() -> Self {
        Self::with_attempts(true)
    }

    fn with_attempts(attach: bool) -> Self {
        Self::with_source(serde_json::from_str(SOURCE).unwrap(), attach)
    }

    fn with_source(original: WorkflowSource, attach: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("development.json");
        let commissioned = FactoryDevelopmentalFileProvider::commission(
            &path,
            FactoryCommissionRequest {
                contract: FACTORY_COMMISSION_REQUEST.into(),
                request_ref: COMMISSION.into(),
                project_key: "continuation-native-provider".into(),
                purpose: "Retain a bounded undertaking across a corrected source candidate".into(),
                frontier: "Commissioned work has no execution evidence".into(),
                run_destination: "same bounded development concern".into(),
                write_owner: "factory".into(),
                commissioned_at: "2026-09-30T23:50:00Z".into(),
                central_composition: None,
                participant_requirements: vec![FactoryParticipantRequirement {
                    reference: "agent:hermes".into(),
                    description: "Bounded native repair participant".into(),
                    source_owner: "personal-owner".into(),
                    source_ref: "source:controlled-participant".into(),
                    source_revision: "controlled-requirement-v1".into(),
                }],
                root_act: BoundedRootAct {
                    act_ref: "act:continuation-regression".into(),
                    agent_ref: "agent:hermes".into(),
                    purpose: "Continue this commissioned concern only".into(),
                    scope_refs: vec!["issue:continuation-regression".into()],
                    standing: "commissioned-not-executed".into(),
                },
            },
        )
        .unwrap();
        let predecessor = commissioned.commission.run_ref;
        FactoryDevelopmentalFileProvider::open(&path)
            .unwrap()
            .apply_developmental_mutation(request(
                "original-source",
                &original,
                FactoryDevelopmentalMutation::AttachWorkflowSource {
                    run_ref: predecessor.clone(),
                    workflow_source: original.clone(),
                },
            ))
            .unwrap();
        // The original packet is actually attached to its canonical Run. No leg
        // is claimed executed, and continuation must retain this owner field.
        if attach {
            FileAttemptStore::attach(
                &path,
                predecessor.clone(),
                &original.source.reference.to_string(),
            )
            .unwrap();
        }
        Self {
            _directory: directory,
            path,
            original,
            predecessor,
            commission_ref: COMMISSION.into(),
        }
    }

    fn restore_historical_owner() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("development.json");
        std::fs::write(&path, HISTORICAL_OWNER).unwrap();
        let provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
        let state = provider.state();
        let original = state.workflow_sources[0].clone();
        let commission = &state.commissions[0];
        Self {
            _directory: directory,
            path,
            original,
            predecessor: commission.run_ref.clone(),
            commission_ref: commission.request.request_ref.clone(),
        }
    }

    fn state(&self) -> FactoryDevelopmentalState {
        read_developmental_state(&self.path).unwrap()
    }

    fn action_request(
        &self,
        run: &RunRef,
        operation: FactoryAttemptOperation,
    ) -> FactoryAttemptActionRequest {
        let reading = FileAttemptStore::open_run(&self.path, run.clone())
            .unwrap()
            .reading()
            .unwrap();
        FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: "projection:continuation-native-regression".into(),
            run_ref: run.clone(),
            expected_revision: reading.revision,
            caller: FactoryActionCaller {
                caller_ref: "agent:controlled-coordinator".into(),
                projection_kind: FactoryActionProjectionKind::Headless,
                lineage: vec![
                    "agency:controlled-coordinator".into(),
                    "agent:controlled-coordinator".into(),
                ],
            },
            authority: ProjectedFactoryActionAuthority {
                authority_ref: "authority:controlled-native-attempt".into(),
                native_owner: "factory".into(),
                capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
            operation,
        }
    }

    fn action(&self, run: &RunRef, operation: FactoryAttemptOperation) -> Result<(), String> {
        let request = self.action_request(run, operation);
        FileAttemptStore::open_run(&self.path, run.clone())
            .unwrap()
            .apply(request)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn transition(
        &self,
        run: &RunRef,
        command_id: &str,
        lifecycle: RunLifecycle,
    ) -> Result<(), String> {
        let state = self.state();
        let current = state.build.run(run).unwrap();
        self.action(
            run,
            FactoryAttemptOperation::TransitionRun {
                command: RunLifecycleCommand {
                    command_id: command_id.into(),
                    expected_revision: current.revision(),
                    lifecycle,
                },
                authority: current.mutation_authority(),
                closure: None,
            },
        )
    }

    fn continuation(&self, id: &str, successor: &str) -> FactoryDevelopmentalMutationRequest {
        let state = self.state();
        let commission = state
            .commissions
            .iter()
            .find(|item| item.request.request_ref == self.commission_ref)
            .unwrap();
        let journey = state
            .journeys
            .iter()
            .find(|item| item.journey_ref == commission.journey_ref)
            .unwrap();
        let mut source = self.original.clone();
        source.source.reference = "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FAX"
            .parse()
            .unwrap();
        source.source.revision = "corrected-source-v2".into();
        for unit in &mut source.units {
            unit.basis_revision = "coherent-candidate-manifest-v2".into();
        }
        source.source.digest = workflow_source_digest(&source).unwrap();
        request(
            id,
            &source,
            FactoryDevelopmentalMutation::ContinueCommission {
                continuation_relation: CommissionContinuationRelation::SupersedingSource,
                commission_ref: self.commission_ref.clone(),
                journey_ref: commission.journey_ref.clone(),
                predecessor_run_ref: self.predecessor.clone(),
                expected_journey_revision: journey.revision,
                expected_predecessor_run_revision: state
                    .build
                    .run(&self.predecessor)
                    .unwrap()
                    .revision(),
                predecessor_source_ref: self.original.source.reference.clone(),
                predecessor_source_revision: self.original.source.revision.clone(),
                predecessor_source_digest: self.original.source.digest.clone(),
                successor_run_ref: successor.parse().unwrap(),
                workflow_source: Box::new(source.clone()),
                reason: "Explicit corrected source; retain the first candidate and its receipts"
                    .into(),
                basis_refs: vec!["source:governing-commission".into()],
            },
        )
    }

    fn bounded_contribution(&self, id: &str) -> FactoryDevelopmentalMutationRequest {
        let mut continuation = self.continuation(id, SUCCESSOR);
        let FactoryDevelopmentalMutation::ContinueCommission {
            continuation_relation,
            workflow_source,
            reason,
            ..
        } = &mut continuation.mutation
        else {
            unreachable!()
        };
        *continuation_relation = CommissionContinuationRelation::BoundedContribution;
        workflow_source.units.truncate(1);
        workflow_source.units[0].dependencies.clear();
        workflow_source.units[0].independence_from.clear();
        workflow_source.units[0].inputs.clear();
        workflow_source.barriers.clear();
        workflow_source.source.digest = workflow_source_digest(workflow_source).unwrap();
        *reason = "Bounded contribution only; preserve broader unfinished work and current predecessor admission".into();
        continuation
    }
}

fn read_only_disposition(
    world: &World,
    run: &RunRef,
    source: &WorkflowSource,
) -> SituatedExecutionDisposition {
    let compiled = compile_workflow(source.clone()).unwrap();
    let unit = compiled.unit("inspect-source").unwrap();
    let selected_run = world.state().build.run(run).unwrap().clone();
    let agency = format!("agency:controlled-read-only-{run}");
    let selection = accept_aikit_selection(
        ExecutionDemand {
            project_ref: selected_run.project_ref().to_string(),
            run_ref: run.to_string(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some(agency.clone()),
            profile_ref: None,
            use_type: "controlled-native-continuation-regression".into(),
            required_capabilities: unit.capability_refs.clone(),
            required_modalities: BTreeSet::from(["text".into()]),
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
            model_ref: "model:controlled-owner-regression".into(),
            provider_ref: "provider:controlled-owner-regression".into(),
            ranking_policy: "controlled-contract-regression".into(),
            ranking_explanation: serde_json::json!({"controlled":true}),
            provenance: vec!["selection:controlled-source-cleanup".into()],
        },
        "2026-10-01T01:30:00Z",
    )
    .unwrap();
    SituatedExecutionDisposition {
        selected_inputs: vec![],
        selection,
        participant: SituatedParticipant {
            agent_ref: unit
                .agent_requirements
                .agent_refs
                .iter()
                .next()
                .unwrap()
                .clone(),
            agency_ref: agency,
            world_binding_ref: "world-binding:controlled-source-cleanup".into(),
            profile_ref: None,
            position_ref: None,
            source_ref: compiled.source.reference.to_string(),
            source_revision: compiled.source.revision.clone(),
            source_digest: format!("blake3:{}", compiled.source.digest),
        },
        context_refs: BTreeSet::from(["context:controlled-source-cleanup".into()]),
        praxis_refs: unit.praxis_refs.clone(),
        capability_refs: unit.capability_refs.clone(),
        body: ExecutionBody {
            model_ref: "model:controlled-owner-regression".into(),
            provider_ref: "provider:controlled-owner-regression".into(),
            route_ref: "route:controlled-source-cleanup".into(),
            harness_ref: "harness:controlled-source-cleanup".into(),
            harness_composition_ref: "composition:controlled-source-cleanup".into(),
            agent_session_ref: format!("agent-session:controlled-{run}"),
            session_space_ref: format!("session-space:controlled-{run}"),
            material_world_ref: None,
            workcell_ref: None,
        },
        placement: None,
        permitted_effects: unit.permitted_effects.clone(),
        verification_obligations: unit.verification_obligations.clone(),
        return_address: unit.return_address.clone(),
        stop_conditions: unit.stop_conditions.clone(),
        escalation_conditions: unit.escalation_conditions.clone(),
        budget: ExecutionBudget {
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            wall_clock_timeout_ms: Some(30_000),
            retry_grant_ref: None,
            maximum_attempts: Some(1),
        },
    }
}

fn worker_receipt(phase: OwnerOperationPhase) -> OwnerOperationReceipt {
    OwnerOperationReceipt {
        owner_ref: "aikit/session-space".into(),
        contract: "aikit.controlled-native-return/v1".into(),
        operation_ref: "delivery:controlled-source-cleanup".into(),
        receipt_ref: format!("receipt:controlled-{phase:?}"),
        source_revision: "controlled-journal-cursor:12".into(),
        phase,
        evidence_refs: BTreeSet::from(["evidence:controlled-owner-return".into()]),
        partial_effect_refs: BTreeSet::new(),
        payload: serde_json::json!({"controlled":true}),
    }
}

fn start_predecessor_read_only(world: &World, returned: bool) {
    world
        .transition(
            &world.predecessor,
            "bounded-parent-active",
            RunLifecycle::Active,
        )
        .unwrap();
    let workflow = compile_workflow(world.original.clone()).unwrap();
    let unit = workflow.unit("inspect-source").unwrap();
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::StartSerial {
                attempt_ref: "attempt:bounded-parent".into(),
                task_ref: "task:bounded-parent".into(),
                parent_journey_ref: world.state().journeys[0].journey_ref.to_string(),
                workflow_unit_ref: unit.reference.clone(),
                disposition: read_only_disposition(world, &world.predecessor, &world.original),
                retry_grant: None,
                tracking: vec![],
                place_grant: None,
            },
        )
        .unwrap();
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::BindDispatch {
                attempt_ref: "attempt:bounded-parent".into(),
                execution_ref: "execution:bounded-parent".into(),
                receipt: worker_receipt(if returned {
                    OwnerOperationPhase::Returned
                } else {
                    OwnerOperationPhase::Submitted
                }),
            },
        )
        .unwrap();
    if returned {
        world
            .action(
                &world.predecessor,
                FactoryAttemptOperation::RecordVerification {
                    attempt_ref: "attempt:bounded-parent".into(),
                    verification: VerificationReceipt {
                        verification_ref: "verification:bounded-parent".into(),
                        owner_ref: "factory/verification".into(),
                        source_revision: world.original.source.revision.clone(),
                        outcome: VerificationOutcome::Passed,
                        obligations: unit.verification_obligations.clone(),
                        evidence_refs: BTreeSet::from(
                            ["evidence:controlled-bounded-parent".into()],
                        ),
                    },
                },
            )
            .unwrap();
    }
}

#[test]
fn bounded_contribution_preserves_current_predecessor_return_and_unfinished_commission() {
    let world = World::new();
    start_predecessor_read_only(&world, true);
    let before = world.state();
    let continuation = world.bounded_contribution("bounded-native-contribution");
    let saved = serde_json::to_value(&continuation).unwrap();
    assert_eq!(
        saved["mutation"]["continuationRelation"],
        "bounded-contribution"
    );
    let applied = FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(continuation.clone())
        .unwrap();
    let bytes = std::fs::read(&world.path).unwrap();
    let replay = FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(continuation)
        .unwrap();
    assert_eq!(applied.record, replay.record);
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    let workflow = compile_workflow(world.original.clone()).unwrap();
    let unit = workflow.unit("inspect-source").unwrap();
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::ReturnArtifact {
                attempt_ref: "attempt:bounded-parent".into(),
                artifact: ReturnedArtifact {
                    artifact_ref: "artifact:bounded-parent".into(),
                    subject_ref: unit.subject_ref.to_string(),
                    subject_revision: unit.basis_revision.clone(),
                    producing_execution_ref: "execution:bounded-parent".into(),
                    evidence_refs: BTreeSet::from(["evidence:controlled-bounded-parent".into()]),
                    semantic_difference:
                        "Current predecessor bytes survive a bounded sibling contribution".into(),
                },
                readable_return: ReadableReturn {
                    return_ref: "return:bounded-parent".into(),
                    summary: "Controlled native owner Return remains current".into(),
                    artifact_refs: BTreeSet::from(["artifact:bounded-parent".into()]),
                    evidence_refs: BTreeSet::from(["evidence:controlled-bounded-parent".into()]),
                    receiving_ref: None,
                    receiving_source_revision: None,
                    archive_refs: BTreeSet::new(),
                    regression_observation_refs: BTreeSet::new(),
                },
            },
        )
        .unwrap();
    let current = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .reading()
        .unwrap();
    assert!(current.source_current);
    assert_eq!(current.legs[&unit.reference].status, LegStatus::Returned);
    assert!(current.legs[&unit.reference].late_artifacts.is_empty());
    assert!(current.current_returned_units.contains(&unit.reference));
    assert_eq!(current.whole_run_state, WholeRunState::Incomplete);
    assert!(!current.completion_verified);
    let after = world.state();
    assert_eq!(after.commissions, before.commissions);
    assert_eq!(after.journeys[0].status, before.journeys[0].status);
    let commission = after.commission_reading(COMMISSION).unwrap();
    assert_eq!(commission.commission.run_ref, world.predecessor);
    assert_eq!(
        commission.continuation_run_refs,
        vec![SUCCESSOR.parse().unwrap()]
    );
    let sibling = after.journeys[0]
        .runs
        .iter()
        .find(|link| link.run_ref.to_string() == SUCCESSOR)
        .unwrap();
    assert!(sibling
        .basis_refs
        .contains(&"factory-continuation-relation:bounded-contribution".into()));
    let mut changed = serde_json::from_value::<FactoryDevelopmentalMutationRequest>(saved).unwrap();
    if let FactoryDevelopmentalMutation::ContinueCommission {
        continuation_relation,
        ..
    } = &mut changed.mutation
    {
        *continuation_relation = CommissionContinuationRelation::SupersedingSource;
    }
    let bytes = std::fs::read(&world.path).unwrap();
    assert!(FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(changed)
        .is_err());
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
}

#[test]
fn bounded_contribution_cannot_retire_a_current_predecessor_human_decision() {
    let world = World::new();
    start_predecessor_read_only(&world, false);
    let workflow = compile_workflow(world.original.clone()).unwrap();
    let unit = workflow.unit("inspect-source").unwrap();
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::RequestUnitDecision {
                request: UnitDecisionRequest {
                    human_request_ref: "human-request:bounded-parent".into(),
                    decision_ref: "decision:bounded-parent".into(),
                    question: "Controlled pending predecessor question".into(),
                    why_human: "Controlled authority boundary regression".into(),
                    basis: UnitDecisionBasis {
                        workflow_unit_ref: unit.reference.clone(),
                        attempt_ref: "attempt:bounded-parent".into(),
                        execution_ref: "execution:bounded-parent".into(),
                        subject_ref: unit.subject_ref.to_string(),
                        subject_revision: unit.basis_revision.clone(),
                        workflow_source_ref: workflow.source.reference.to_string(),
                        workflow_source_revision: workflow.source.revision.clone(),
                        workflow_source_digest: workflow.source.digest.clone(),
                        resolver_ref: "human:controlled-owner".into(),
                        controlled: true,
                    },
                    evidence_refs: BTreeSet::from(["evidence:controlled-pending-parent".into()]),
                },
            },
        )
        .unwrap();
    let before = serde_json::to_value(&world.state().build).unwrap()["humanRequests"].clone();
    let continuation = world.bounded_contribution("bounded-preserve-human-question");
    let replacement = BTreeSet::from([continuation.mutation_ref.clone(), SUCCESSOR.into()]);
    FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(continuation)
        .unwrap();
    let bytes = std::fs::read(&world.path).unwrap();
    assert!(world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::RetireUnitDecision {
                human_request_ref: "human-request:bounded-parent".into(),
                reason: "A sibling cannot retire this still-current basis".into(),
                replacement_basis_refs: replacement,
            }
        )
        .unwrap_err()
        .contains("only an obsolete affected-work basis"));
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    assert_eq!(
        serde_json::to_value(&world.state().build).unwrap()["humanRequests"],
        before
    );
    let current = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .reading()
        .unwrap();
    assert!(current.source_current);
    assert_eq!(current.lifecycle, RunLifecycle::WaitingHuman);
    assert_eq!(current.legs[&unit.reference].status, LegStatus::Active);
}

#[test]
fn bounded_contribution_retains_ambiguous_predecessor_without_admitting_it() {
    let world = World::restore_historical_owner();
    let before = world.state();
    assert_eq!(world.original.units.len(), 8);
    assert_eq!(
        world.original.source.digest,
        "d683728f9c34659a92c501a3c5a470aeb68887a9f4e5d07284c5be0b950edc58"
    );
    assert_eq!(before.work_custody.len(), 7);
    let historical = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .reading()
        .unwrap();
    assert!(historical.attempts.is_empty());
    assert!(historical.legs.is_empty());
    assert_eq!(historical.whole_run_state, WholeRunState::Incomplete);
    assert!(!historical.completion_verified);

    // Restoration is not fresh admission. The same retained ambiguous source
    // still refuses a new owner attachment without changing the new Run.
    let fresh = World::with_attempts(false);
    let fresh_bytes = std::fs::read(&fresh.path).unwrap();
    let refused = FactoryDevelopmentalFileProvider::open(&fresh.path)
        .unwrap()
        .apply_developmental_mutation(request(
            "fresh-ambiguous-admission",
            &world.original,
            FactoryDevelopmentalMutation::AttachWorkflowSource {
                run_ref: fresh.predecessor.clone(),
                workflow_source: world.original.clone(),
            },
        ))
        .unwrap_err();
    assert!(refused
        .to_string()
        .contains("AmbiguousInitialSubjectRevision"));
    assert_eq!(std::fs::read(&fresh.path).unwrap(), fresh_bytes);

    let mut continuation = world.bounded_contribution("bounded-ambiguous-parent");
    let FactoryDevelopmentalMutation::ContinueCommission {
        workflow_source, ..
    } = &mut continuation.mutation
    else {
        unreachable!()
    };
    // The historical source has immutable authored bytes. A narrower successor
    // must be compiled from its own source instead of reusing those bytes after
    // editing the semantic units underneath their provenance.
    let definition = serde_json::json!({
        "source": {
            "ref": workflow_source.source.reference,
            "revision": workflow_source.source.revision,
            "successorOf": {
                "ref": world.original.source.reference,
                "revision": world.original.source.revision,
                "digest": world.original.source.digest,
            },
        },
        "workflowKey": workflow_source.workflow_key,
        "units": workflow_source.units,
        "barriers": workflow_source.barriers,
        "nesting": workflow_source.nesting,
    });
    let entry = Path::new("bounded-historical-successor.workflow.ts");
    std::fs::write(
        world._directory.path().join(entry),
        format!(
            "import {{ defineWorkflow }} from \"@epilogos/factory-workflow\";\nexport default defineWorkflow({});\n",
            serde_json::to_string_pretty(&definition).unwrap()
        ),
    )
    .unwrap();
    let authored = load_workflow(world._directory.path(), entry).unwrap();
    **workflow_source = authored.source;
    let successor_source_ref = continuation.source.reference.clone();
    FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(continuation)
        .unwrap();
    let after = world.state();
    assert_eq!(
        after.build.run(&world.predecessor),
        before.build.run(&world.predecessor)
    );
    assert!(after.workflow_sources.contains(&world.original));
    assert_eq!(after.commissions, before.commissions);
    assert_eq!(after.work_custody, before.work_custody);
    assert_eq!(after.attempt_states, before.attempt_states);
    let bytes = std::fs::read(&world.path).unwrap();
    // An existing historical attachment is an idempotent readback, not a new
    // admission and not a reason to erase its coordinator.
    FileAttemptStore::attach(
        &world.path,
        world.predecessor.clone(),
        &world.original.source.reference.to_string(),
    )
    .unwrap();
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    FileAttemptStore::attach(
        &world.path,
        SUCCESSOR.parse().unwrap(),
        &successor_source_ref,
    )
    .unwrap();
    let current = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .reading()
        .unwrap();
    assert!(current.source_current);
    assert!(current.attempts.is_empty());
    assert_eq!(current.whole_run_state, WholeRunState::Incomplete);
    assert!(!current.completion_verified);
}

#[test]
fn absent_relation_preserves_legacy_superseding_serialization_and_retry_identity() {
    let world = World::new();
    let legacy = serde_json::to_value(world.continuation("legacy-relation", SUCCESSOR)).unwrap();
    assert!(legacy["mutation"].get("continuationRelation").is_none());
    let parsed: FactoryDevelopmentalMutationRequest =
        serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), legacy);
    let applied = FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(parsed)
        .unwrap();
    let bytes = std::fs::read(&world.path).unwrap();
    let mut explicit = legacy;
    explicit["mutation"]["continuationRelation"] = serde_json::json!("superseding-source");
    let parsed = serde_json::from_value(explicit).unwrap();
    let replay = FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(parsed)
        .unwrap();
    assert_eq!(replay.status, FactoryAdmissionStatus::AlreadyApplied);
    assert_eq!(replay.record, applied.record);
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
}

#[test]
fn superseded_read_only_worker_retains_late_return_and_receiving_without_certifying_successor() {
    let world = World::new();
    world
        .transition(&world.predecessor, "activate-old", RunLifecycle::Active)
        .unwrap();
    let compiled = compile_workflow(world.original.clone()).unwrap();
    let unit = compiled.unit("inspect-source").unwrap();
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::StartSerial {
                attempt_ref: "attempt:old-read-only".into(),
                task_ref: "task:old-read-only".into(),
                parent_journey_ref: world.state().journeys[0].journey_ref.to_string(),
                workflow_unit_ref: unit.reference.clone(),
                disposition: read_only_disposition(&world, &world.predecessor, &world.original),
                retry_grant: None,
                tracking: vec![],
                place_grant: None,
            },
        )
        .unwrap();
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::BindDispatch {
                attempt_ref: "attempt:old-read-only".into(),
                execution_ref: "execution:old-read-only".into(),
                receipt: worker_receipt(OwnerOperationPhase::Submitted),
            },
        )
        .unwrap();
    let basis = UnitDecisionBasis {
        workflow_unit_ref: unit.reference.clone(),
        attempt_ref: "attempt:old-read-only".into(),
        execution_ref: "execution:old-read-only".into(),
        subject_ref: unit.subject_ref.to_string(),
        subject_revision: unit.basis_revision.clone(),
        workflow_source_ref: compiled.source.reference.to_string(),
        workflow_source_revision: compiled.source.revision.clone(),
        workflow_source_digest: compiled.source.digest.clone(),
        resolver_ref: "human:controlled-owner".into(),
        controlled: true,
    };
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::RequestUnitDecision {
                request: UnitDecisionRequest {
                    human_request_ref: "human-request:old-read-only".into(),
                    decision_ref: "decision:old-read-only".into(),
                    question: "Controlled source continuation decision".into(),
                    why_human: "Controlled protocol case".into(),
                    basis: basis.clone(),
                    evidence_refs: BTreeSet::from(["evidence:controlled-source-decision".into()]),
                },
            },
        )
        .unwrap();
    assert_eq!(
        world
            .state()
            .build
            .run(&world.predecessor)
            .unwrap()
            .lifecycle(),
        RunLifecycle::WaitingHuman
    );
    let snapshot =
        serde_json::to_value(world.state().attempt_states[&world.predecessor].snapshot()).unwrap();
    assert_eq!(snapshot["activeWriters"], serde_json::json!({}));
    let continuation = world.continuation("cleanup-continuation", SUCCESSOR);
    let successor_source = match &continuation.mutation {
        FactoryDevelopmentalMutation::ContinueCommission {
            workflow_source, ..
        } => workflow_source.as_ref().clone(),
        _ => unreachable!(),
    };
    let successor: RunRef = SUCCESSOR.parse().unwrap();
    let replacement = BTreeSet::from([
        continuation.mutation_ref.clone(),
        SUCCESSOR.into(),
        format!(
            "{}@{}:{}",
            successor_source.source.reference,
            successor_source.source.revision,
            successor_source.source.digest
        ),
    ]);
    FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(continuation)
        .unwrap();
    FileAttemptStore::attach(
        &world.path,
        successor.clone(),
        &successor_source.source.reference.to_string(),
    )
    .unwrap();
    let successor_unit = compile_workflow(successor_source.clone())
        .unwrap()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    world
        .action(
            &successor,
            FactoryAttemptOperation::StartSerial {
                attempt_ref: "attempt:new-read-only".into(),
                task_ref: "task:new-read-only".into(),
                parent_journey_ref: world.state().journeys[0].journey_ref.to_string(),
                workflow_unit_ref: successor_unit.clone(),
                disposition: read_only_disposition(&world, &successor, &successor_source),
                retry_grant: None,
                tracking: vec![],
                place_grant: None,
            },
        )
        .unwrap();
    let successor_before = world.state().attempt_states[&successor].clone();
    let response = UnitDecisionResponse {
        response_ref: "response:old-obsolete".into(),
        resolver_ref: basis.resolver_ref,
        channel_receipt_ref: "channel:controlled-old-response".into(),
        source_revision: "controlled-human-channel-v1".into(),
        outcome: UnitDecisionOutcome::Resume,
        evidence_refs: BTreeSet::from(["evidence:controlled-response".into()]),
        controlled: true,
    };
    let mut late_reply = world.action_request(
        &world.predecessor,
        FactoryAttemptOperation::ResolveUnitDecision {
            human_request_ref: "human-request:old-read-only".into(),
            response,
        },
    );
    late_reply.caller.caller_ref = "human:controlled-owner".into();
    late_reply.caller.projection_kind = FactoryActionProjectionKind::DesktopHuman;
    let bytes = std::fs::read(&world.path).unwrap();
    assert!(
        FileAttemptStore::open_run(&world.path, world.predecessor.clone())
            .unwrap()
            .apply(late_reply)
            .is_err()
    );
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    assert!(world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::RetireUnitDecision {
                human_request_ref: "human-request:old-read-only".into(),
                reason: "Superseded source".into(),
                replacement_basis_refs: BTreeSet::from([SUCCESSOR.into()]),
            }
        )
        .is_err());
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    assert!(world
        .transition(
            &world.predecessor,
            "unsafe-abort-active",
            RunLifecycle::Aborted
        )
        .is_err());
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: "attempt:old-read-only".into(),
                receipt: worker_receipt(OwnerOperationPhase::Returned),
            },
        )
        .unwrap();
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::RecordVerification {
                attempt_ref: "attempt:old-read-only".into(),
                verification: VerificationReceipt {
                    verification_ref: "verification:controlled-old-work".into(),
                    owner_ref: "factory/verification".into(),
                    source_revision: world.original.source.revision.clone(),
                    outcome: VerificationOutcome::Passed,
                    obligations: unit.verification_obligations.clone(),
                    evidence_refs: BTreeSet::from(["evidence:controlled-old-verification".into()]),
                },
            },
        )
        .unwrap();
    let late_return = world.action_request(
        &world.predecessor,
        FactoryAttemptOperation::ReturnArtifact {
            attempt_ref: "attempt:old-read-only".into(),
            artifact: ReturnedArtifact {
                artifact_ref: "artifact:old-read-only-late".into(),
                subject_ref: unit.subject_ref.to_string(),
                subject_revision: unit.basis_revision.clone(),
                producing_execution_ref: "execution:old-read-only".into(),
                evidence_refs: BTreeSet::from(["evidence:controlled-late-bytes".into()]),
                semantic_difference: "Retained old worker bytes".into(),
            },
            readable_return: ReadableReturn {
                return_ref: "return:old-read-only-late".into(),
                summary: "Controlled old-source Return preserved as historical testimony".into(),
                artifact_refs: BTreeSet::from(["artifact:old-read-only-late".into()]),
                evidence_refs: BTreeSet::from(["evidence:controlled-late-bytes".into()]),
                receiving_ref: None,
                receiving_source_revision: None,
                archive_refs: BTreeSet::new(),
                regression_observation_refs: BTreeSet::new(),
            },
        },
    );
    let first_return = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .apply(late_return.clone())
        .unwrap();
    let bytes = std::fs::read(&world.path).unwrap();
    assert_eq!(
        FileAttemptStore::open_run(&world.path, world.predecessor.clone())
            .unwrap()
            .apply(late_return)
            .unwrap(),
        first_return
    );
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    let receiving = world.action_request(
        &world.predecessor,
        FactoryAttemptOperation::AttachReceiving {
            attempt_ref: "attempt:old-read-only".into(),
            receiving_ref: "receiving:controlled-old-return".into(),
            source_revision: "controlled-receiving-owner-v1".into(),
            evidence_refs: BTreeSet::from(["evidence:controlled-receiving".into()]),
        },
    );
    let first_receiving = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .apply(receiving.clone())
        .unwrap();
    let bytes = std::fs::read(&world.path).unwrap();
    assert_eq!(
        FileAttemptStore::open_run(&world.path, world.predecessor.clone())
            .unwrap()
            .apply(receiving)
            .unwrap(),
        first_receiving
    );
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    let old = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .reading()
        .unwrap();
    assert!(!old.source_current);
    assert_eq!(old.legs[&unit.reference].status, LegStatus::LateResult);
    assert!(old.legs[&unit.reference].artifacts.is_empty());
    assert_eq!(old.legs[&unit.reference].late_artifacts.len(), 1);
    assert!(old.current_returned_units.is_empty());
    assert_eq!(
        old.attempts[0]
            .readable_return
            .as_ref()
            .unwrap()
            .receiving_ref
            .as_deref(),
        Some("receiving:controlled-old-return")
    );
    assert_eq!(
        world
            .state()
            .build
            .run(&world.predecessor)
            .unwrap()
            .lifecycle(),
        RunLifecycle::WaitingHuman
    );
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::RetireUnitDecision {
                human_request_ref: "human-request:old-read-only".into(),
                reason: "Canonical source successor makes this affected basis obsolete".into(),
                replacement_basis_refs: replacement,
            },
        )
        .unwrap();
    assert!(world
        .transition(
            &world.predecessor,
            "unsafe-abort-late",
            RunLifecycle::Aborted
        )
        .is_err());
    world
        .action(
            &world.predecessor,
            FactoryAttemptOperation::MarkQuiescent {
                attempt_ref: "attempt:old-read-only".into(),
            },
        )
        .unwrap();
    world
        .transition(&world.predecessor, "safe-old-abort", RunLifecycle::Aborted)
        .unwrap();
    world
        .transition(
            &world.predecessor,
            "safe-old-archive",
            RunLifecycle::Archived,
        )
        .unwrap();
    let after = world.state();
    assert_eq!(after.attempt_states[&successor], successor_before);
    let build = serde_json::to_value(&after.build).unwrap();
    let request = &build["humanRequests"]["human-request:old-read-only"];
    assert!(request["unitDecisionResponse"].is_null());
    assert!(request["unitDecisionRetirement"].is_object());
    let current = FileAttemptStore::open_run(&world.path, successor)
        .unwrap()
        .reading()
        .unwrap();
    assert_eq!(current.legs[&successor_unit].status, LegStatus::Active);
    assert_eq!(current.whole_run_state, WholeRunState::Incomplete);
    assert!(current.current_returned_units.is_empty());
}

fn disjoint_source(original: &WorkflowSource) -> WorkflowSource {
    let mut source = original.clone();
    source.source.reference = "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FAY"
        .parse()
        .unwrap();
    source.source.revision = "additional-source-v1".into();
    source.workflow_key = "separately-authored-required-work".into();
    source.units.truncate(1);
    source.units[0].key = "additional-required-unit".into();
    source.barriers.clear();
    source.source.digest = workflow_source_digest(&source).unwrap();
    source
}

fn validate_commission_reading(reading: &serde_json::Value) {
    let schema = serde_json::from_str(include_str!(
        "../../contracts/factory/commission.schema.json"
    ))
    .unwrap();
    let request_schema = serde_json::from_str(include_str!(
        "../../contracts/factory/commission-request.schema.json"
    ))
    .unwrap();
    jsonschema::options().with_resource(
        "https://github.com/EpiLogos/agent-system-design/contracts/factory/commission-request.schema.json",
        jsonschema::Resource::from_contents(request_schema).unwrap(),
    ).build(&schema).unwrap().validate(reading).unwrap();
}

#[test]
fn commission_reading_retains_original_run_and_source_bound_continuations_after_restart() {
    let world = World::new();
    let original = world.state().commission_reading(COMMISSION).unwrap();
    assert!(original.continuation_run_refs.is_empty());
    let legacy = serde_json::to_value(&original).unwrap();
    assert!(legacy.get("continuationRunRefs").is_none());
    validate_commission_reading(&legacy);
    let mutation = world.continuation("reading-continuation", SUCCESSOR);
    let source = match &mutation.mutation {
        FactoryDevelopmentalMutation::ContinueCommission {
            workflow_source, ..
        } => workflow_source,
        _ => unreachable!(),
    };
    let source_basis = format!(
        "{}@{}:{}",
        source.source.reference, source.source.revision, source.source.digest
    );
    FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(mutation)
        .unwrap();
    let reading = world.state().commission_reading(COMMISSION).unwrap();
    assert_eq!(reading.commission, original.commission);
    assert_eq!(reading.commission.run_ref, world.predecessor);
    assert_eq!(
        reading.continuation_run_refs,
        vec![SUCCESSOR.parse().unwrap()]
    );
    assert!(reading
        .traversal
        .iter()
        .any(|edge| edge.relation == "continued-from"
            && edge.subject_ref == SUCCESSOR
            && edge.object_ref == world.predecessor.to_string()));
    assert!(reading
        .traversal
        .iter()
        .any(|edge| edge.relation == "source-basis-for"
            && edge.subject_ref == source_basis
            && edge.object_ref == SUCCESSOR));
    let wire = serde_json::to_value(&reading).unwrap();
    validate_commission_reading(&wire);
    let mut duplicate = wire;
    duplicate["continuationRunRefs"] = serde_json::json!([SUCCESSOR, SUCCESSOR]);
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../contracts/factory/commission.schema.json"
    ))
    .unwrap();
    let refs_schema = &schema["$defs"]["reading"]["properties"]["continuationRunRefs"];
    // Resolve the public reference definition for this focused negative set case.
    let mut isolated = refs_schema.clone();
    isolated["items"] = schema["$defs"]["factoryRef"].clone();
    assert!(!jsonschema::Validator::new(&isolated)
        .unwrap()
        .is_valid(&duplicate["continuationRunRefs"]));
}

#[test]
fn attaching_another_source_cannot_orphan_an_existing_native_attempt_snapshot() {
    let world = World::new();
    let before = world.state();
    let extra = disjoint_source(&world.original);
    // The old topology operation itself is valid: the failure is its mismatch
    // with the retained native coordinator, not duplicate node identities.
    let mut changed_run = before.build.run(&world.predecessor).unwrap().clone();
    let compiled = compile_workflow(extra.clone()).unwrap();
    changed_run
        .apply_topology_command(
            &changed_run.mutation_authority(),
            compiled.topology_command(changed_run.revision()),
        )
        .unwrap();
    assert!(before.attempt_states[&world.predecessor]
        .snapshot()
        .restore(
            compile_workflow(world.original.clone()).unwrap(),
            changed_run
        )
        .is_err());
    let bytes = std::fs::read(&world.path).unwrap();
    let refused = FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(request(
            "extra-attached-source",
            &extra,
            FactoryDevelopmentalMutation::AttachWorkflowSource {
                run_ref: world.predecessor.clone(),
                workflow_source: extra.clone(),
            },
        ));
    assert!(refused
        .unwrap_err()
        .to_string()
        .contains("continue-commission"));
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    let reopened = FileAttemptStore::open_run(&world.path, world.predecessor.clone())
        .unwrap()
        .reading()
        .unwrap();
    assert!(reopened.attempts.is_empty());
    assert_eq!(
        world.state().attempt_states[&world.predecessor],
        before.attempt_states[&world.predecessor]
    );
}

#[test]
fn preattach_sources_remain_readable_but_partial_source_selection_cannot_omit_owned_work() {
    let world = World::with_attempts(false);
    let extra = disjoint_source(&world.original);
    FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(request(
            "extra-preattach-source",
            &extra,
            FactoryDevelopmentalMutation::AttachWorkflowSource {
                run_ref: world.predecessor.clone(),
                workflow_source: extra.clone(),
            },
        ))
        .unwrap();
    let before = world.state();
    assert_eq!(before.workflow_sources.len(), 2);
    assert!(before.attempt_states.is_empty());
    let bytes = std::fs::read(&world.path).unwrap();
    for source in [&world.original, &extra] {
        let refused = FileAttemptStore::attach(
            &world.path,
            world.predecessor.clone(),
            &source.source.reference.to_string(),
        )
        .unwrap_err();
        assert!(refused.to_string().contains("UnaccountedWorkflowUnits"));
        assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    }
    assert_eq!(world.state().workflow_sources, before.workflow_sources);
    assert_eq!(
        world.state().build.run(&world.predecessor),
        before.build.run(&world.predecessor)
    );
}

#[test]
fn continuation_retains_exact_predecessor_and_resumes_in_same_commission_after_restart() {
    let world = World::new();
    let before = world.state();
    let mutation = world.continuation("continue", SUCCESSOR);
    let applied = FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(mutation.clone())
        .unwrap();
    assert_eq!(applied.status, FactoryAdmissionStatus::Applied);
    let bytes = std::fs::read(&world.path).unwrap();
    let replay = FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(mutation.clone())
        .unwrap();
    assert_eq!(replay.status, FactoryAdmissionStatus::AlreadyApplied);
    assert_eq!(applied.record, replay.record);
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    let after = world.state();
    assert_eq!(after.commissions, before.commissions);
    assert_eq!(after.journeys.len(), 1);
    assert_eq!(after.journeys[0].runs.len(), 2);
    assert_eq!(
        after.build.run(&world.predecessor),
        before.build.run(&world.predecessor)
    );
    assert_eq!(
        after.attempt_states[&world.predecessor],
        before.attempt_states[&world.predecessor]
    );
    assert_eq!(
        after
            .workflow_sources
            .iter()
            .find(|source| *source == &world.original),
        Some(&world.original)
    );
    let successor: RunRef = SUCCESSOR.parse().unwrap();
    assert_eq!(
        after.build.run(&successor).unwrap().lifecycle(),
        RunLifecycle::Seeded
    );
    let source_ref = match mutation.mutation {
        FactoryDevelopmentalMutation::ContinueCommission {
            workflow_source, ..
        } => workflow_source.source.reference,
        _ => unreachable!(),
    };
    let reading = FileAttemptStore::attach(&world.path, successor, &source_ref.to_string())
        .unwrap()
        .reading()
        .unwrap();
    assert_eq!(reading.whole_run_state, WholeRunState::Incomplete);
    assert!(reading.attempts.is_empty());
}

#[test]
fn changed_retry_identity_and_stale_or_foreign_basis_never_publish_partial_continuation() {
    let world = World::new();
    let good = world.continuation("continue", SUCCESSOR);
    let mut variants = Vec::new();
    for kind in 0..7 {
        let mut invalid = good.clone();
        if let FactoryDevelopmentalMutation::ContinueCommission {
            commission_ref,
            journey_ref,
            predecessor_run_ref,
            expected_journey_revision,
            expected_predecessor_run_revision,
            predecessor_source_revision,
            predecessor_source_digest,
            ..
        } = &mut invalid.mutation
        {
            match kind {
                0 => *commission_ref = "commission:another-undertaking".into(),
                1 => *journey_ref = "journey:01ARZ3NDEKTSV4RRFFQ69G5FBA".parse().unwrap(),
                2 => *predecessor_run_ref = "run:01ARZ3NDEKTSV4RRFFQ69G5FBZ".parse().unwrap(),
                3 => {
                    *expected_journey_revision =
                        Revision::new(expected_journey_revision.get() + 1).unwrap()
                }
                4 => {
                    *expected_predecessor_run_revision =
                        Revision::new(expected_predecessor_run_revision.get() + 1).unwrap()
                }
                5 => *predecessor_source_revision = "different-source-revision".into(),
                _ => *predecessor_source_digest = "0".repeat(64),
            }
        }
        variants.push(invalid);
    }
    let bytes = std::fs::read(&world.path).unwrap();
    for invalid in variants {
        assert!(FactoryDevelopmentalFileProvider::open(&world.path)
            .unwrap()
            .apply_developmental_mutation(invalid)
            .is_err());
        assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    }
    FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(good.clone())
        .unwrap();
    let committed = std::fs::read(&world.path).unwrap();
    let mut changed = good;
    if let FactoryDevelopmentalMutation::ContinueCommission { reason, .. } = &mut changed.mutation {
        *reason = "Changed payload under the same retry identity".into();
    }
    assert!(FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(changed)
        .is_err());
    assert_eq!(std::fs::read(&world.path).unwrap(), committed);
}

#[test]
fn ambiguous_successor_and_duplicate_source_identity_are_refused_atomically() {
    let world = World::new();
    let good = world.continuation("continue", SUCCESSOR);
    let bytes = std::fs::read(&world.path).unwrap();
    let mut ambiguous = good.clone();
    if let FactoryDevelopmentalMutation::ContinueCommission {
        workflow_source, ..
    } = &mut ambiguous.mutation
    {
        workflow_source.units[1].basis_revision = "different-product-pin".into();
        workflow_source.source.digest = workflow_source_digest(workflow_source).unwrap();
    }
    assert!(FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(ambiguous)
        .is_err());
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
    let mut duplicate = good;
    if let FactoryDevelopmentalMutation::ContinueCommission {
        workflow_source, ..
    } = &mut duplicate.mutation
    {
        **workflow_source = world.original.clone();
    }
    duplicate.source.reference = world.original.source.reference.to_string();
    duplicate.source.revision = world.original.source.revision.clone();
    assert!(FactoryDevelopmentalFileProvider::open(&world.path)
        .unwrap()
        .apply_developmental_mutation(duplicate)
        .is_err());
    assert_eq!(std::fs::read(&world.path).unwrap(), bytes);
}

#[test]
fn concurrent_coordinators_cannot_create_two_successors_from_one_journey_revision() {
    let world = World::new();
    let first = world.continuation("first-coordinator", SUCCESSOR);
    let mut second =
        world.continuation("replacement-coordinator", "run:01ARZ3NDEKTSV4RRFFQ69G5FBF");
    if let FactoryDevelopmentalMutation::ContinueCommission {
        workflow_source, ..
    } = &mut second.mutation
    {
        workflow_source.source.reference = "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FAY"
            .parse()
            .unwrap();
        second.source.reference = workflow_source.source.reference.to_string();
    }
    let barrier = Arc::new(Barrier::new(2));
    let threads = [first, second]
        .into_iter()
        .map(|request| {
            let path = world.path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
                barrier.wait();
                provider.apply_developmental_mutation(request).is_ok()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|applied| *applied)
            .count(),
        1
    );
    let after = world.state();
    assert_eq!(after.commissions.len(), 1);
    assert_eq!(after.journeys[0].runs.len(), 2);
    assert_eq!(after.workflow_sources.len(), 2);
}
