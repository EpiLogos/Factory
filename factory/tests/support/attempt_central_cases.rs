//! Real Factory commands; the normal suite uses an explicit Central protocol
//! double. The native evidence lane reruns native_* with the pinned ctrl binary.
use super::*;
use epilogos_factory::attempt_central::CENTRAL_ACTION;
use epilogos_factory::attempt_receiving::CENTRAL_CONTRACT_REVISION;

const CTRL: &str = r#"#!/usr/bin/env python3
import hashlib, json, os, pathlib, sys, time
root=pathlib.Path(sys.argv[sys.argv.index('--root')+1]); action=sys.argv[-2]; request=json.loads(sys.argv[-1])
mode=(root/'central-mode').read_text().strip()
with (root/'central-calls.jsonl').open('a') as log: log.write(json.dumps({'action':action,'request':request})+'\n')
if mode=='slow': time.sleep(.06)
if mode=='pause' and action=='central.now.allocate':
    (root/'central-entered').write_text('test-only')
    while not (root/'central-release').exists(): time.sleep(.01)
policy_path=root/'Control/user/placement.json'; raw=policy_path.read_bytes(); source=json.loads(raw)
revision='test-only-policy:'+hashlib.sha256(raw).hexdigest()
now=int(time.time())
def anchor(path):
    path=pathlib.Path(path); parent=path
    while not parent.exists(): parent=parent.parent
    st=parent.stat()
    return {'path':str(path),'existing_ancestor':str(parent),'device':st.st_dev,'inode':st.st_ino,'exists':path==parent}
policy={'schema':'central.effective-placement-policy/v1','scope_ref':'control:root','revision':revision,
    'sources':[{'source':{'ref':'central:source:control:root:Control/user/placement.json'},'revision':revision}],
    'outside_writes_prevented':False,'required_coverage':source['required_coverage'],'enforcement':source['enforcement'],
    'issued_at_unix_seconds':now,'expires_at_unix_seconds':now+source['lease_seconds'],
    'protected_paths':[str(root/'Control/user')], 'writable_destinations':[{'path':str(root/'Work/demo'),'class':'repository','anchor':anchor(root/'Work/demo')}]}
area=root/'Control/agents/now/test-only'; nr=area/'now.json'; destination=area/'T'
def reading():
    record=json.loads(nr.read_text()); body=nr.read_bytes()
    return {'record':record,'source':{'ref':record['source_ref'],'path':str(nr.relative_to(root))},
        'revision':{'revision':'test-only-now:'+hashlib.sha256(body).hexdigest(),'byte_len':len(body)}}
if action=='central.work.policy': data=policy
elif action=='central.now.allocate':
    assert request['expected_policy_revision']==revision
    created=not nr.exists()
    if created:
        destination.mkdir(parents=True)
        record={'schema':'central.now-clearing/v1','now_ref':'central:now:test-only','source_ref':'central:source:control:root:Control/agents/now/test-only/now.json',
            'scope_ref':'control:root','task_ref':request['task_ref'],'purpose':request['purpose'],
            'participant_refs':request['participant_refs'],'source_refs':request['source_refs'],'lifecycle':'active'}
        nr.write_text(json.dumps(record))
    record=json.loads(nr.read_text())
    for key in ['task_ref','purpose','participant_refs','source_refs']: assert record[key]==request[key]
    if mode=='lost': sys.exit(3)
    data={**reading(),'schema':'central.now-allocation/v1','created':created,'now_ref':record['now_ref'],
        'writable_destination':str(destination),'artifact_namespace':'T','policy':policy,'automatic_agent_or_model_invocation':False}
    if mode=='foreign': data['record']['task_ref']='task:somebody-else'
elif action=='central.now.read': data={**reading(),'schema':'central.now-reading/v1'}
elif action=='central.work.validate':
    current=reading(); path=pathlib.Path(request['destination']); current_anchor=anchor(path)
    allowed=(path.is_relative_to(root/'Work/demo') or path.is_relative_to(destination)) and not any(p.is_symlink() for p in [path,*path.parents])
    allowed=allowed and request['expected_now_revision']==current['revision']['revision'] and request['expected_policy_revision']==revision
    if 'expected_destination_anchor' in request: allowed=allowed and request['expected_destination_anchor']==current_anchor
    data={'schema':'central.work-placement-validation/v1','allowed':allowed,'destination':str(path),'destination_anchor':current_anchor,
        'now_ref':current['record']['now_ref'],'now_revision':current['revision']['revision'],'policy_revision':revision,
        'required_coverage':policy['required_coverage'],'required_enforcement':policy['enforcement'],
        'valid_now_destination':str(destination),'outside_writes_prevented':False,'expires_at_unix_seconds':policy['expires_at_unix_seconds']}
    if not allowed:
        print(json.dumps({'ok':False,'status':'unavailable-capability','action':action,'data':data})); sys.exit(2)
else: raise AssertionError(action)
print(json.dumps({'ok':True,'status':'success','action':action,'data':data}))
"#;

fn setup(world: &World, native: bool) -> std::path::PathBuf {
    // macOS tempdirs hang under the /var symlink; canonicalise so the
    // double's symlink-discipline check sees the real path components.
    let root = world.dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("Control/user")).unwrap();
    fs::create_dir_all(root.join("Control/relations")).unwrap();
    fs::create_dir_all(root.join("Work/demo/src")).unwrap();
    let policy = json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root",
        "writable":[{"path":"Work/demo","class":"repository"}],"protected":[],
        "enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300});
    fs::write(root.join("Control/user/placement.json"), policy.to_string()).unwrap();
    fs::write(root.join("Control/relations/source-relations.json"), json!({
        "schema":"central.control.ground-relations/v1","project_id":"control:root","relations":[{
        "ref":"central:source:control:root:Control/user/placement.json","path":"Control/user/placement.json",
        "roles":["work-placement-policy"],"provenance":"human-adopted","standing":"architecture-contract",
        "treatment":"projectcentral-user","recognition":"controlled-test-fixture-not-personal-adoption","recorded_at_unix_seconds":1}]}).to_string()).unwrap();
    fs::write(root.join("central-mode"), "normal").unwrap();
    let double = root.join("ctrl-double.py");
    fs::write(&double, CTRL).unwrap();
    fs::set_permissions(&double, fs::Permissions::from_mode(0o700)).unwrap();
    if native {
        if let Some(binary) = std::env::var_os("FACTORY_TEST_CTRL") {
            let binary = std::path::PathBuf::from(binary);
            assert!(
                binary.is_absolute() && binary.is_file(),
                "native test requires an actual ctrl binary"
            );
            return binary;
        }
    }
    assert!(
        !native || std::env::var_os("FACTORY_REQUIRE_NATIVE_CTRL").is_none(),
        "native evidence lane must supply FACTORY_TEST_CTRL; no double fallback"
    );
    double
}
fn prepare_request(world: &World, ctrl: &Path, id: &str) -> Value {
    json!({"contract":CENTRAL_ACTION,"requestRef":id,"projectionRef":"projection:central-test",
        "caller":caller(),"runRef":RUN,"expectedRevision":world.reading().revision,"authority":authority(),
        "attemptRef":ATTEMPT,"central":{"binary":ctrl,"root":world.dir.path().canonicalize().unwrap(),"contractRevision":CENTRAL_CONTRACT_REVISION,"project":null},
        "workingDirectory":world.dir.path().canonicalize().unwrap().join("Work/demo"),"destinations":[world.dir.path().canonicalize().unwrap().join("Work/demo/src/result.rs")]})
}
fn preparation(world: &World, request: &Value) -> Value {
    success(world.call("prepare", Some(request)))
}
fn owner_request(world: &World) -> Value {
    let mut request = world.request("native-central-dispatch", "send");
    request["invocation"]["cwd"] =
        json!(world.dir.path().canonicalize().unwrap().join("Work/demo"));
    request
}
fn policy_change(world: &World, key: &str, value: Value) {
    let path = world.dir.path().join("Control/user/placement.json");
    let mut policy: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    policy[key] = value;
    fs::write(path, policy.to_string()).unwrap();
}
fn central_calls(world: &World) -> usize {
    fs::read_to_string(world.dir.path().join("central-calls.jsonl"))
        .unwrap_or_default()
        .lines()
        .count()
}
fn native_world() -> (World, std::path::PathBuf, Value) {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, true);
    let request = prepare_request(&world, &ctrl, "prepare:native");
    (world, ctrl, request)
}

#[test]
fn native_central_preparation_reaches_owner_and_the_actual_dispatch_path() {
    let (world, _, request) = native_world();
    let prepared = preparation(&world, &request);
    assert_eq!(prepared["needsReconciliation"], false, "{prepared}");
    assert!(world.calls().is_empty());
    let checkpoint = &prepared["observation"]["payload"]["detail"]["checkpoint"];
    let now_path = checkpoint["allocation"]["writable_destination"]
        .as_str()
        .unwrap();
    assert!(Path::new(now_path).is_dir());
    assert_eq!(world.reading().attempts[0].tracking.len(), 3);
    let result = success(world.owner(&owner_request(&world)));
    assert_eq!(result["needsReconciliation"], false, "{result}");
    assert_eq!(world.calls().len(), 1);
    let observation = &result["transportObservation"]["payload"]["placementPreflight"];
    assert_eq!(observation["workerEnforcementEstablished"], false);
    assert_eq!(
        observation["now"]["data"]["record"]["now_ref"],
        checkpoint["allocation"]["now_ref"]
    );
    assert!(world.reading().attempts[0].readable_return.is_none());
    assert!(world.calls()[0]["turn"]["packet"]["text"]
        .as_str()
        .unwrap()
        .contains(now_path));
}

#[test]
fn native_central_exact_replay_retains_bytes_and_does_not_reallocate() {
    let (world, _, request) = native_world();
    assert_eq!(preparation(&world, &request)["needsReconciliation"], false);
    let before = fs::read(world.state()).unwrap();
    let calls = central_calls(&world);
    let replay = preparation(&world, &request);
    assert_eq!(replay["replayed"], true);
    assert_eq!(fs::read(world.state()).unwrap(), before);
    assert_eq!(central_calls(&world), calls);
    let mut changed = request;
    changed["workingDirectory"] = json!(world.dir.path().canonicalize().unwrap());
    assert!(!world.call("prepare", Some(&changed)).status.success());
    assert_eq!(fs::read(world.state()).unwrap(), before);
}

#[test]
fn native_central_stale_policy_and_wrong_cwd_stop_before_worker_transport() {
    for change_policy in [true, false] {
        let (world, _, request) = native_world();
        assert_eq!(preparation(&world, &request)["needsReconciliation"], false);
        let mut owner = owner_request(&world);
        if change_policy {
            policy_change(&world, "lease_seconds", json!(299));
        } else {
            owner["invocation"]["cwd"] = json!(world.dir.path().canonicalize().unwrap());
        }
        let result = world.owner(&owner);
        assert!(
            !result.status.success(),
            "changed native basis dispatched a worker"
        );
        assert!(world.calls().is_empty());
        assert!(world.reading().attempts[0].execution_ref.is_none());
    }
}

#[test]
fn native_central_changed_now_or_destination_anchor_stops_dispatch() {
    for change_now in [true, false] {
        let (world, _, request) = native_world();
        let prepared = preparation(&world, &request);
        assert_eq!(prepared["needsReconciliation"], false);
        if change_now {
            let relative = prepared["observation"]["payload"]["detail"]["checkpoint"]["allocation"]
                ["source"]["path"]
                .as_str()
                .unwrap();
            let path = world.dir.path().join(relative);
            let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            record["lifecycle"] = json!("closed");
            fs::write(path, record.to_string()).unwrap();
        } else {
            fs::write(
                world.dir.path().join("Work/demo/src/result.rs"),
                "external new human bytes",
            )
            .unwrap();
        }
        assert!(!world.owner(&owner_request(&world)).status.success());
        assert!(world.calls().is_empty());
    }
}

#[test]
fn native_central_rejected_root_scratch_keeps_actual_allocation_for_corrected_retry() {
    let (world, ctrl, mut request) = native_world();
    request["destinations"] = json!([world
        .dir
        .path()
        .canonicalize()
        .unwrap()
        .join("Work/scratch.diff")]);
    let refused = preparation(&world, &request);
    assert_eq!(refused["needsReconciliation"], true);
    let responses = refused["observation"]["payload"]["detail"]["nativeResponses"]
        .as_array()
        .unwrap();
    let allocation = &responses
        .iter()
        .find(|value| value["action"] == "central.now.allocate")
        .unwrap()["response"]["data"];
    assert!(Path::new(allocation["writable_destination"].as_str().unwrap()).is_dir());
    assert!(!world.dir.path().join("Work/scratch.diff").exists());
    let corrected = preparation(&world, &prepare_request(&world, &ctrl, "prepare:corrected"));
    assert_eq!(corrected["needsReconciliation"], false, "{corrected}");
    assert_eq!(
        corrected["observation"]["payload"]["detail"]["checkpoint"]["allocation"]["now_ref"],
        allocation["now_ref"]
    );
    assert_eq!(
        success(world.owner(&owner_request(&world)))["needsReconciliation"],
        false
    );
}

#[test]
fn native_central_required_worker_enforcement_cannot_be_bypassed_by_preparation() {
    let (world, _, request) = native_world();
    policy_change(&world, "enforcement", json!("harness-interception"));
    let result = preparation(&world, &request);
    assert_eq!(result["needsReconciliation"], false, "{result}");
    let output = world.owner(&owner_request(&world));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("worker interception"));
    assert!(world.calls().is_empty());
}

#[test]
fn preparation_stale_or_unauthorised_requests_never_call_central() {
    for stale in [true, false] {
        let world = World::new();
        world.start(false);
        let ctrl = setup(&world, false);
        let mut request = prepare_request(&world, &ctrl, "prepare:invalid");
        if stale {
            request["expectedRevision"] = json!(1);
        } else {
            request["authority"]["actionAuthorised"] = json!(false);
        }
        let before = fs::read(world.state()).unwrap();
        assert!(!world.call("prepare", Some(&request)).status.success());
        assert_eq!(fs::read(world.state()).unwrap(), before);
        assert_eq!(central_calls(&world), 0);
    }
}

#[test]
fn preparation_lost_allocation_response_recovers_same_native_now_without_dispatch() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    fs::write(world.dir.path().join("central-mode"), "lost").unwrap();
    let mut request = prepare_request(&world, &ctrl, "prepare:lost");
    let uncertain = preparation(&world, &request);
    assert_eq!(uncertain["needsReconciliation"], true);
    let bytes = fs::read(
        world
            .dir
            .path()
            .join("Control/agents/now/test-only/now.json"),
    )
    .unwrap();
    let calls = central_calls(&world);
    assert_eq!(preparation(&world, &request)["replayed"], true);
    assert_eq!(central_calls(&world), calls);
    assert!(!world.owner(&owner_request(&world)).status.success());
    fs::write(world.dir.path().join("central-mode"), "normal").unwrap();
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    let recovered = preparation(&world, &request);
    assert_eq!(recovered["needsReconciliation"], false, "{recovered}");
    assert_eq!(
        fs::read(
            world
                .dir
                .path()
                .join("Control/agents/now/test-only/now.json")
        )
        .unwrap(),
        bytes
    );
    assert!(world.calls().is_empty());
}

#[test]
fn preparation_foreign_task_receipt_never_becomes_a_ready_checkpoint() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    fs::write(world.dir.path().join("central-mode"), "foreign").unwrap();
    let result = preparation(&world, &prepare_request(&world, &ctrl, "prepare:foreign"));
    assert_eq!(result["needsReconciliation"], true);
    assert!(world.reading().attempts[0].tracking.is_empty());
    assert!(!world.owner(&owner_request(&world)).status.success());
    assert!(world.calls().is_empty());
}

#[test]
fn preparation_has_one_total_transport_deadline() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    fs::write(world.dir.path().join("central-mode"), "slow").unwrap();
    let mut request = prepare_request(&world, &ctrl, "prepare:bounded");
    request["timeoutMs"] = json!(100);
    let start = Instant::now();
    let result = preparation(&world, &request);
    assert_eq!(result["needsReconciliation"], true);
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(world.calls().is_empty());
}

#[test]
fn explicit_refresh_crash_invalidates_the_earlier_ready_checkpoint() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    let mut request = prepare_request(&world, &ctrl, "prepare:refresh");
    assert_eq!(preparation(&world, &request)["needsReconciliation"], false);
    fs::write(world.dir.path().join("central-mode"), "pause").unwrap();
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    let mut child = spawn(
        &[
            "attempt".into(),
            "prepare".into(),
            world.state(),
            "-".into(),
            "--json".into(),
        ],
        Some(&request),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !world.dir.path().join("central-entered").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(world.dir.path().join("central-entered").exists());
    child.kill().unwrap();
    child.wait().unwrap();
    fs::write(
        world.dir.path().join("central-release"),
        "release test-only ctrl",
    )
    .unwrap();
    assert!(!world.owner(&owner_request(&world)).status.success());
    fs::write(world.dir.path().join("central-mode"), "normal").unwrap();
    request["expectedRevision"] = json!(world.reading().revision);
    let restored = preparation(&world, &request);
    assert_eq!(restored["needsReconciliation"], false, "{restored}");
}

#[test]
fn preparation_is_discoverable_through_the_existing_native_cli() {
    let result = success(
        spawn(&["capabilities".into(), "--json".into()], None)
            .wait_with_output()
            .unwrap(),
    );
    assert!(result["commands"]
        .as_array()
        .unwrap()
        .contains(&json!("attempt.prepare")));
    assert!(result["commands"]
        .as_array()
        .unwrap()
        .contains(&json!("development.attempt.prepare")));
    assert!(result["nativeContracts"]
        .as_array()
        .unwrap()
        .contains(&json!(CENTRAL_ACTION)));
}

#[path = "attempt_central_followup.rs"]
mod followup;
