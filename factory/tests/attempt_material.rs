#![cfg(unix)]

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_material::MATERIAL_ACTION;
use epilogos_factory::attempt_owner_dispatch::FactoryAttemptOwnerRequest;
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::native_owner::{
    NativeOwnerInvocation, WorkcellWorldOperation, WORKCELL_CAW_CONTRACT_REVISION,
};
use epilogos_factory::workflow::{compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const ATTEMPT: &str = "attempt:material";
const MATERIAL: &str = "world:material-test";
const WORKCELL: &str = "workcell:material-test";

fn binary(args: &[String], body: Option<Value>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    if let Some(body) = body {
        child.stdin.take().unwrap().write_all(body.to_string().as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}
fn success(output: Output) -> Value {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
fn refused(output: Output, message: &str) {
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    assert!(String::from_utf8_lossy(&output.stderr).contains(message), "{}", String::from_utf8_lossy(&output.stderr));
}
fn caller() -> FactoryActionCaller {
    FactoryActionCaller { caller_ref: "agent:material-test".into(), projection_kind: FactoryActionProjectionKind::Headless,
        lineage: vec!["agent:material-test".into()] }
}
fn authority() -> ProjectedFactoryActionAuthority {
    ProjectedFactoryActionAuthority { authority_ref: "authority:material-test".into(), native_owner: "factory".into(),
        capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()), capability_granted: true, action_authorised: true }
}

struct World {
    dir: tempfile::TempDir,
    workflow: CompiledWorkflow,
    timeout: u64,
}
impl World {
    fn new() -> Self { Self::with_timeout(5_000) }
    fn with_timeout(timeout: u64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut raw: Value = serde_json::from_str(include_str!("../../contracts/factory/fixtures/agent-workflow-source.json")).unwrap();
        let first = raw["units"][0].clone();
        let mut peer = first.clone();
        peer["key"] = json!("peer-read");
        raw["units"] = json!([first, peer]);
        raw["barriers"] = json!([]);
        raw["nesting"] = json!([]);
        let mut source: WorkflowSource = serde_json::from_value(raw).unwrap();
        source.source.digest = workflow_source_digest(&source).unwrap();
        let workflow = compile_workflow(source.clone()).unwrap();
        let run = Run::new(RUN.parse::<RunRef>().unwrap(), PROJECT.parse().unwrap(), "material consumer tests", "factory-test").unwrap();
        let world = Self { dir, workflow, timeout };
        success(binary(&["attempt".into(), "init".into(), world.state(), "-".into(), "--json".into()],
            Some(serde_json::to_value(FactoryAttemptSeed { run, workflow_source: source }).unwrap())));
        success(world.start("inspect-source", ATTEMPT));
        fs::write(world.receipt(), serde_json::to_vec(&json!({
            "version":"workcell.material-world/v1", "world_ref":MATERIAL, "workcell_ref":WORKCELL,
            "demand_ref":"demand:material-test", "subjects":{"attempt":ATTEMPT,"run":RUN},
            "binding_graph":{"bindings":[],"relations":[]}, "planned_exposures":[],"planned_constraints":[],
            "plan_degradations":[],"plan_omissions":[],"persistence":null,"retention":"preserve",
            "state":"healthy","provenance":{}
        })).unwrap()).unwrap();
        let script = format!(r#"#!/usr/bin/env python3
# Explicit protocol double. Native-owner integration is a separate test.
import json, pathlib, sys, time
root = pathlib.Path({root})
state = json.loads((root / 'state.json').read_text())
observations = state['state']['attemptStates'][{run}]['attempts'][{attempt}]['observations']
assert any(o['contract']=='factory.attempt-material-call/v1' and o['phase']=='dispatching' for o in observations)
args = sys.argv[1:]
assert args[0:2] == ['--endpoint', '127.0.0.1:1']
assert '--authorization' not in args
op = args[-1]
with (root/'calls').open('a') as calls: calls.write(op+'\n')
mode = (root/'mode').read_text() if (root/'mode').exists() else 'ok'
if mode == 'timeout': time.sleep(10)
if mode == 'lost': sys.exit(8)
if mode == 'hold':
    (root/'entered').touch()
    while not (root/'continue').exists(): time.sleep(.01)
if mode == 'swap':
    original = json.loads((root/'world.json').read_text())
    original['world_ref'] = 'world:foreign'
    (root/'world.json').write_text(json.dumps(original))
world = json.loads(pathlib.Path(args[args.index('--receipt')+1]).read_text())
ref = 'world:foreign' if mode == 'foreign' else world['world_ref']
if op == 'inspect':
    value = dict(world, world_ref=ref)
elif op == 'observe':
    value = {{'world_ref':ref,'observations':[{{'logical_ref':'service:test','state':'degraded','detail':{{'probe':'test-only'}}}}]}}
elif op == 'collect': value = {{'world_ref':ref,'outputs':[]}}
elif op == 'expose': value = {{'world_ref':ref,'surfaces':[]}}
elif op == 'recover':
    successor = dict(world, world_ref='world:recovered-test')
    if mode == 'foreign': successor['subjects'] = {{'attempt':'attempt:foreign'}}
    value = {{'world':successor,'previous_world_ref':world['world_ref'],'receipt':'test-only-owner-receipt'}}
else:
    value = {{'world_ref':ref,'disposition':'retained' if mode=='retained' else 'released','changed':mode!='retained'}}
    pathlib.Path(args[args.index('--receipt')+1]).write_text('owner updated transport copy')
value['ok'] = True
print(json.dumps(value))
"#, root=json!(world.dir.path()), run=json!(RUN), attempt=json!(ATTEMPT));
        fs::write(world.owner(), script).unwrap();
        fs::set_permissions(world.owner(), fs::Permissions::from_mode(0o700)).unwrap();
        world
    }
    fn state(&self) -> String { self.dir.path().join("state.json").display().to_string() }
    fn receipt(&self) -> PathBuf { self.dir.path().join("world.json") }
    fn owner(&self) -> PathBuf { self.dir.path().join("owner.py") }
    fn mode(&self, mode: &str) { fs::write(self.dir.path().join("mode"), mode).unwrap(); }
    fn calls(&self) -> usize { fs::read_to_string(self.dir.path().join("calls")).unwrap_or_default().lines().count() }
    fn read(&self) -> FactoryAttemptReading {
        serde_json::from_value(success(binary(&["attempt".into(), "read".into(), self.state(), "--json".into()], None))).unwrap()
    }
    fn action(&self, operation: FactoryAttemptOperation) -> Output {
        let request = FactoryAttemptActionRequest { contract: FACTORY_ATTEMPT_ACTION.into(), projection_ref: "projection:material-tests".into(),
            caller: caller(), run_ref: RUN.parse().unwrap(), expected_revision: self.read().revision, authority: authority(), operation };
        binary(&["attempt".into(),"action".into(),self.state(),"-".into(),"--json".into()],Some(serde_json::to_value(request).unwrap()))
    }
    fn disposition(&self, key: &str) -> SituatedExecutionDisposition {
        let unit = self.workflow.unit(key).unwrap();
        let selection = accept_aikit_selection(ExecutionDemand {
            project_ref:PROJECT.into(),run_ref:RUN.into(),workflow_unit_ref:Some(unit.reference.to_string()),agency_ref:Some("agency:material-test".into()),
            profile_ref:None,use_type:"test-only".into(),required_capabilities:unit.capability_refs.clone(),required_modalities:BTreeSet::from(["text".into()]),
            required_actions:BTreeSet::new(),required_tools:BTreeSet::new(),context_characteristics:BTreeSet::new(),independence_from:BTreeSet::new(),
            cost_ceiling_usd:None,latency_preference_ms:None,requires_local_materialisation:false,
        }, AikitModelRosterSelection { roster_version:AIKIT_MODEL_ROSTER_VERSION.into(),model_ref:"model:test".into(),provider_ref:"provider:test".into(),
            ranking_policy:"test".into(),ranking_explanation:json!({"testOnly":true}),provenance:vec!["selection:test".into()] }, "2026-09-10T20:00:00+01:00").unwrap();
        SituatedExecutionDisposition {
            selection,participant:SituatedParticipant { agent_ref:unit.agent_requirements.agent_refs.iter().next().unwrap().clone(),
                agency_ref:"agency:material-test".into(),world_binding_ref:"binding:test".into(),profile_ref:None,
                source_ref:self.workflow.source.reference.to_string(),source_revision:self.workflow.source.revision.clone(),source_digest:format!("blake3:{}",self.workflow.source.digest) },
            context_refs:BTreeSet::new(),praxis_refs:unit.praxis_refs.clone(),capability_refs:unit.capability_refs.clone(),
            body:ExecutionBody { model_ref:"model:test".into(),provider_ref:"provider:test".into(),route_ref:"route:test".into(),harness_ref:"harness:test".into(),
                harness_composition_ref:"composition:test".into(),agent_session_ref:format!("session:{key}"),session_space_ref:"space:test".into(),
                material_world_ref:Some(MATERIAL.into()),workcell_ref:Some(WORKCELL.into()) },
            placement:None,permitted_effects:unit.permitted_effects.clone(),verification_obligations:unit.verification_obligations.clone(),
            return_address:unit.return_address.clone(),stop_conditions:unit.stop_conditions.clone(),escalation_conditions:unit.escalation_conditions.clone(),
            budget:ExecutionBudget { cost_ceiling_usd:None,latency_preference_ms:None,wall_clock_timeout_ms:Some(self.timeout),retry_grant_ref:None,maximum_attempts:None },
        }
    }
    fn start(&self, key: &str, attempt: &str) -> Output {
        self.action(FactoryAttemptOperation::StartSerial { attempt_ref:attempt.into(),task_ref:format!("task:{key}"),parent_journey_ref:"journey:test".into(),
            workflow_unit_ref:self.workflow.unit(key).unwrap().reference.clone(),disposition:self.disposition(key),retry_grant:None,tracking:vec![] })
    }
    fn request(&self, reference: &str, operation: WorkcellWorldOperation) -> FactoryAttemptOwnerRequest {
        FactoryAttemptOwnerRequest { contract:MATERIAL_ACTION.into(),request_ref:reference.into(),projection_ref:"projection:material".into(),caller:caller(),
            run_ref:RUN.parse().unwrap(),expected_revision:self.read().revision,authority:authority(),attempt_ref:ATTEMPT.into(),
            execution_ref:format!("factory-attempt:{ATTEMPT}"),invocation:NativeOwnerInvocation::WorkcellWorld {
                binary:self.owner(),receipt:self.receipt(),world_operation:operation,endpoint:Some("127.0.0.1:1".into()),authorization:None,
                contract_revision:WORKCELL_CAW_CONTRACT_REVISION.into() } }
    }
    fn invoke(&self, request: &FactoryAttemptOwnerRequest) -> Output {
        binary(&["attempt".into(),"material".into(),self.state(),"-".into(),"--json".into()],Some(serde_json::to_value(request).unwrap()))
    }
    fn quiesce(&self) {
        for operation in [FactoryAttemptOperation::RequestCancellation { attempt_ref:ATTEMPT.into() },
            FactoryAttemptOperation::AcceptCancellation { attempt_ref:ATTEMPT.into() },
            FactoryAttemptOperation::RecordProcessTermination { attempt_ref:ATTEMPT.into() },
            FactoryAttemptOperation::MarkQuiescent { attempt_ref:ATTEMPT.into() }]
        { success(self.action(operation)); }
    }
}

#[test]
fn native_material_reads_preserve_observations_without_creating_execution_or_return() {
    let world = World::new();
    for (index, operation) in [WorkcellWorldOperation::Inspect,WorkcellWorldOperation::Observe,WorkcellWorldOperation::Collect,WorkcellWorldOperation::Expose].into_iter().enumerate() {
        let response = success(world.invoke(&world.request(&format!("read:{index}"),operation)));
        assert_eq!(response["needsReconciliation"],false);
        assert_eq!(response["transportObservation"]["payload"]["detail"]["validated"],true);
        assert_eq!(response["transportObservation"]["payload"]["detail"]["taskReturnEstablished"],false);
    }
    let reading = world.read();
    assert_eq!(world.calls(),4);
    assert!(reading.attempts[0].execution_ref.is_none());
    assert!(reading.attempts[0].readable_return.is_none());
    assert_eq!(reading.attempts[0].observations.len(),8);
}

#[test]
fn exact_material_replay_is_byte_stable_and_changed_requests_are_refused() {
    let world = World::new();
    let request = world.request("read:stable",WorkcellWorldOperation::Observe);
    success(world.invoke(&request));
    let before = fs::read(world.state()).unwrap();
    assert_eq!(success(world.invoke(&request))["replayed"],true);
    assert_eq!(fs::read(world.state()).unwrap(),before);
    assert_eq!(world.calls(),1);
    let mut changed=request.clone(); changed.expected_revision=world.read().revision;
    refused(world.invoke(&changed),"different content");
}

#[test]
fn foreign_material_response_is_retained_as_uncertain_and_blocks_new_effects() {
    let world = World::new();world.mode("foreign");
    let response=success(world.invoke(&world.request("foreign",WorkcellWorldOperation::Observe)));
    assert_eq!(response["needsReconciliation"],true);
    assert_eq!(response["ownerReceipt"]["payload"]["world_ref"],"world:foreign");
    assert_eq!(response["transportObservation"]["payload"]["detail"]["validated"],false);
    refused(world.invoke(&world.request("recover:after-foreign",WorkcellWorldOperation::Recover)),"unresolved material");
    assert_eq!(world.calls(),1);
}

#[test]
fn material_recovery_retains_predecessor_and_requires_new_arrangement_not_body_rewriting() {
    let world=World::new();
    let response=success(world.invoke(&world.request("recover:one",WorkcellWorldOperation::Recover)));
    assert_eq!(response["needsReconciliation"],false);
    assert_eq!(response["ownerReceipt"]["payload"]["previous_world_ref"],MATERIAL);
    assert_eq!(world.read().attempts[0].disposition.body.material_world_ref.as_deref(),Some(MATERIAL));
    refused(world.invoke(&world.request("recover:old",WorkcellWorldOperation::Recover)),"superseded material");
    refused(world.start("peer-read","attempt:peer"),"material lifecycle");
    fs::write(world.receipt(),serde_json::to_vec(&response["ownerReceipt"]["payload"]["world"]).unwrap()).unwrap();
    assert_eq!(success(world.invoke(&world.request("read:successor",WorkcellWorldOperation::Observe)))["ownerReceipt"]["payload"]["world_ref"],"world:recovered-test");
    assert_eq!(world.calls(),2);
}

#[test]
fn release_requires_quiescence_preserves_input_and_does_not_complete_task() {
    let world=World::new();
    refused(world.invoke(&world.request("release:early",WorkcellWorldOperation::Release)),"explicit quiescence");
    assert_eq!(world.calls(),0);
    world.quiesce();
    let before=fs::read(world.receipt()).unwrap();
    let response=success(world.invoke(&world.request("release:ready",WorkcellWorldOperation::Release)));
    assert_eq!(response["transportObservation"]["phase"],"released");
    assert_eq!(fs::read(world.receipt()).unwrap(),before);
    assert!(world.read().attempts[0].readable_return.is_none());
    refused(world.start("peer-read","attempt:peer"),"material lifecycle");
}

#[test]
fn retained_material_is_not_reported_as_released() {
    let world=World::new();world.quiesce();world.mode("retained");
    let response=success(world.invoke(&world.request("release:retained",WorkcellWorldOperation::Release)));
    assert_eq!(response["ownerReceipt"]["payload"]["disposition"],"retained");
    assert_eq!(response["transportObservation"]["phase"],"observed");
}

#[test]
fn release_refuses_other_active_material_users() {
    let world=World::new();
    success(world.start("peer-read","attempt:peer"));world.quiesce();
    refused(world.invoke(&world.request("release:shared",WorkcellWorldOperation::Release)),"another active");
    assert_eq!(world.calls(),0);
}

#[test]
fn release_intent_prevents_a_later_native_start_using_the_same_material() {
    let world=World::new();world.quiesce();world.mode("hold");
    let request=world.request("release:held",WorkcellWorldOperation::Release);
    let state=world.state();
    let handle=std::thread::spawn(move|| binary(&["attempt".into(),"material".into(),state,"-".into(),"--json".into()],Some(serde_json::to_value(request).unwrap())));
    let deadline=Instant::now()+Duration::from_secs(4);
    while !world.dir.path().join("entered").exists() && Instant::now()<deadline {std::thread::sleep(Duration::from_millis(10));}
    assert!(world.dir.path().join("entered").exists(),"owner did not receive the durably reserved call");
    refused(world.start("peer-read","attempt:peer"),"material lifecycle");
    fs::write(world.dir.path().join("continue"),b"continue").unwrap();
    assert_eq!(success(handle.join().unwrap())["needsReconciliation"],false);
}

#[test]
fn lost_material_response_is_never_implicitly_recovered_again() {
    let world=World::new();world.mode("lost");
    let request=world.request("recover:lost",WorkcellWorldOperation::Recover);
    assert_eq!(success(world.invoke(&request))["needsReconciliation"],true);
    assert_eq!(success(world.invoke(&request))["replayed"],true);
    refused(world.invoke(&world.request("recover:new-key",WorkcellWorldOperation::Recover)),"unresolved material");
    assert_eq!(world.calls(),1);
}

#[test]
fn material_transport_deadline_is_finite_without_claiming_worker_quiescence() {
    let world=World::with_timeout(100);world.mode("timeout");
    let before=Instant::now();
    let response=success(world.invoke(&world.request("read:timeout",WorkcellWorldOperation::Observe)));
    assert!(before.elapsed()<Duration::from_secs(3));
    assert_eq!(response["needsReconciliation"],true);
    assert!(world.read().attempts[0].readable_return.is_none());
}

#[test]
fn changed_input_path_cannot_redirect_the_admitted_native_operation() {
    let world=World::new();world.mode("swap");
    let response=success(world.invoke(&world.request("read:stable-copy",WorkcellWorldOperation::Inspect)));
    assert_eq!(response["needsReconciliation"],false);
    assert_eq!(response["ownerReceipt"]["payload"]["world_ref"],MATERIAL);
    let input:Value=serde_json::from_slice(&fs::read(world.receipt()).unwrap()).unwrap();
    assert_eq!(input["world_ref"],"world:foreign");
}

#[test]
fn stale_unauthorised_foreign_and_credential_bearing_calls_do_not_reach_transport() {
    let world=World::new();let before=fs::read(world.state()).unwrap();
    let mut request=world.request("rejected",WorkcellWorldOperation::Inspect);
    request.expected_revision-=1;refused(world.invoke(&request),"stale Factory");
    request.expected_revision=world.read().revision;request.authority.action_authorised=false;
    refused(world.invoke(&request),"authority");request.authority.action_authorised=true;
    if let NativeOwnerInvocation::WorkcellWorld { authorization, .. }=&mut request.invocation { *authorization=Some("test-private-never-retain".into()); }
    let output=world.invoke(&request);assert!(!String::from_utf8_lossy(&output.stderr).contains("test-private-never-retain"));
    refused(output,"WORKCELL_CONTROL_TOKEN");
    assert_eq!(fs::read(world.state()).unwrap(),before);
    assert_eq!(world.calls(),0);
}

#[test]
fn material_alias_and_capability_discovery_use_the_same_native_contract() {
    let world=World::new();
    let request=world.request("alias",WorkcellWorldOperation::Inspect);
    let response=success(binary(&["development".into(),"attempt".into(),"material".into(),world.state(),"-".into(),"--json".into()],Some(serde_json::to_value(request).unwrap())));
    assert_eq!(response["contract"],"factory.attempt-material-receipt/v1");
    let capabilities=success(binary(&["capabilities".into(),"--json".into()],None));
    for name in ["attempt.material","development.attempt.material"] { assert!(capabilities["commands"].as_array().unwrap().contains(&json!(name))); }
    assert!(capabilities["nativeContracts"].as_array().unwrap().contains(&json!(MATERIAL_ACTION)));
}
