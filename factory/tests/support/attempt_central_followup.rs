use super::*;

#[test]
fn unresolved_preparation_cannot_be_replaced_by_a_different_request() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    world.mode("returned");
    fs::write(world.dir.path().join("central-mode"), "lost").unwrap();
    let request = prepare_request(&world, &ctrl, "prepare:unresolved");
    assert_eq!(preparation(&world, &request)["needsReconciliation"], true);
    fs::write(world.dir.path().join("central-mode"), "normal").unwrap();
    let replacement = prepare_request(&world, &ctrl, "prepare:replacement");
    let before = fs::read(world.state()).unwrap();
    let calls = central_calls(&world);
    assert!(!world.call("prepare", Some(&replacement)).status.success());
    assert_eq!(fs::read(world.state()).unwrap(), before);
    assert_eq!(central_calls(&world), calls);
    assert!(world.calls().is_empty());
}

#[test]
fn old_preparation_replay_is_read_only_but_cannot_refresh_over_its_successor() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    let mut first = prepare_request(&world, &ctrl, "prepare:first");
    assert_eq!(preparation(&world, &first)["needsReconciliation"], false);
    let second = prepare_request(&world, &ctrl, "prepare:second");
    assert_eq!(preparation(&world, &second)["needsReconciliation"], false);
    let before = fs::read(world.state()).unwrap();
    let calls = central_calls(&world);
    assert_eq!(preparation(&world, &first)["replayed"], true);
    first["recover"] = json!(true);
    first["expectedRevision"] = json!(world.reading().revision);
    assert!(!world.call("prepare", Some(&first)).status.success());
    assert_eq!(fs::read(world.state()).unwrap(), before);
    assert_eq!(central_calls(&world), calls);
}

#[test]
fn two_preparations_on_the_same_opening_revision_have_one_winner() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    let first = prepare_request(&world, &ctrl, "prepare:concurrent-a");
    let second = prepare_request(&world, &ctrl, "prepare:concurrent-b");
    let args = [
        "attempt".into(),
        "prepare".into(),
        world.state(),
        "-".into(),
        "--json".into(),
    ];
    let one = spawn(&args, Some(&first));
    let two = spawn(&args, Some(&second));
    let outputs = [
        one.wait_with_output().unwrap(),
        two.wait_with_output().unwrap(),
    ];
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1
    );
    for output in outputs.into_iter().filter(|output| output.status.success()) {
        assert_eq!(success(output)["needsReconciliation"], false);
    }
    assert_eq!(world.reading().attempts[0].tracking.len(), 3);
    assert!(world.calls().is_empty());
}

#[test]
fn expired_preparation_requires_explicit_fresh_native_readback() {
    let world = World::new();
    world.start(false);
    let ctrl = setup(&world, false);
    policy_change(&world, "lease_seconds", json!(2));
    let mut request = prepare_request(&world, &ctrl, "prepare:expiry");
    assert_eq!(preparation(&world, &request)["needsReconciliation"], false);
    std::thread::sleep(Duration::from_secs(3));
    assert!(!world.owner(&owner_request(&world)).status.success());
    assert!(world.calls().is_empty());
    request["recover"] = json!(true);
    request["expectedRevision"] = json!(world.reading().revision);
    assert_eq!(preparation(&world, &request)["needsReconciliation"], false);
    assert_eq!(
        success(world.owner(&owner_request(&world)))["needsReconciliation"],
        false
    );
}

/// The normal Factory suite has no Central build dependency. The required
/// native-evidence lane supplies its exact source-built binary and explicitly
/// executes this test with --include-ignored; absent native execution is not a pass.
#[test]
#[ignore = "requires the pinned source-built ctrl; executed by native attempt evidence CI"]
fn native_central_return_keeps_the_allocated_now_without_including_the_document() {
    let binary =
        std::env::var_os("FACTORY_TEST_CTRL").expect("native ctrl is required, no protocol double");
    let binary = std::path::PathBuf::from(binary);
    let world = World::new();
    world.start(false);
    setup(&world, false);
    let agent_ref = world.reading().attempts[0]
        .disposition
        .participant
        .agent_ref
        .clone();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let authority_path = world.dir.path().join("Control/user/authority.json");
    let grants = json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":[
        {"principal_ref":"human:factory-native-fixture","actor_kind":"human","token_sha256":"3364f74b11412c13aaa6aeea0c408170c4c935f4b262a50372ce22858c5586db","scope_refs":["control:root"],"actions":["central.document.create"],"expires_at_unix_seconds":expires},
        {"principal_ref":agent_ref,"actor_kind":"agent","token_sha256":"80e9e4523f7eff057168e5e08e5bbac4b81b95a128fd5cc9393363037771f6f3","scope_refs":["control:root"],"actions":["central.receiving.submit"],"expires_at_unix_seconds":expires}
    ]});
    fs::write(authority_path, grants.to_string()).unwrap();
    let relations_path = world
        .dir
        .path()
        .join("Control/relations/source-relations.json");
    let mut relations: Value = serde_json::from_slice(&fs::read(&relations_path).unwrap()).unwrap();
    relations["relations"].as_array_mut().unwrap().push(json!({
        "ref":"central:source:control:root:Control/user/authority.json","path":"Control/user/authority.json",
        "roles":["native-action-authority"],"provenance":"human-adopted","standing":"architecture-contract",
        "treatment":"projectcentral-user","recognition":"disposable-native-test-not-personal-adoption","recorded_at_unix_seconds":1}));
    fs::write(world.dir.path().join("Control/user/time.json"), json!({
        "schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London",
        "day_boundary_minutes":0,"automatic_day_rollover":true}).to_string()).unwrap();
    relations["relations"].as_array_mut().unwrap().push(json!({
        "ref":"central:source:control:root:Control/user/time.json","path":"Control/user/time.json",
        "roles":["civil-time-policy"],"provenance":"human-adopted","standing":"architecture-contract",
        "treatment":"projectcentral-user","recognition":"disposable-native-test-not-personal-adoption","recorded_at_unix_seconds":1}));
    fs::write(relations_path, relations.to_string()).unwrap();
    let invoke = |action: &str, input: Value, token: &str| {
        let output = Command::new(&binary)
            .args(["--json", "--root"])
            .arg(world.dir.path())
            .args(["action", "run", action])
            .arg(input.to_string())
            .env("CENTRAL_NATIVE_TOKEN", token)
            .output()
            .unwrap();
        let response = success(output);
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    };
    let policy = invoke("central.work.policy", json!({}), "");
    let document = invoke(
        "central.document.create",
        json!({
            "kind":"flow","document_id":"document:factory-native-return","title":"Controlled native Return target",
            "expected_policy_revision":policy["revision"],"template_payload":{},"fields":[]
        }),
        "factory-native-central-human-fixture",
    );
    let document_path = world
        .dir
        .path()
        .join(document["source"]["path"].as_str().unwrap());
    let before = fs::read(&document_path).unwrap();
    let preparation_request = prepare_request(&world, &binary, "prepare:native-return");
    let prepared = preparation(&world, &preparation_request);
    assert_eq!(prepared["needsReconciliation"], false, "{prepared}");
    let now_ref =
        &prepared["observation"]["payload"]["detail"]["checkpoint"]["allocation"]["now_ref"];
    // Only the provider turn and verification input are explicit controlled
    // evidence here; Factory and Central are both real native binaries.
    world.mode("returned");
    assert_eq!(
        success(world.owner(&owner_request(&world)))["needsReconciliation"],
        false
    );
    let unit = world.workflow.unit("inspect-source").unwrap();
    let evidence = BTreeSet::from(["evidence:native-join-controlled-result".into()]);
    success(world.action(FactoryAttemptOperation::RecordVerification {
        attempt_ref: ATTEMPT.into(),
        verification: VerificationReceipt {
            verification_ref: "verification:native-join-controlled".into(),
            owner_ref: "factory".into(),
            source_revision: world.workflow.source.revision.clone(),
            outcome: VerificationOutcome::Passed,
            obligations: unit.verification_obligations.clone(),
            evidence_refs: evidence.clone(),
        },
    }));
    success(world.action(FactoryAttemptOperation::ReturnArtifact {
        attempt_ref: ATTEMPT.into(),
        artifact: epilogos_factory::orchestration::ReturnedArtifact {
            artifact_ref: "artifact:native-join-controlled".into(),
            subject_ref: unit.subject_ref.to_string(),
            subject_revision: unit.basis_revision.clone(),
            producing_execution_ref: EXECUTION.into(),
            evidence_refs: evidence.clone(),
            semantic_difference: "Controlled source report for native receiving".into(),
        },
        readable_return: ReadableReturn {
            return_ref: "return:native-join-controlled".into(),
            summary: "Controlled native Return, not human inclusion".into(),
            artifact_refs: BTreeSet::from(["artifact:native-join-controlled".into()]),
            evidence_refs: evidence,
            receiving_ref: None,
            receiving_source_revision: None,
            archive_refs: BTreeSet::new(),
            regression_observation_refs: BTreeSet::new(),
        },
    }));
    let request = json!({"contract":"factory.attempt-receiving-action/v1","requestRef":"receiving:native-join","projectionRef":"projection:native-join",
        "caller":caller(),"runRef":RUN,"expectedRevision":world.reading().revision,"authority":authority(),"attemptRef":ATTEMPT,
        "central":{"binary":binary,"root":world.dir.path(),"contractRevision":CENTRAL_CONTRACT_REVISION,"project":null},
        "target":{"sourceRef":document["source"]["ref"],"documentId":document["document_id"],"sourceRevision":document["revision"]["revision"]}});
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(["attempt", "receiving"])
        .arg(world.state())
        .args(["-", "--json"])
        .env(
            "CENTRAL_NATIVE_TOKEN",
            "factory-native-central-agent-fixture",
        )
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
    let received = success(child.wait_with_output().unwrap());
    assert_eq!(received["needsReconciliation"], false, "{received}");
    let actual = &received["centralResponse"]["data"];
    assert_eq!(actual["record"]["now_ref"], *now_ref);
    assert_eq!(
        actual["record"]["task_ref"],
        world.reading().attempts[0].task_ref
    );
    assert_eq!(actual["record"]["author"]["principal_ref"], agent_ref);
    assert_eq!(actual["included"], false);
    assert_eq!(fs::read(document_path).unwrap(), before);
    assert_eq!(
        world.reading().attempts[0]
            .readable_return
            .as_ref()
            .unwrap()
            .receiving_ref
            .as_deref(),
        actual["return_ref"].as_str()
    );
}
