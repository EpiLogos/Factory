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
use epilogos_factory::core::run::{Run, RunRef, WorkflowUnitRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{LegStatus, RetryGrant, ReturnedArtifact};
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
    });
    assert!(third.is_err());
    let reading = fixture.reading();
    assert_eq!(reading.attempts.len(), 2);
    assert_eq!(reading.legs[&unit].status, LegStatus::Failed);
    assert!(reading.attempts[0]
        .failure_evidence_refs
        .contains("effect:possible-provider-turn"));
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
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("attempt-state.json");
        let run = Run::new(
            RUN.parse::<RunRef>().unwrap(),
            PROJECT.parse().unwrap(),
            "public native attempt tests",
            "factory-attempt-public-test",
        )
        .unwrap();
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
    }
}

fn disposition(
    run: &Run,
    workflow: &CompiledWorkflow,
    key: &str,
    retry: Option<&RetryGrant>,
) -> SituatedExecutionDisposition {
    let unit = workflow.unit(key).unwrap();
    let agency_ref = "agency:controlled-test";
    let demand = ExecutionDemand {
        project_ref: run.project_ref().to_string(),
        run_ref: run.reference().to_string(),
        workflow_unit_ref: Some(unit.reference.to_string()),
        agency_ref: Some(agency_ref.into()),
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
        selection,
        participant: SituatedParticipant {
            agent_ref: unit
                .agent_requirements
                .agent_refs
                .iter()
                .next()
                .cloned()
                .unwrap_or_else(|| "agent:controlled-test".into()),
            agency_ref: agency_ref.into(),
            world_binding_ref: "world-binding:controlled".into(),
            profile_ref: None,
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
