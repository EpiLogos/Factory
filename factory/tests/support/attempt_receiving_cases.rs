//! Owner protocol doubles exist only in the test world. Production calls ctrl.
use super::*;
use epilogos_factory::attempt_receiving::CENTRAL_CONTRACT_REVISION;
use std::os::unix::fs::PermissionsExt;

const CTRL: &str = r#"#!/usr/bin/env python3
import json, os, pathlib, sys
root=pathlib.Path(sys.argv[sys.argv.index('--root')+1])
action=sys.argv[-2]
request=json.loads(sys.argv[-1])
assert action in ['central.receiving.submit','central.receiving.read'], 'Factory attempted review/inclusion'
assert os.environ.get('CENTRAL_NATIVE_TOKEN')=='test-only-central-credential'
assert 'test-only-central-credential' not in ' '.join(sys.argv)
state=json.loads((root/'state.json').read_text())
def walk(value):
    if isinstance(value,dict):
        if value.get('contract')=='factory.attempt-receiving-call/v1': yield value
        for child in value.values(): yield from walk(child)
    elif isinstance(value,list):
        for child in value: yield from walk(child)
assert any(item['phase']=='dispatching' for item in walk(state)), 'missing durable Factory receiving intent'
with (root/'receiving-calls.jsonl').open('a') as file: file.write(json.dumps({'action':action,'request':request})+'\n')
mode=(root/'central-mode').read_text().strip()
path=root/'central-record.json'
if action=='central.receiving.submit':
    if path.exists():
        saved=json.loads(path.read_text())
        assert saved['input']==request, 'idempotent producer key changed proposal'
    else:
        principal=(root/'producer-ref').read_text()
        if mode=='foreign': principal='agent:other'
        record={'schema':'central.received-contribution/v1','return_ref':'central:return:test:opaque-owner-id',
            'source_ref':request['source_ref'],'document_id':request['document_id'],
            'proposed_source_revision':request['expected_source_revision'],'proposal':request['proposal'],
            'author':{'principal_ref':principal,'actor_kind':'agent'},'run_ref':request['run_ref'],
            'task_ref':request['task_ref'],'session_ref':request['session_ref'],
            'now_ref':request.get('now_ref'),'day_ref':request.get('day_ref'),
            'status':'needs-review' if mode=='stale' else 'pending','sequence':1}
        saved={'input':request,'record':record,'revision':'receiving-r1'}
        path.write_text(json.dumps(saved))
        (root/'created-count').write_text('1')
    if mode=='lost': sys.exit(3)
else:
    saved=json.loads(path.read_text())
    assert request['return_ref']==saved['record']['return_ref']
record=saved['record']
data={'schema':'central.receiving-reading/v1','return_ref':record['return_ref'],'revision':saved['revision'],
    'record':record,'included':record['status']=='included','source_changed_by_arrival_or_review':False,'automatic_agent_or_model_invocation':False}
print(json.dumps({'ok':True,'status':'success','action':action,'data':data}))
"#;

impl World {
    fn complete_for_receiving(&self) {
        let unit = self.workflow.unit("inspect-source").unwrap();
        let evidence = BTreeSet::from(["evidence:controlled-return".into()]);
        self.act(FactoryAttemptOperation::BindDispatch {
            attempt_ref: "attempt:z-old".into(),
            execution_ref: "execution:test".into(),
            receipt: OwnerOperationReceipt {
                owner_ref: "aikit".into(),
                contract: "aikit.encounter-delivery/v1".into(),
                operation_ref: "delivery:test".into(),
                receipt_ref: "receipt:controlled-return".into(),
                source_revision: "test-only-r1".into(),
                phase: OwnerOperationPhase::Returned,
                evidence_refs: evidence.clone(),
                partial_effect_refs: BTreeSet::new(),
                payload: json!({"testOnly":true}),
            },
        });
        self.act(FactoryAttemptOperation::RecordVerification {
            attempt_ref: "attempt:z-old".into(),
            verification: VerificationReceipt {
                verification_ref: "verification:controlled".into(),
                owner_ref: "factory".into(),
                source_revision: "test-only-r1".into(),
                outcome: VerificationOutcome::Passed,
                obligations: unit.verification_obligations.clone(),
                evidence_refs: evidence.clone(),
            },
        });
        self.act(FactoryAttemptOperation::ReturnArtifact {
            attempt_ref: "attempt:z-old".into(),
            artifact: ReturnedArtifact {
                artifact_ref: "artifact:controlled".into(),
                subject_ref: unit.subject_ref.to_string(),
                subject_revision: unit.basis_revision.clone(),
                producing_execution_ref: "execution:test".into(),
                evidence_refs: evidence.clone(),
                semantic_difference: "Controlled source report".into(),
            },
            readable_return: ReadableReturn {
                return_ref: "return:controlled".into(),
                summary: "A retained report <script>not executable</script>".into(),
                artifact_refs: BTreeSet::from(["artifact:controlled".into()]),
                evidence_refs: evidence,
                receiving_ref: None,
                receiving_source_revision: None,
                archive_refs: BTreeSet::new(),
                regression_observation_refs: BTreeSet::new(),
            },
        });
    }
    fn central_setup(&self, mode: &str) {
        std::fs::write(self.dir.path().join("ctrl-test.py"), CTRL).unwrap();
        std::fs::set_permissions(
            self.dir.path().join("ctrl-test.py"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        std::fs::write(self.dir.path().join("central-mode"), mode).unwrap();
        std::fs::write(
            self.dir.path().join("producer-ref"),
            &self.disposition.participant.agent_ref,
        )
        .unwrap();
        std::fs::write(
            self.dir.path().join("human-document.html"),
            "<p>Human authored, untouched</p>",
        )
        .unwrap();
    }
    fn receiving_request(&self) -> Value {
        json!({"contract":"factory.attempt-receiving-action/v1","requestRef":"receiving-request:test","projectionRef":"projection:test",
            "caller":{"callerRef":"agent:test","projectionKind":"headless","lineage":["agent:test"]},"runRef":RUN,"expectedRevision":self.reading().revision,
            "authority":{"authorityRef":"authority:test","nativeOwner":"factory","capabilityRef":FACTORY_ATTEMPT_CAPABILITY_REF,"capabilityGranted":true,"actionAuthorised":true},
            "attemptRef":"attempt:z-old","central":{"binary":self.dir.path().join("ctrl-test.py"),"root":self.dir.path(),"contractRevision":CENTRAL_CONTRACT_REVISION,"project":null},
            "target":{"sourceRef":"central:source:test:human-day","documentId":"day:test","sourceRevision":"document-r1","expectedAuthorityRevision":null},"occurredAtUnixSeconds":42,"recover":false})
    }
    fn receiving(&self, request: &Value) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
            .args(["attempt", "receiving", &self.state(), "-", "--json"])
            .env("CENTRAL_NATIVE_TOKEN", "test-only-central-credential")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
    fn central_calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.path().join("receiving-calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

#[test]
fn receiving_uses_retained_return_native_identity_and_never_edits_the_human_source() {
    let world = World::new();
    world.complete_for_receiving();
    world.central_setup("pending");
    let response = success(world.receiving(&world.receiving_request()));
    assert_eq!(response["needsReconciliation"], false);
    assert_eq!(response["humanRecognitionPerformed"], false);
    assert_eq!(response["documentInclusionPerformed"], false);
    let returned = world.reading().attempts[0].readable_return.clone().unwrap();
    assert_eq!(
        returned.receiving_ref.as_deref(),
        Some("central:return:test:opaque-owner-id")
    );
    assert_eq!(
        returned.receiving_source_revision.as_deref(),
        Some("receiving-r1")
    );
    let calls = world.central_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["action"], "central.receiving.submit");
    let input = &calls[0]["request"];
    assert_eq!(input["task_ref"], TASK);
    assert_eq!(input["run_ref"], RUN);
    assert_eq!(input["session_ref"], "session:test");
    let html = input["proposal"]["html"].as_str().unwrap();
    assert!(html.contains("&lt;script&gt;") && html.contains("artifact:controlled"));
    assert!(!html.contains("<script>"));
    assert_eq!(
        std::fs::read_to_string(world.dir.path().join("human-document.html")).unwrap(),
        "<p>Human authored, untouched</p>"
    );
    assert!(!std::fs::read_to_string(world.state())
        .unwrap()
        .contains("test-only-central-credential"));
}

#[test]
fn exact_receiving_replay_is_read_only_and_changed_target_cannot_reuse_request_identity() {
    let world = World::new();
    world.complete_for_receiving();
    world.central_setup("pending");
    let request = world.receiving_request();
    success(world.receiving(&request));
    let before = std::fs::read(world.state()).unwrap();
    let replay = success(world.receiving(&request));
    assert_eq!(replay["replayed"], true);
    assert_eq!(before, std::fs::read(world.state()).unwrap());
    let mut changed = request;
    changed["target"]["sourceRevision"] = json!("replacement-r2");
    assert!(!world.receiving(&changed).status.success());
    assert_eq!(world.central_calls().len(), 1);
}

#[test]
fn lost_receiving_ack_recovers_the_same_native_producer_key_without_a_second_contribution() {
    let world = World::new();
    world.complete_for_receiving();
    world.central_setup("lost");
    let request = world.receiving_request();
    assert_eq!(
        success(world.receiving(&request))["needsReconciliation"],
        true
    );
    assert!(world.reading().attempts[0]
        .readable_return
        .as_ref()
        .unwrap()
        .receiving_ref
        .is_none());
    assert_eq!(success(world.receiving(&request))["replayed"], true);
    assert_eq!(world.central_calls().len(), 1);
    std::fs::write(world.dir.path().join("central-mode"), "pending").unwrap();
    let mut recovery = request;
    recovery["recover"] = json!(true);
    recovery["expectedRevision"] = json!(world.reading().revision);
    let recovered = success(world.receiving(&recovery));
    assert_eq!(recovered["needsReconciliation"], false);
    let calls = world.central_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["request"], calls[1]["request"]);
    assert_eq!(
        std::fs::read_to_string(world.dir.path().join("created-count")).unwrap(),
        "1"
    );
}

#[test]
fn later_native_inclusion_read_preserves_arrival_basis_without_factory_recognition() {
    let world = World::new();
    world.complete_for_receiving();
    world.central_setup("pending");
    let request = world.receiving_request();
    success(world.receiving(&request));
    let path = world.dir.path().join("central-record.json");
    let mut record: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    record["record"]["status"] = json!("included");
    record["record"]["applied_source_revision"] = json!("document-r2");
    record["revision"] = json!("receiving-r2");
    std::fs::write(path, record.to_string()).unwrap();
    let mut recovery = request;
    recovery["recover"] = json!(true);
    recovery["expectedRevision"] = json!(world.reading().revision);
    let response = success(world.receiving(&recovery));
    assert_eq!(response["needsReconciliation"], false);
    assert_eq!(response["centralResponse"]["data"]["included"], true);
    assert_eq!(response["humanRecognitionPerformed"], false);
    assert_eq!(
        world.reading().attempts[0]
            .readable_return
            .as_ref()
            .unwrap()
            .receiving_source_revision
            .as_deref(),
        Some("receiving-r1")
    );
    assert_eq!(
        world.central_calls().last().unwrap()["action"],
        "central.receiving.read"
    );
}

#[test]
fn foreign_producer_receipt_is_retained_as_unresolved_not_attached() {
    let world = World::new();
    world.complete_for_receiving();
    world.central_setup("foreign");
    let response = success(world.receiving(&world.receiving_request()));
    assert_eq!(response["needsReconciliation"], true);
    assert_eq!(
        response["centralResponse"]["data"]["record"]["author"]["principal_ref"],
        "agent:other"
    );
    assert!(world.reading().attempts[0]
        .readable_return
        .as_ref()
        .unwrap()
        .receiving_ref
        .is_none());
}

#[test]
fn missing_return_missing_authority_and_unverified_owner_revision_do_not_call_central() {
    let world = World::new();
    world.central_setup("pending");
    assert!(!world.receiving(&world.receiving_request()).status.success());
    world.complete_for_receiving();
    let mut request = world.receiving_request();
    request["authority"]["actionAuthorised"] = json!(false);
    assert!(!world.receiving(&request).status.success());
    let mut request = world.receiving_request();
    request["central"]["contractRevision"] = json!("unknown-owner-cut");
    assert!(!world.receiving(&request).status.success());
    assert!(world.central_calls().is_empty());
}

#[test]
fn stale_target_is_a_reviewable_received_proposal_not_a_factory_source_update() {
    let world = World::new();
    world.complete_for_receiving();
    world.central_setup("stale");
    let response = success(world.receiving(&world.receiving_request()));
    assert_eq!(response["needsReconciliation"], false);
    assert_eq!(
        response["centralResponse"]["data"]["record"]["status"],
        "needs-review"
    );
    assert_eq!(response["documentInclusionPerformed"], false);
    assert!(world.reading().attempts[0]
        .readable_return
        .as_ref()
        .unwrap()
        .receiving_ref
        .is_some());
}
