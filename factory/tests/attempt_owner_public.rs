//! Public-path evidence for Factory's persisted attempt-to-owner handoff.
//!
//! The fake AIKit executable exists only inside this test World. Production code
//! still invokes the configured native owner binary; these tests prove Factory's
//! durable intent/recovery semantics without claiming installed-world execution.

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_owner_dispatch::{
    FactoryAttemptOwnerReceipt, FactoryAttemptOwnerRequest, FACTORY_ATTEMPT_OWNER_ACTION,
};
use epilogos_factory::attempt_runtime::{
    ExecutionBody, ExecutionBudget, FactoryAttemptActionRequest, FactoryAttemptOperation,
    FactoryAttemptReading, FactoryAttemptSeed, PlacementProtection, SituatedExecutionDisposition,
    SituatedParticipant, FACTORY_ATTEMPT_ACTION, FACTORY_ATTEMPT_CAPABILITY_REF,
};
use epilogos_factory::core::run::Run;
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::native_owner::{NativeOwnerInvocation, AIKIT_CAW_CONTRACT_REVISION};
use epilogos_factory::orchestration::RetryGrant;
use epilogos_factory::workflow::{compile_workflow, WorkflowSource};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const ATTEMPT: &str = "owner-public-attempt";
const UNIT: &str = "inspect-source";

struct World {
    dir: tempfile::TempDir,
    state: PathBuf,
    session: String,
    agent: String,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        for name in ["NOW", "work", "Control"] {
            fs::create_dir(dir.path().join(name)).unwrap();
        }
        let state = dir.path().join("state.json");
        let source: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
        let workflow = compile_workflow(source.clone()).unwrap();
        let unit = workflow.unit(UNIT).unwrap();
        let agent = unit
            .agent_requirements
            .agent_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agent:owner-public".into());
        let agency = unit
            .agent_requirements
            .agency_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agency:owner-public".into());
        let session = "agent-session:owner-public".to_owned();
        let seed = FactoryAttemptSeed {
            run: Run::new(
                RUN.parse().unwrap(),
                PROJECT.parse().unwrap(),
                "owner public regression",
                "factory-test",
            )
            .unwrap(),
            workflow_source: source,
        };
        success(run(
            &["attempt", "init", state.to_str().unwrap(), "-", "--json"],
            Some(&serde_json::to_string(&seed).unwrap()),
        ));

        let demand = ExecutionDemand {
            project_ref: PROJECT.into(),
            run_ref: RUN.into(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some(agency.clone()),
            profile_ref: None,
            use_type: "owner-public-test".into(),
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
                model_ref: "model:owner-public".into(),
                provider_ref: "provider:owner-public".into(),
                ranking_policy: "test-only".into(),
                ranking_explanation: json!({"testOnly": true}),
                provenance: vec!["selection:owner-public".into()],
            },
            "2026-09-10T20:00:00Z",
        )
        .unwrap();
        let disposition = SituatedExecutionDisposition {
            selection,
            participant: SituatedParticipant {
                agent_ref: agent.clone(),
                agency_ref: agency,
                world_binding_ref: "world-binding:owner-public".into(),
                profile_ref: None,
                source_ref: "source:owner-public".into(),
                source_revision: "fixture-v1".into(),
                source_digest: format!(
                    "blake3:{}",
                    blake3::hash(b"owner public test only").to_hex()
                ),
            },
            context_refs: set(["context:owner-public"]),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            body: ExecutionBody {
                model_ref: "model:owner-public".into(),
                provider_ref: "provider:owner-public".into(),
                route_ref: "route:owner-public".into(),
                harness_ref: "harness:owner-public".into(),
                harness_composition_ref: "composition:owner-public".into(),
                agent_session_ref: session.clone(),
                session_space_ref: "session-space:owner-public".into(),
                material_world_ref: Some("material-world:owner-public".into()),
                workcell_ref: Some("workcell:owner-public".into()),
            },
            placement: Some(PlacementProtection {
                now_ref: "now:owner-public".into(),
                now_path: dir.path().join("NOW").to_string_lossy().into_owned(),
                policy_ref: "policy:owner-public".into(),
                policy_revision: "fixture-policy-v1".into(),
                authority_ref: "authority:owner-public".into(),
                writable_paths: BTreeSet::from([
                    dir.path().join("NOW").to_string_lossy().into_owned(),
                    dir.path().join("work").to_string_lossy().into_owned(),
                ]),
                protected_paths: BTreeSet::from([dir
                    .path()
                    .join("Control")
                    .to_string_lossy()
                    .into_owned()]),
                required_coverage: set(["file-content"]),
                effective_coverage: set(["file-content"]),
                write_boundary_ref: Some("boundary:test-only".into()),
                material_receipt_ref: Some("material-receipt:test-only".into()),
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
                retry_grant_ref: Some("grant:owner-public".into()),
                maximum_attempts: Some(2),
            },
        };
        let reading = read(&state);
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: "projection:owner-public-start".into(),
            caller: caller(),
            run_ref: RUN.parse().unwrap(),
            expected_revision: reading.revision,
            authority: authority(),
            operation: FactoryAttemptOperation::StartSerial {
                attempt_ref: ATTEMPT.into(),
                task_ref: "task:owner-public".into(),
                parent_journey_ref: "journey:owner-public".into(),
                workflow_unit_ref: unit.reference.clone(),
                disposition,
                retry_grant: Some(RetryGrant::new("grant:owner-public", 2).unwrap()),
                tracking: vec![],
            },
        };
        success(run(
            &["attempt", "action", state.to_str().unwrap(), "-", "--json"],
            Some(&serde_json::to_string(&request).unwrap()),
        ));
        Self {
            dir,
            state,
            session,
            agent,
        }
    }

    fn owner_request(
        &self,
        binary: &Path,
        request_ref: &str,
        delivery: &str,
    ) -> FactoryAttemptOwnerRequest {
        FactoryAttemptOwnerRequest {
            contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
            request_ref: request_ref.into(),
            projection_ref: format!("projection:{request_ref}"),
            caller: caller(),
            run_ref: RUN.parse().unwrap(),
            expected_revision: read(&self.state).revision,
            authority: authority(),
            attempt_ref: ATTEMPT.into(),
            execution_ref: "execution:owner-public".into(),
            invocation: NativeOwnerInvocation::AikitEncounter {
                binary: binary.to_path_buf(),
                cwd: self.dir.path().to_path_buf(),
                contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
                request: json!({
                    "action": "send",
                    "agent_session": self.session,
                    "turn": {
                        "delivery_ref": delivery,
                        "sender": "caller:owner-public",
                        "packet": {
                            "audience": [self.agent],
                            "text": "perform bounded owner-public test task"
                        }
                    }
                }),
            },
        }
    }

    fn invoke_owner(&self, request: &FactoryAttemptOwnerRequest) -> Output {
        run(
            &[
                "attempt",
                "owner-action",
                self.state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(&serde_json::to_string(request).unwrap()),
        )
    }
}

fn caller() -> FactoryActionCaller {
    FactoryActionCaller {
        caller_ref: "caller:owner-public".into(),
        projection_kind: FactoryActionProjectionKind::Headless,
        lineage: vec!["caller:owner-public".into()],
    }
}

fn authority() -> ProjectedFactoryActionAuthority {
    ProjectedFactoryActionAuthority {
        authority_ref: "authority:owner-public".into(),
        native_owner: "factory".into(),
        capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
        capability_granted: true,
        action_authorised: true,
    }
}

fn set<const N: usize>(items: [&str; N]) -> BTreeSet<String> {
    items.into_iter().map(str::to_owned).collect()
}

fn read(state: &Path) -> FactoryAttemptReading {
    serde_json::from_slice(
        &success(run(
            &["attempt", "read", state.to_str().unwrap(), "--json"],
            None,
        ))
        .stdout,
    )
    .unwrap()
}

fn fake_owner(dir: &Path, phase: &str, exit_code: i32) -> (PathBuf, PathBuf) {
    let binary = dir.join(format!("fake-aikit-{phase}.sh"));
    let count = dir.join(format!("fake-aikit-{phase}.count"));
    let body = format!(
        "#!/bin/sh\ncount=0\n[ -f '{count}' ] && count=$(cat '{count}')\ncount=$((count+1))\nprintf '%s' \"$count\" > '{count}'\nprintf '%s\\n' '{{\"delivery\":{{\"delivery_ref\":\"delivery:owner-public\",\"agent_session\":\"agent-session:owner-public\",\"phase\":\"{phase}\",\"terminal_cursor\":1}}}}'\nexit {exit_code}\n",
        count = count.display(),
    );
    fs::write(&binary, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary, permissions).unwrap();
    }
    (binary, count)
}

fn run(args: &[&str], input: Option<&str>) -> Output {
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
    child.wait_with_output().unwrap()
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
#[cfg(unix)]
fn public_owner_action_binds_real_receipt_and_exact_replay_never_resends() {
    let world = World::new();
    let (binary, count) = fake_owner(world.dir.path(), "submitted", 0);
    let request = world.owner_request(&binary, "owner-call:submitted", "delivery:owner-public");
    let first: FactoryAttemptOwnerReceipt =
        serde_json::from_slice(&success(world.invoke_owner(&request)).stdout).unwrap();
    assert!(!first.replayed);
    assert!(!first.needs_reconciliation);
    assert_eq!(fs::read_to_string(&count).unwrap(), "1");
    let attempt = read(&world.state)
        .attempts
        .into_iter()
        .find(|record| record.attempt_ref == ATTEMPT)
        .unwrap();
    assert_eq!(
        attempt.execution_ref.as_deref(),
        Some("execution:owner-public")
    );

    let replay: FactoryAttemptOwnerReceipt =
        serde_json::from_slice(&success(world.invoke_owner(&request)).stdout).unwrap();
    assert!(replay.replayed);
    assert_eq!(fs::read_to_string(&count).unwrap(), "1");
}

#[test]
#[cfg(unix)]
fn public_owner_failure_is_durable_uncertainty_and_replay_never_reissues_effect() {
    let world = World::new();
    let (binary, count) = fake_owner(world.dir.path(), "submitted", 17);
    let request = world.owner_request(&binary, "owner-call:uncertain", "delivery:owner-public");
    let first: FactoryAttemptOwnerReceipt =
        serde_json::from_slice(&success(world.invoke_owner(&request)).stdout).unwrap();
    assert!(first.needs_reconciliation);
    assert_eq!(fs::read_to_string(&count).unwrap(), "1");
    let reading = read(&world.state);
    let attempt = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == ATTEMPT)
        .unwrap();
    assert!(attempt.execution_ref.is_none());
    assert!(attempt.observations.iter().any(|receipt| {
        format!("{:?}", receipt.phase) == "Uncertain"
            && receipt.payload["detail"]["instruction"]
                == "observe exact owner delivery; do not resend"
    }));

    let replay: FactoryAttemptOwnerReceipt =
        serde_json::from_slice(&success(world.invoke_owner(&request)).stdout).unwrap();
    assert!(replay.replayed);
    assert!(replay.needs_reconciliation);
    assert_eq!(fs::read_to_string(&count).unwrap(), "1");
}
