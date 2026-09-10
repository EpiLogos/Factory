//! Controlled state-machine evidence, not commercial-model or installed-world
//! proof. Every mutation and read below invokes the actual Factory binary in a
//! fresh process. Owner receipts are explicitly marked test-only.

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{LegStatus, RetryGrant, ReturnedArtifact};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";

struct World {
    dir: tempfile::TempDir,
    state: PathBuf,
    workflow: CompiledWorkflow,
}

impl World {
    fn new() -> Self {
        Self::with_source(serde_json::from_str(SOURCE).unwrap())
    }

    fn with_source(source: WorkflowSource) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for name in ["NOW", "work", "Control"] {
            fs::create_dir(dir.path().join(name)).unwrap();
        }
        let state = dir.path().join("attempts.json");
        let workflow = compile_workflow(source.clone()).unwrap();
        let seed = FactoryAttemptSeed {
            run: Run::new(
                RUN.parse().unwrap(),
                PROJECT.parse().unwrap(),
                "public attempt regressions",
                "factory-test",
            )
            .unwrap(),
            workflow_source: source,
        };
        success(command(
            &["attempt", "init", state.to_str().unwrap(), "-", "--json"],
            Some(&serde_json::to_string(&seed).unwrap()),
        ));
        Self {
            dir,
            state,
            workflow,
        }
    }

    fn reading(&self) -> FactoryAttemptReading {
        serde_json::from_slice(
            &success(command(
                &["attempt", "read", self.state.to_str().unwrap(), "--json"],
                None,
            ))
            .stdout,
        )
        .unwrap()
    }

    fn request(&self, operation: FactoryAttemptOperation) -> FactoryAttemptActionRequest {
        let revision = self.reading().revision;
        FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: format!("projection:controlled-{revision}"),
            caller: FactoryActionCaller {
                caller_ref: "caller:controlled-test".into(),
                projection_kind: FactoryActionProjectionKind::Headless,
                lineage: vec!["caller:controlled-test".into()],
            },
            run_ref: RUN.parse::<RunRef>().unwrap(),
            expected_revision: revision,
            authority: ProjectedFactoryActionAuthority {
                authority_ref: "authority:controlled-test".into(),
                native_owner: "factory".into(),
                capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
            operation,
        }
    }

    fn invoke(&self, request: &FactoryAttemptActionRequest) -> Output {
        command(
            &[
                "attempt",
                "action",
                self.state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(&serde_json::to_string(request).unwrap()),
        )
    }

    fn apply(&self, operation: FactoryAttemptOperation) {
        success(self.invoke(&self.request(operation)));
    }

    fn refuses(&self, operation: FactoryAttemptOperation) -> String {
        let request = self.request(operation);
        let bytes = fs::read(&self.state).unwrap();
        let result = self.invoke(&request);
        assert!(
            !result.status.success(),
            "unexpected success: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(
            bytes,
            fs::read(&self.state).unwrap(),
            "refusal mutated durable state"
        );
        String::from_utf8(result.stderr).unwrap()
    }

    fn disposition(&self, key: &str) -> SituatedExecutionDisposition {
        let unit = self.workflow.unit(key).unwrap();
        let agent = unit
            .agent_requirements
            .agent_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agent:controlled-test".into());
        let agency = unit
            .agent_requirements
            .agency_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agency:controlled-test".into());
        let demand = ExecutionDemand {
            project_ref: PROJECT.into(),
            run_ref: RUN.into(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some(agency.clone()),
            profile_ref: None,
            use_type: "controlled-state-test".into(),
            required_capabilities: unit.capability_refs.clone(),
            required_modalities: BTreeSet::new(),
            required_actions: BTreeSet::new(),
            required_tools: BTreeSet::new(),
            context_characteristics: BTreeSet::new(),
            independence_from: BTreeSet::new(),
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            requires_local_materialisation: false,
        };
        let selection = accept_aikit_selection(
            demand,
            AikitModelRosterSelection {
                roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
                model_ref: "model:controlled-test".into(),
                provider_ref: "provider:controlled-test".into(),
                ranking_policy: "controlled-test".into(),
                ranking_explanation: json!({"testOnly":true}),
                provenance: vec!["selection:controlled-test".into()],
            },
            "2026-09-10T20:00:00Z",
        )
        .unwrap();
        SituatedExecutionDisposition {
            selection,
            participant: SituatedParticipant {
                agent_ref: agent,
                agency_ref: agency,
                world_binding_ref: "world-binding:controlled-test".into(),
                profile_ref: None,
                source_ref: "source:controlled-test".into(),
                source_revision: "fixture-v1".into(),
                source_digest: format!("blake3:{}", blake3::hash(b"controlled test only").to_hex()),
            },
            context_refs: set(["context:controlled-test"]),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            body: ExecutionBody {
                model_ref: "model:controlled-test".into(),
                provider_ref: "provider:controlled-test".into(),
                route_ref: "route:controlled-test".into(),
                harness_ref: "harness:controlled-test".into(),
                harness_composition_ref: "composition:controlled-test".into(),
                agent_session_ref: format!("agent-session:controlled-{key}"),
                session_space_ref: "session-space:controlled-test".into(),
                material_world_ref: Some("material-world:controlled-test".into()),
                workcell_ref: Some("workcell:controlled-test".into()),
            },
            placement: Some(PlacementProtection {
                now_ref: "now:controlled-test".into(),
                now_path: self.dir.path().join("NOW").to_str().unwrap().into(),
                policy_ref: "policy:controlled-test".into(),
                policy_revision: "fixture-policy-v1".into(),
                authority_ref: "authority:controlled-test".into(),
                writable_paths: BTreeSet::from([
                    self.dir.path().join("NOW").to_str().unwrap().to_owned(),
                    self.dir.path().join("work").to_str().unwrap().to_owned(),
                ]),
                protected_paths: BTreeSet::from([self
                    .dir
                    .path()
                    .join("Control")
                    .to_str()
                    .unwrap()
                    .to_owned()]),
                required_coverage: set(["file-content"]),
                effective_coverage: set(["file-content"]),
                write_boundary_ref: Some("boundary:controlled-fixture-not-kernel-proof".into()),
                material_receipt_ref: Some("material-receipt:controlled-test".into()),
            }),
            permitted_effects: unit.permitted_effects.clone(),
            verification_obligations: unit.verification_obligations.clone(),
            return_address: unit.return_address.clone(),
            stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(),
            budget: ExecutionBudget {
                cost_ceiling_usd: None,
                latency_preference_ms: None,
                wall_clock_timeout_ms: Some(5000),
                retry_grant_ref: Some(format!("grant:{key}")),
                maximum_attempts: Some(2),
            },
        }
    }

    fn start_operation(&self, reference: &str, key: &str) -> FactoryAttemptOperation {
        FactoryAttemptOperation::StartSerial {
            attempt_ref: reference.into(),
            task_ref: format!("task:{key}"),
            parent_journey_ref: "journey:controlled-test".into(),
            workflow_unit_ref: self.workflow.unit(key).unwrap().reference.clone(),
            disposition: self.disposition(key),
            retry_grant: Some(RetryGrant::new(format!("grant:{key}"), 2).unwrap()),
            tracking: vec![],
        }
    }

    fn start(&self, reference: &str) {
        self.apply(self.start_operation(reference, "inspect-source"));
    }

    fn bind(&self, reference: &str, phase: OwnerOperationPhase) {
        self.apply(FactoryAttemptOperation::BindDispatch {
            attempt_ref: reference.into(),
            execution_ref: format!("execution:{reference}"),
            receipt: owner_receipt(reference, "dispatch", phase),
        });
    }

    fn verify(&self, reference: &str, outcome: VerificationOutcome) {
        self.apply(FactoryAttemptOperation::RecordVerification {
            attempt_ref: reference.into(),
            verification: VerificationReceipt {
                verification_ref: format!("verification:{reference}:{}", self.reading().revision),
                owner_ref: "factory-verifier:controlled-test".into(),
                source_revision: "controlled-verifier-v1".into(),
                outcome,
                obligations: self
                    .workflow
                    .unit("inspect-source")
                    .unwrap()
                    .verification_obligations
                    .clone(),
                evidence_refs: set(["evidence:controlled-verification"]),
            },
        });
    }

    fn fail(&self, reference: &str) {
        self.apply(FactoryAttemptOperation::Fail {
            attempt_ref: reference.into(),
            reason: "controlled failure".into(),
            evidence_refs: set(["evidence:controlled-failure"]),
        });
    }

    fn retry(&self, reference: &str) -> FactoryAttemptOperation {
        FactoryAttemptOperation::Retry {
            attempt_ref: reference.into(),
            task_ref: "task:inspect-source".into(),
            parent_journey_ref: "journey:controlled-test".into(),
            workflow_unit_ref: self
                .workflow
                .unit("inspect-source")
                .unwrap()
                .reference
                .clone(),
            grant_ref: "grant:inspect-source".into(),
            disposition: self.disposition("inspect-source"),
            tracking: vec![],
            reresolution: None,
        }
    }

    fn returned(&self, reference: &str) -> FactoryAttemptOperation {
        let unit = self.workflow.unit("inspect-source").unwrap();
        let artifact_ref = format!("artifact:{reference}");
        let evidence = set(["evidence:controlled-verification"]);
        FactoryAttemptOperation::ReturnArtifact {
            attempt_ref: reference.into(),
            artifact: ReturnedArtifact {
                artifact_ref: artifact_ref.clone(),
                subject_ref: unit.subject_ref.to_string(),
                subject_revision: unit.basis_revision.clone(),
                producing_execution_ref: format!("execution:{reference}"),
                evidence_refs: evidence.clone(),
                semantic_difference: "Controlled test artifact; no provider proof".into(),
            },
            readable_return: ReadableReturn {
                return_ref: format!("return:{reference}"),
                summary: "Controlled native state transition, not live model execution".into(),
                artifact_refs: BTreeSet::from([artifact_ref]),
                evidence_refs: evidence,
                receiving_ref: None,
                receiving_source_revision: None,
                archive_refs: BTreeSet::new(),
                regression_observation_refs: BTreeSet::new(),
            },
        }
    }
}

fn set<const N: usize>(items: [&str; N]) -> BTreeSet<String> {
    items.into_iter().map(str::to_owned).collect()
}

fn owner_receipt(attempt: &str, suffix: &str, phase: OwnerOperationPhase) -> OwnerOperationReceipt {
    OwnerOperationReceipt {
        owner_ref: "aikit".into(),
        contract: "controlled-owner-fixture/v1".into(),
        operation_ref: format!("delivery:{attempt}"),
        receipt_ref: format!("receipt:{attempt}:{suffix}"),
        source_revision: "controlled-owner-fixture".into(),
        phase,
        evidence_refs: set(["evidence:controlled-owner"]),
        partial_effect_refs: BTreeSet::new(),
        payload: json!({"testOnly":true}),
    }
}

fn spawn(args: &[&str], input: Option<&str>) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    child
}
fn command(args: &[&str], input: Option<&str>) -> Output {
    spawn(args, input).wait_with_output().unwrap()
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "Factory command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn native_restart_readback_and_stale_revision_are_process_safe() {
    let world = World::new();
    let stale = world.request(world.start_operation("stale", "inspect-source"));
    world.start("first");
    let before = fs::read(&world.state).unwrap();
    assert!(!world.invoke(&stale).status.success());
    assert_eq!(before, fs::read(&world.state).unwrap());
    let reading = world.reading();
    assert_eq!(reading.attempts[0].attempt_ref, "first");
    assert_eq!(reading.run_ref.to_string(), RUN);
    let stored: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        stored["state"]["build"]["runs"]["runs"][RUN]["writeAuthority"],
        json!({"owner":"factory-test","epoch":1})
    );
    let alias: FactoryAttemptReading = serde_json::from_slice(
        &success(command(
            &[
                "development",
                "attempt",
                "read",
                world.state.to_str().unwrap(),
                "--json",
            ],
            None,
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(reading, alias);
}

#[test]
fn historical_cancellation_cannot_hit_retry_and_late_return_stays_historical() {
    let world = World::new();
    world.start("z-first");
    world.bind("z-first", OwnerOperationPhase::Returned);
    world.verify("z-first", VerificationOutcome::Passed);
    world.fail("z-first");
    world.apply(world.retry("a-second"));
    assert!(world
        .refuses(FactoryAttemptOperation::RequestCancellation {
            attempt_ref: "z-first".into()
        })
        .contains("historical"));
    assert!(world
        .refuses(FactoryAttemptOperation::Fail {
            attempt_ref: "z-first".into(),
            reason: "late failure".into(),
            evidence_refs: set(["evidence:late"])
        })
        .contains("historical"));
    world.apply(world.returned("z-first"));
    let reading = world.reading();
    let leg = &reading.legs[&world.workflow.unit("inspect-source").unwrap().reference];
    assert_eq!(leg.status, LegStatus::Active);
    assert_eq!(leg.execution_ref, "factory-attempt:a-second");
    assert_eq!(leg.attempts[0].late_artifacts.len(), 1);
    assert!(leg.artifacts.is_empty());
    world.fail("a-second");
    assert!(world
        .refuses(world.retry("third"))
        .contains("RetryExhausted"));
}

#[test]
fn unknown_effects_do_not_release_writer_or_become_a_return() {
    let world = World::new();
    world.start("unknown");
    world.bind("unknown", OwnerOperationPhase::Uncertain);
    world.verify("unknown", VerificationOutcome::Passed);
    assert!(world
        .refuses(FactoryAttemptOperation::Fail {
            attempt_ref: "unknown".into(),
            reason: "transport lost".into(),
            evidence_refs: set(["evidence:lost"])
        })
        .contains("uncertain"));
    assert!(world
        .refuses(world.returned("unknown"))
        .contains("uncertain"));
    let reading = world.reading();
    assert_eq!(
        reading.legs.values().next().unwrap().status,
        LegStatus::Active
    );
    assert_eq!(
        reading.attempts[0].dispatch.as_ref().unwrap().phase,
        OwnerOperationPhase::Uncertain
    );
}

#[test]
fn submitted_ack_is_not_work_but_observed_return_and_verification_can_return() {
    let world = World::new();
    world.start("ack");
    world.bind("ack", OwnerOperationPhase::Submitted);
    world.verify("ack", VerificationOutcome::Passed);
    assert!(world
        .refuses(world.returned("ack"))
        .contains("not completed"));
    world.apply(FactoryAttemptOperation::RecordObservation {
        attempt_ref: "ack".into(),
        receipt: owner_receipt("ack", "completed", OwnerOperationPhase::Returned),
    });
    world.apply(world.returned("ack"));
    let reading = world.reading();
    assert_eq!(
        reading.legs.values().next().unwrap().status,
        LegStatus::Returned
    );
    assert!(reading.attempts[0]
        .readable_return
        .as_ref()
        .unwrap()
        .receiving_ref
        .is_none());
    let prose = success(command(
        &["attempt", "read", world.state.to_str().unwrap()],
        None,
    ));
    assert!(String::from_utf8(prose.stdout)
        .unwrap()
        .contains("Controlled native state transition"));
}

#[test]
fn later_failed_verification_supersedes_an_earlier_pass() {
    let world = World::new();
    world.start("verified");
    world.bind("verified", OwnerOperationPhase::Returned);
    world.verify("verified", VerificationOutcome::Passed);
    world.verify("verified", VerificationOutcome::Failed);
    assert!(world
        .refuses(world.returned("verified"))
        .contains("supersedes"));
}

#[test]
fn attribution_preserves_distinct_usage_owners_and_exact_source_revisions() {
    let world = World::new();
    world.start("trace");
    let fact = AttemptTrackingFact {
        fact_ref: "fact:model-usage".into(),
        kind: "model-usage".into(),
        owner_ref: "workcell".into(),
        subject_ref: "usage:controlled".into(),
        source_revision: "usage-revision-exact".into(),
        evidence_refs: set(["evidence:owner-usage"]),
    };
    assert!(world
        .refuses(FactoryAttemptOperation::RecordTracking {
            attempt_ref: "trace".into(),
            fact: fact.clone()
        })
        .contains("misattributes"));
    world.apply(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "trace".into(),
        fact: AttemptTrackingFact {
            owner_ref: "actuation".into(),
            ..fact
        },
    });
    world.apply(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "trace".into(),
        fact: AttemptTrackingFact {
            fact_ref: "fact:source".into(),
            kind: "source-change".into(),
            owner_ref: "central".into(),
            subject_ref: "source:controlled".into(),
            source_revision: "source-revision-exact".into(),
            evidence_refs: set(["evidence:source-change"]),
        },
    });
    let reading = world.reading();
    assert_eq!(
        reading.attempts[0].tracking[0].source_revision,
        "usage-revision-exact"
    );
    assert_eq!(
        reading.attempts[0].tracking[1].source_revision,
        "source-revision-exact"
    );
}

#[test]
fn persisted_retry_spend_and_source_tampering_are_refused_on_public_read() {
    let world = World::new();
    world.start("corrupt");
    let bytes = fs::read(&world.state).unwrap();
    let mut state: Value = serde_json::from_slice(&bytes).unwrap();
    state["state"]["attemptStates"][RUN]["snapshot"]["retryGrants"]["grant:inspect-source"]
        ["attemptsSpent"] = json!(0);
    fs::write(&world.state, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(!command(
        &["attempt", "read", world.state.to_str().unwrap(), "--json"],
        None
    )
    .status
    .success());
    let mut state: Value = serde_json::from_slice(&bytes).unwrap();
    state["state"]["attemptStates"][RUN]["workflowSource"]["source"]["revision"] =
        json!("different-source");
    fs::write(&world.state, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(!command(
        &["attempt", "read", world.state.to_str().unwrap(), "--json"],
        None
    )
    .status
    .success());
}

#[test]
fn parallel_public_writers_cannot_both_commit_the_same_revision() {
    let world = World::new();
    let first =
        serde_json::to_string(&world.request(world.start_operation("one", "inspect-source")))
            .unwrap();
    let second =
        serde_json::to_string(&world.request(world.start_operation("two", "inspect-source")))
            .unwrap();
    let args = [
        "attempt",
        "action",
        world.state.to_str().unwrap(),
        "-",
        "--json",
    ];
    let first = spawn(&args, Some(&first));
    let second = spawn(&args, Some(&second));
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert_ne!(first.status.success(), second.status.success());
    assert_eq!(world.reading().attempts.len(), 1);
}

fn fork_world(shared_writer: bool) -> World {
    let mut value: Value = serde_json::from_str(SOURCE).unwrap();
    value["units"][1]["dependencies"] = json!([]);
    value["units"][2]["dependencies"] = json!([]);
    if shared_writer {
        value["units"][2]["permittedEffects"] = json!(["write Factory review"]);
    }
    let mut source: WorkflowSource = serde_json::from_value(value).unwrap();
    source.source.digest = workflow_source_digest(&source).unwrap();
    World::with_source(source)
}

fn fork(world: &World) -> FactoryAttemptOperation {
    FactoryAttemptOperation::StartFork {
        parent_journey_ref: "journey:controlled-test".into(),
        attempts: ["implement-compiler", "review-adversarially"]
            .into_iter()
            .map(|key| AttemptStart {
                attempt_ref: format!("fork:{key}"),
                task_ref: format!("task:{key}"),
                workflow_unit_ref: world.workflow.unit(key).unwrap().reference.clone(),
                disposition: world.disposition(key),
                retry_grant: Some(RetryGrant::new(format!("grant:{key}"), 2).unwrap()),
                tracking: vec![],
            })
            .collect(),
    }
}

#[test]
fn native_fork_refuses_shared_writer_and_failed_branch_does_not_release_barrier() {
    let conflict = fork_world(true);
    assert!(conflict
        .refuses(fork(&conflict))
        .contains("SharedWriterConflict"));
    let world = fork_world(false);
    world.apply(fork(&world));
    world.fail("fork:review-adversarially");
    assert!(world
        .refuses(world.start_operation("integrate", "integrate-return"))
        .contains("BarrierNotSatisfied"));
    let reading = world.reading();
    assert_eq!(reading.attempts.len(), 2);
}

#[test]
fn wrong_project_and_missing_action_authority_never_publish_attempts() {
    let world = World::new();
    let mut operation = world.start_operation("wrong", "inspect-source");
    if let FactoryAttemptOperation::StartSerial { disposition, .. } = &mut operation {
        disposition.selection.demand.project_ref = "project:01ARZ3NDEKTSV4RRFFQ69G5FAA".into();
    }
    assert!(world.refuses(operation).contains("another Project"));
    let mut request = world.request(world.start_operation("unauthorised", "inspect-source"));
    request.authority.action_authorised = false;
    let before = fs::read(&world.state).unwrap();
    assert!(!world.invoke(&request).status.success());
    assert_eq!(before, fs::read(&world.state).unwrap());
}
