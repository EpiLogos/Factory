use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{RetryGrant, ReturnedArtifact};
use epilogos_factory::workflow::{compile_workflow, CompiledWorkflow, WorkflowSource};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Output, Stdio};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const TASK: &str = "task:readback";

fn binary(args: &[String], body: Option<Value>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(body) = body {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.to_string().as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
struct World {
    dir: tempfile::TempDir,
    workflow: CompiledWorkflow,
    disposition: SituatedExecutionDisposition,
}
impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let run = Run::new(
            RUN.parse::<RunRef>().unwrap(),
            PROJECT.parse().unwrap(),
            "task reading test",
            "factory-test",
        )
        .unwrap();
        let source: WorkflowSource = serde_json::from_str(include_str!(
            "../../contracts/factory/fixtures/agent-workflow-source.json"
        ))
        .unwrap();
        let workflow = compile_workflow(source.clone()).unwrap();
        let unit = workflow.unit("inspect-source").unwrap();
        let selection = accept_aikit_selection(
            ExecutionDemand {
                project_ref: PROJECT.into(),
                run_ref: RUN.into(),
                workflow_unit_ref: Some(unit.reference.to_string()),
                agency_ref: Some("agency:task-test".into()),
                profile_ref: None,
                use_type: "test-only".into(),
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
                model_ref: "model:test".into(),
                provider_ref: "provider:test".into(),
                ranking_policy: "test".into(),
                ranking_explanation: json!({"testOnly":true}),
                provenance: vec!["selection:test".into()],
            },
            "2026-09-10T20:00:00+01:00",
        )
        .unwrap();
        let disposition = SituatedExecutionDisposition {
            selection,
            participant: SituatedParticipant {
                agent_ref: unit
                    .agent_requirements
                    .agent_refs
                    .iter()
                    .next()
                    .unwrap()
                    .clone(),
                agency_ref: "agency:task-test".into(),
                world_binding_ref: "binding:test".into(),
                profile_ref: None,
                source_ref: workflow.source.reference.to_string(),
                source_revision: workflow.source.revision.clone(),
                source_digest: format!("blake3:{}", workflow.source.digest),
            },
            context_refs: BTreeSet::from(["context:test".into()]),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            body: ExecutionBody {
                model_ref: "model:test".into(),
                provider_ref: "provider:test".into(),
                route_ref: "route:test".into(),
                harness_ref: "harness:test".into(),
                harness_composition_ref: "composition:test".into(),
                agent_session_ref: "session:test".into(),
                session_space_ref: "space:test".into(),
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
                wall_clock_timeout_ms: Some(10_000),
                retry_grant_ref: Some("grant:task".into()),
                maximum_attempts: Some(2),
            },
        };
        let world = Self {
            dir,
            workflow,
            disposition,
        };
        success(binary(
            &[
                "attempt".into(),
                "init".into(),
                world.state(),
                "-".into(),
                "--json".into(),
            ],
            Some(
                serde_json::to_value(FactoryAttemptSeed {
                    run,
                    workflow_source: source,
                })
                .unwrap(),
            ),
        ));
        world.act(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:z-old".into(),
            task_ref: TASK.into(),
            parent_journey_ref: "journey:test".into(),
            workflow_unit_ref: world
                .workflow
                .unit("inspect-source")
                .unwrap()
                .reference
                .clone(),
            disposition: world.disposition.clone(),
            retry_grant: Some(RetryGrant::new("grant:task", 2).unwrap()),
            tracking: vec![],
        });
        world
    }
    fn state(&self) -> String {
        self.dir.path().join("state.json").display().to_string()
    }
    fn reading(&self) -> FactoryAttemptReading {
        serde_json::from_value(success(binary(
            &[
                "attempt".into(),
                "read".into(),
                self.state(),
                "--json".into(),
            ],
            None,
        )))
        .unwrap()
    }
    fn act(&self, operation: FactoryAttemptOperation) {
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: format!("projection:test:{}", self.reading().revision),
            caller: FactoryActionCaller {
                caller_ref: "agent:test".into(),
                projection_kind: FactoryActionProjectionKind::Headless,
                lineage: vec!["agent:test".into()],
            },
            run_ref: RUN.parse().unwrap(),
            expected_revision: self.reading().revision,
            authority: ProjectedFactoryActionAuthority {
                authority_ref: "authority:test".into(),
                native_owner: "factory".into(),
                capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
            operation,
        };
        success(binary(
            &[
                "attempt".into(),
                "action".into(),
                self.state(),
                "-".into(),
                "--json".into(),
            ],
            Some(serde_json::to_value(request).unwrap()),
        ));
    }
    fn task(&self, extra: &[&str]) -> Output {
        let mut args = vec![
            "attempt".into(),
            "task".into(),
            self.state(),
            RUN.into(),
            TASK.into(),
        ];
        args.extend(extra.iter().map(|value| (*value).into()));
        binary(&args, None)
    }
    fn retry(&self) {
        self.act(FactoryAttemptOperation::Fail {
            attempt_ref: "attempt:z-old".into(),
            reason: "controlled failure".into(),
            evidence_refs: BTreeSet::from(["evidence:failure".into()]),
        });
        self.act(FactoryAttemptOperation::Retry {
            attempt_ref: "attempt:a-new".into(),
            task_ref: TASK.into(),
            parent_journey_ref: "journey:test".into(),
            workflow_unit_ref: self
                .workflow
                .unit("inspect-source")
                .unwrap()
                .reference
                .clone(),
            grant_ref: "grant:task".into(),
            disposition: self.disposition.clone(),
            tracking: vec![],
            reresolution: None,
        });
    }
}

#[test]
fn task_reads_exact_native_source_and_absence_without_mutating_owner_state() {
    let world = World::new();
    let before = std::fs::read(world.state()).unwrap();
    let value = success(world.task(&["--json"]));
    assert_eq!(value["contract"], "factory.attempt-task-reading/v1");
    assert_eq!(value["projectRef"], PROJECT);
    assert_eq!(value["runRef"], RUN);
    assert_eq!(value["workflowSourceRevision"], "source-revision-7");
    assert_eq!(value["sourceCurrent"], true);
    assert_eq!(value["attempts"][0]["standing"], "current-attempt");
    assert_eq!(value["attempts"][0]["modelUsageStatus"], "not-observed");
    assert!(value["attempts"][0]["ownerTelemetryCorrelations"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(before, std::fs::read(world.state()).unwrap());
}

#[test]
fn historical_and_current_are_not_determined_by_lexical_attempt_names() {
    let world = World::new();
    world.retry();
    let value = success(world.task(&["--json"]));
    let views = value["attempts"].as_array().unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0]["record"]["attemptRef"], "attempt:a-new");
    assert_eq!(views[0]["standing"], "current-attempt");
    assert_eq!(views[1]["record"]["attemptRef"], "attempt:z-old");
    assert_eq!(views[1]["standing"], "historical-attempt");
    assert_eq!(views[1]["status"], "failed");
}

#[test]
fn cursor_pins_native_revision_and_invalid_limits_are_refused() {
    let world = World::new();
    world.retry();
    let first = success(world.task(&["--json", "--limit", "1"]));
    let cursor = first["nextCursor"].to_string();
    let next = success(world.task(&["--json", "--limit", "1", "--cursor", &cursor]));
    assert_eq!(next["attempts"].as_array().unwrap().len(), 1);
    assert!(next["nextCursor"].is_null());
    world.act(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "attempt:a-new".into(),
        fact: AttemptTrackingFact {
            fact_ref: "fact:test".into(),
            kind: "source-revision".into(),
            owner_ref: "central".into(),
            subject_ref: "source:test".into(),
            source_revision: "r2".into(),
            evidence_refs: BTreeSet::from(["receipt:source".into()]),
        },
    });
    assert!(!world
        .task(&["--json", "--cursor", &cursor])
        .status
        .success());
    assert!(!world.task(&["--limit", "0"]).status.success());
    assert!(!world.task(&["--limit", "101"]).status.success());
}

#[test]
fn tracking_reference_is_not_promoted_to_model_usage_values() {
    let world = World::new();
    world.act(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "attempt:z-old".into(),
        fact: AttemptTrackingFact {
            fact_ref: "fact:usage".into(),
            kind: "model-usage".into(),
            owner_ref: "actuation".into(),
            subject_ref: "usage:owner".into(),
            source_revision: "r1".into(),
            evidence_refs: BTreeSet::from(["receipt:usage".into()]),
        },
    });
    let value = success(world.task(&["--json"]));
    assert_eq!(value["attempts"][0]["modelUsageStatus"], "not-observed");
    let output = world.task(&[]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("usage:owner") && text.contains("actuation") && text.contains("not-observed")
    );
    assert!(!text.contains("$0"));
}

#[test]
fn readable_return_exposes_evidence_receiving_archive_and_learning_without_recognition() {
    let world = World::new();
    let unit = world.workflow.unit("inspect-source").unwrap();
    let evidence = BTreeSet::from(["evidence:test-return".into()]);
    world.act(FactoryAttemptOperation::BindDispatch {
        attempt_ref: "attempt:z-old".into(),
        execution_ref: "execution:test".into(),
        receipt: OwnerOperationReceipt {
            owner_ref: "aikit".into(),
            contract: "aikit.encounter-delivery/v1".into(),
            operation_ref: "delivery:test".into(),
            receipt_ref: "receipt:test-return".into(),
            source_revision: "owner-r1".into(),
            phase: OwnerOperationPhase::Returned,
            evidence_refs: evidence.clone(),
            partial_effect_refs: BTreeSet::new(),
            payload: json!({"testOnly":true}),
        },
    });
    world.act(FactoryAttemptOperation::RecordVerification {
        attempt_ref: "attempt:z-old".into(),
        verification: VerificationReceipt {
            verification_ref: "verification:test".into(),
            owner_ref: "factory".into(),
            source_revision: "verification-r1".into(),
            outcome: VerificationOutcome::Passed,
            obligations: unit.verification_obligations.clone(),
            evidence_refs: evidence.clone(),
        },
    });
    world.act(FactoryAttemptOperation::ReturnArtifact {
        attempt_ref: "attempt:z-old".into(),
        artifact: ReturnedArtifact {
            artifact_ref: "artifact:test".into(),
            subject_ref: unit.subject_ref.to_string(),
            subject_revision: unit.basis_revision.clone(),
            producing_execution_ref: "execution:test".into(),
            evidence_refs: evidence.clone(),
            semantic_difference: "Test-only inspected source".into(),
        },
        readable_return: ReadableReturn {
            return_ref: "return:test".into(),
            summary: "Readable test-only source report".into(),
            artifact_refs: BTreeSet::from(["artifact:test".into()]),
            evidence_refs: evidence,
            receiving_ref: None,
            receiving_source_revision: None,
            archive_refs: BTreeSet::new(),
            regression_observation_refs: BTreeSet::new(),
        },
    });
    world.act(FactoryAttemptOperation::AttachReceiving {
        attempt_ref: "attempt:z-old".into(),
        receiving_ref: "receiving:test".into(),
        source_revision: "receiving-r1".into(),
        evidence_refs: BTreeSet::from(["receipt:receiving".into()]),
    });
    world.act(FactoryAttemptOperation::AttachArchive {
        attempt_ref: "attempt:z-old".into(),
        archive_ref: "archive:test".into(),
        regression_observation_ref: Some("observation:existing-intake".into()),
    });
    let output = binary(
        &[
            "attempt".into(),
            "return".into(),
            world.state(),
            RUN.into(),
            "attempt:z-old".into(),
        ],
        None,
    );
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for required in [
        "Readable test-only source report",
        "artifact:test",
        "evidence:test-return",
        "receiving:test",
        "archive:test",
        "observation:existing-intake",
        "not-recognition",
    ] {
        assert!(text.contains(required), "missing {required}: {text}");
    }
}

#[test]
fn foreign_run_and_missing_task_are_not_satisfied_by_another_record() {
    let world = World::new();
    for (run, task) in [
        ("run:01ARZ3NDEKTSV4RRFFQ69G5FBC", TASK),
        (RUN, "task:absent"),
    ] {
        assert!(!binary(
            &[
                "attempt".into(),
                "task".into(),
                world.state(),
                run.into(),
                task.into(),
                "--json".into()
            ],
            None
        )
        .status
        .success());
    }
    let output = binary(
        &[
            "development".into(),
            "attempt".into(),
            "return".into(),
            world.state(),
            RUN.into(),
            "attempt:z-old".into(),
        ],
        None,
    );
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("Return: not recorded"));
}

#[test]
fn native_discovery_advertises_attempt_contracts_once_and_preserves_other_products() {
    let value = success(binary(&["capabilities".into(), "--json".into()], None));
    let commands = value["commands"].as_array().unwrap();
    for command in [
        "attempt.owner-action",
        "attempt.task",
        "attempt.return",
        "development.attempt.task",
        "development.field.read",
        "development.observe",
    ] {
        assert_eq!(
            commands
                .iter()
                .filter(|value| value.as_str() == Some(command))
                .count(),
            1,
            "{command}"
        );
    }
    assert!(value["nativeContracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "factory.attempt-task-reading/v1"));
}

#[cfg(unix)]
#[path = "support/attempt_receiving_cases.rs"]
mod receiving;
