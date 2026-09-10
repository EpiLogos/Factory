//! Controlled subprocesses test the production Factory command and transport.
//! They are owner protocol doubles, not commercial execution or installed proof.
#![cfg(unix)]

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::native_owner::AIKIT_CAW_CONTRACT_REVISION;
use epilogos_factory::workflow::{compile_workflow, CompiledWorkflow, WorkflowSource};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const ATTEMPT: &str = "attempt:transport";
const EXECUTION: &str = "execution:transport";
const SESSION: &str = "agent-session:inspect-source";
const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");

// The double verifies the native Factory intent is already on disk BEFORE it
// receives work. Its reply is labelled TEST_ONLY and must never ship as a worker.
const OWNER: &str = r#"#!/usr/bin/env python3
import json, pathlib, sys, time
root = pathlib.Path(__file__).resolve().parent
request = json.loads(sys.argv[sys.argv.index('--request-json') + 1])
state = json.loads((root / 'state.json').read_text())
def intents(value):
    if isinstance(value, dict):
        if value.get('contract') == 'factory.attempt-owner-transport/v1':
            yield value
        for child in value.values():
            yield from intents(child)
    elif isinstance(value, list):
        for child in value:
            yield from intents(child)
assert any(r['phase'] == 'dispatching' for r in intents(state)), 'Factory did not retain an intent'
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps(request) + '\n')
mode = (root / 'mode').read_text().strip()
if request['action'] == 'send':
    (root / 'entered').write_text('TEST_ONLY owner received task')
    if mode == 'lost':
        sys.exit(3)
    if mode == 'pause':
        deadline = time.monotonic() + 10
        while not (root / 'release').exists() and time.monotonic() < deadline:
            time.sleep(0.01)
        if not (root / 'release').exists():
            sys.exit(4)
    if mode == 'malformed':
        print('TEST_ONLY not JSON')
        sys.exit(0)
session = request['agent_session']
delivery = request['turn']['delivery_ref'] if request['action'] == 'send' else request['delivery_ref']
phase = 'submitted' if mode == 'submitted' else 'returned'
if mode == 'wrong-session':
    session = 'agent-session:unrelated'
if mode == 'failed':
    phase = 'failed'
print(json.dumps({'test_only': True, 'delivery': {'agent_session': session, 'delivery_ref': delivery, 'phase': phase, 'terminal_cursor': 41 if phase == 'returned' else None}}))
if mode == 'failed':
    sys.exit(9)
"#;

struct World {
    dir: TempDir,
    run: Run,
    workflow: CompiledWorkflow,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let run = Run::new(RUN.parse::<RunRef>().unwrap(), PROJECT.parse().unwrap(),
            "controlled owner transport", "factory-owner-transport-test").unwrap();
        let source: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
        let workflow = compile_workflow(source.clone()).unwrap();
        fs::write(dir.path().join("owner.py"), OWNER).unwrap();
        fs::set_permissions(dir.path().join("owner.py"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(dir.path().join("mode"), "submitted").unwrap();
        let world = Self { dir, run: run.clone(), workflow };
        let seed = FactoryAttemptSeed { run, workflow_source: source };
        success(world.call("init", Some(&serde_json::to_value(seed).unwrap())));
        world
    }
    fn state(&self) -> String { self.dir.path().join("state.json").display().to_string() }
    fn call(&self, operation: &str, input: Option<&Value>) -> Output {
        let mut args = vec!["attempt".to_owned(), operation.into(), self.state()];
        if input.is_some() { args.push("-".into()); }
        args.push("--json".into());
        spawn(&args, input).wait_with_output().unwrap()
    }
    fn reading(&self) -> FactoryAttemptReading {
        serde_json::from_value(success(self.call("read", None))).unwrap()
    }
    fn action(&self, operation: FactoryAttemptOperation) -> Output {
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(), projection_ref: "projection:transport".into(),
            caller: caller(), run_ref: self.run.reference().clone(), expected_revision: self.reading().revision,
            authority: authority(), operation,
        };
        self.call("action", Some(&serde_json::to_value(request).unwrap()))
    }
    fn start(&self, protected: bool) {
        let unit = self.workflow.unit("inspect-source").unwrap();
        let demand = ExecutionDemand {
            project_ref: PROJECT.into(), run_ref: RUN.into(), workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some("agency:transport".into()), profile_ref: None, use_type: "test-only".into(),
            required_capabilities: unit.capability_refs.clone(), required_modalities: BTreeSet::from(["text".into()]),
            required_actions: BTreeSet::new(), required_tools: BTreeSet::new(), context_characteristics: BTreeSet::new(),
            independence_from: BTreeSet::new(), cost_ceiling_usd: None, latency_preference_ms: None,
            requires_local_materialisation: false,
        };
        let selection = accept_aikit_selection(demand, AikitModelRosterSelection {
            roster_version: AIKIT_MODEL_ROSTER_VERSION.into(), model_ref: "model:test".into(),
            provider_ref: "provider:test".into(), ranking_policy: "test-only".into(),
            ranking_explanation: json!({"testOnly":true}), provenance: vec!["selection:test".into()],
        }, "2026-09-10T20:00:00+01:00").unwrap();
        let disposition = SituatedExecutionDisposition {
            selection, participant: SituatedParticipant {
                agent_ref: unit.agent_requirements.agent_refs.iter().next().unwrap().clone(),
                agency_ref: "agency:transport".into(), world_binding_ref: "world-binding:test".into(), profile_ref: None,
                source_ref: self.workflow.source.reference.to_string(), source_revision: self.workflow.source.revision.clone(),
                source_digest: format!("blake3:{}", self.workflow.source.digest),
            },
            context_refs: BTreeSet::from(["context:test".into()]), praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(), body: ExecutionBody {
                model_ref: "model:test".into(), provider_ref: "provider:test".into(), route_ref: "route:test".into(),
                harness_ref: "harness:test".into(), harness_composition_ref: "composition:test".into(),
                agent_session_ref: SESSION.into(), session_space_ref: "space:test".into(),
                material_world_ref: None, workcell_ref: None,
            },
            placement: protected.then(|| PlacementProtection {
                now_ref: "now:test".into(), now_path: self.dir.path().display().to_string(),
                policy_ref: "policy:test".into(), policy_revision: "policy-r1".into(), authority_ref: "authority:placement".into(),
                writable_paths: BTreeSet::new(), protected_paths: BTreeSet::from(["source:test".into()]),
                required_coverage: BTreeSet::from(["source-protection".into()]),
                effective_coverage: BTreeSet::from(["source-protection".into()]),
                write_boundary_ref: Some("declared-not-enforced".into()), material_receipt_ref: None,
            }),
            permitted_effects: unit.permitted_effects.clone(), verification_obligations: unit.verification_obligations.clone(),
            return_address: unit.return_address.clone(), stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(), budget: ExecutionBudget {
                cost_ceiling_usd: None, latency_preference_ms: None, wall_clock_timeout_ms: Some(120_000),
                retry_grant_ref: None, maximum_attempts: None,
            },
        };
        success(self.action(FactoryAttemptOperation::StartSerial {
            attempt_ref: ATTEMPT.into(), task_ref: "task:inspect".into(), parent_journey_ref: "journey:test".into(),
            workflow_unit_ref: unit.reference.clone(), disposition, retry_grant: None, tracking: vec![],
        }));
    }
    fn request(&self, id: &str, action: &str) -> Value {
        let packet = if action == "send" {
            json!({"action":"send", "agent_session":SESSION, "turn": {
                "delivery_ref":"delivery:test", "sender":caller().caller_ref,
                "expected_binding_revision":"binding-r1", "packet": {
                    "text":"TEST_ONLY inspect exact source", "source_refs":[],
                    "audience":[self.workflow.unit("inspect-source").unwrap().agent_requirements.agent_refs.iter().next().unwrap()]
                }
            }})
        } else { json!({"action":"delivery", "agent_session":SESSION, "delivery_ref":"delivery:test"}) };
        json!({"contract":"factory.attempt-owner-action/v1", "requestRef":id, "projectionRef":"projection:transport",
            "caller":caller(), "runRef":RUN, "expectedRevision":self.reading().revision,
            "authority":authority(), "attemptRef":ATTEMPT, "executionRef":EXECUTION,
            "invocation":{"operation":"aikit-encounter", "binary":self.dir.path().join("owner.py"),
                "cwd":self.dir.path(), "contract_revision":AIKIT_CAW_CONTRACT_REVISION, "request":packet}})
    }
    fn owner(&self, request: &Value) -> Output { self.call("owner-action", Some(request)) }
    fn mode(&self, mode: &str) { fs::write(self.dir.path().join("mode"), mode).unwrap(); }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.dir.path().join("calls.jsonl")).unwrap_or_default().lines()
            .map(|line| serde_json::from_str(line).unwrap()).collect()
    }
}

fn caller() -> FactoryActionCaller {
    FactoryActionCaller { caller_ref: "agent:transport-test".into(), projection_kind: FactoryActionProjectionKind::Headless,
        lineage: vec!["agency:transport-test".into(), "agent:transport-test".into()] }
}
fn authority() -> ProjectedFactoryActionAuthority {
    ProjectedFactoryActionAuthority { authority_ref: "authority:transport-test".into(), native_owner: "factory".into(),
        capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()), capability_granted: true, action_authorised: true }
}
fn spawn(args: &[String], input: Option<&Value>) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory")).args(args)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    if let Some(input) = input { child.stdin.take().unwrap().write_all(serde_json::to_string(input).unwrap().as_bytes()).unwrap(); }
    child
}
fn success(output: Output) -> Value {
    assert!(output.status.success(), "Factory refused: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
fn wait_for(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(start.elapsed() < Duration::from_secs(8), "owner never reached transport");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn dispatch_persists_intent_and_passes_exact_bounds_before_real_subprocess_call() {
    let world = World::new(); world.start(false);
    let receipt = success(world.owner(&world.request("send:1", "send")));
    assert_eq!(receipt["ownerReceipt"]["phase"], "submitted");
    assert_eq!(receipt["needsReconciliation"], false);
    let calls = world.calls(); assert_eq!(calls.len(), 1);
    let text = calls[0].pointer("/turn/packet/text").unwrap().as_str().unwrap();
    assert!(text.contains("source-revision-7") && text.contains("947ce7a") && text.contains("cite exact files and revisions"));
    let reading = world.reading();
    assert_eq!(reading.attempts[0].execution_ref.as_deref(), Some(EXECUTION));
    assert!(reading.attempts[0].readable_return.is_none());
}

#[test]
fn exact_request_replay_never_resends_and_changed_content_is_refused() {
    let world = World::new(); world.start(false);
    let request = world.request("send:replay", "send");
    success(world.owner(&request));
    let bytes = fs::read(world.state()).unwrap();
    assert_eq!(success(world.owner(&request))["replayed"], true);
    assert_eq!(bytes, fs::read(world.state()).unwrap());
    let mut changed = request;
    changed["invocation"]["request"]["turn"]["packet"]["text"] = json!("changed task");
    assert!(!world.owner(&changed).status.success());
    assert_eq!(world.calls().len(), 1);
}

#[test]
fn repeated_unchanged_owner_receipt_is_idempotent_through_fresh_public_reads() {
    let world = World::new(); world.start(false);
    success(world.owner(&world.request("send:repeat", "send")));
    for id in ["read:1", "read:2"] {
        let receipt = success(world.owner(&world.request(id, "delivery")));
        assert_eq!(receipt["ownerReceipt"]["phase"], "submitted");
        assert_eq!(receipt["needsReconciliation"], false);
    }
    assert_eq!(world.calls().len(), 3);
}

#[test]
fn lost_response_recovers_original_intent_without_replaying_task() {
    let world = World::new(); world.start(false); world.mode("lost");
    let request = world.request("send:lost", "send");
    assert_eq!(success(world.owner(&request))["needsReconciliation"], true);
    assert_eq!(success(world.owner(&request))["replayed"], true);
    assert_eq!(world.calls().len(), 1);
    assert!(!world.action(FactoryAttemptOperation::Fail { attempt_ref: ATTEMPT.into(), reason:"unknown effects".into(),
        evidence_refs:BTreeSet::from(["evidence:unknown".into()]) }).status.success());
    world.mode("returned");
    let recovered = success(world.owner(&world.request("read:recover", "delivery")));
    assert_eq!(recovered["needsReconciliation"], false);
    assert_eq!(recovered["ownerReceipt"]["phase"], "returned");
    success(world.action(FactoryAttemptOperation::Fail { attempt_ref: ATTEMPT.into(), reason:"explicit task failure despite completed provider turn".into(),
        evidence_refs:BTreeSet::from(["evidence:task-failed".into()]) }));
    assert_eq!(world.calls().iter().filter(|request| request["action"] == "send").count(), 1);
    assert!(world.reading().attempts[0].readable_return.is_none());
}

#[test]
fn factory_process_death_leaves_durable_intent_and_new_process_can_reconcile() {
    let world = World::new(); world.start(false); world.mode("pause");
    let request = world.request("send:killed", "send");
    let args = vec!["attempt".into(), "owner-action".into(), world.state(), "-".into(), "--json".into()];
    let mut child = spawn(&args, Some(&request));
    wait_for(&world.dir.path().join("entered"));
    child.kill().unwrap(); child.wait().unwrap();
    fs::write(world.dir.path().join("release"), "release TEST_ONLY child").unwrap();
    assert_eq!(success(world.owner(&request))["needsReconciliation"], true);
    world.mode("returned");
    let recovered = success(world.owner(&world.request("read:after-kill", "delivery")));
    assert_eq!(recovered["needsReconciliation"], false);
    assert_eq!(world.calls().iter().filter(|value| value["action"] == "send").count(), 1);
}

#[test]
fn stale_or_unauthorised_requests_never_reach_owner_transport() {
    let world = World::new(); world.start(false);
    let before = fs::read(world.state()).unwrap();
    let mut request = world.request("send:denied", "send");
    request["authority"]["actionAuthorised"] = json!(false);
    assert!(!world.owner(&request).status.success());
    let mut request = world.request("send:stale", "send");
    request["expectedRevision"] = json!(1);
    assert!(!world.owner(&request).status.success());
    assert!(world.calls().is_empty()); assert_eq!(before, fs::read(world.state()).unwrap());
}

#[test]
fn foreign_session_result_is_uncertain_and_cannot_bind_an_execution() {
    let world = World::new(); world.start(false); world.mode("wrong-session");
    let receipt = success(world.owner(&world.request("send:foreign", "send")));
    assert_eq!(receipt["needsReconciliation"], true);
    assert!(world.reading().attempts[0].execution_ref.is_none());
    assert!(world.reading().attempts[0].readable_return.is_none());
}

#[test]
fn same_opening_revision_cannot_dispatch_two_public_writers() {
    let world = World::new(); world.start(false);
    let one = world.request("send:writer-1", "send");
    let two = world.request("send:writer-2", "send");
    let args = vec!["attempt".into(), "owner-action".into(), world.state(), "-".into(), "--json".into()];
    let first = spawn(&args, Some(&one));
    let second = spawn(&args, Some(&two));
    let results = [first.wait_with_output().unwrap(), second.wait_with_output().unwrap()];
    assert_eq!(results.iter().filter(|result| result.status.success()).count(), 1);
    assert_eq!(world.calls().len(), 1);
}

#[test]
fn declared_coverage_is_not_accepted_as_enforcement_on_plain_session_path() {
    let world = World::new(); world.start(true);
    let before = fs::read(world.state()).unwrap();
    let output = world.owner(&world.request("send:protection", "send"));
    assert!(!output.status.success(), "declared placement was incorrectly promoted to enforced protection");
    assert!(world.calls().is_empty()); assert_eq!(before, fs::read(world.state()).unwrap());
}

#[test]
fn development_alias_uses_same_canonical_owner_action_and_readback() {
    let world = World::new(); world.start(false);
    let args = vec!["development".into(), "attempt".into(), "owner-action".into(), world.state(), "-".into(), "--json".into()];
    let request = world.request("send:alias", "send");
    let result = success(spawn(&args, Some(&request)).wait_with_output().unwrap());
    assert_eq!(result["runRef"], RUN);
    assert_eq!(world.reading().attempts[0].execution_ref.as_deref(), Some(EXECUTION));
}
