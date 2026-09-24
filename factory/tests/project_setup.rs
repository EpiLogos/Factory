//! Real owner/filesystem tests. No process or provider output is simulated.
use epilogos_factory::project_setup::{locate, setup, setup_central};
use std::fs;

#[test]
fn setup_is_idempotent_and_refuses_identity_collision_without_changing_state() {
    let root = tempfile::tempdir().unwrap();
    assert!(setup(root.path(), "invalid project key", None).is_err());
    assert!(!root.path().join(".factory").exists());
    let first = setup(root.path(), "control:root", None).unwrap();
    assert_eq!(first["status"], "created");
    assert_eq!(first["runCount"], 0);
    let path = first["statePath"].as_str().unwrap();
    let bytes = fs::read(path).unwrap();
    let metadata = fs::read(root.path().join(".factory/project.json")).unwrap();
    assert_eq!(
        setup(root.path(), "control:root", None).unwrap()["status"],
        "already-present"
    );
    assert_eq!(
        locate(root.path()).unwrap()["projectRef"],
        first["projectRef"]
    );
    assert!(setup(root.path(), "different-project", None).is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert_eq!(
        fs::read(root.path().join(".factory/project.json")).unwrap(),
        metadata
    );
}

#[test]
fn relocated_project_retains_original_link_and_refuses_changed_source() {
    let parent = tempfile::tempdir().unwrap();
    let old = parent.path().join("before");
    let new = parent.path().join("after");
    fs::create_dir_all(old.join("ProjectCentral")).unwrap();
    let source = old.join("ProjectCentral/project.json");
    fs::write(
        &source,
        r#"{"schema":"central.project/v1","project_id":"product"}"#,
    )
    .unwrap();
    let original_source = source.canonicalize().unwrap();
    let original = setup(&old, "product", Some(&source)).unwrap();
    fs::rename(&old, &new).unwrap();
    let relocated_source = new.join("ProjectCentral/project.json");
    let relocated = setup(&new, "product", Some(&relocated_source)).unwrap();
    assert_eq!(original["projectRef"], relocated["projectRef"]);
    let path = relocated["statePath"].as_str().unwrap();
    let bytes = fs::read(path).unwrap();
    let state: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        state["state"]["centralProjectLinkRelocations"][0]["previous"]["sourcePath"],
        original_source.to_str().unwrap()
    );
    assert_eq!(
        state["state"]["centralProjectLinkRelocations"][0]["current"]["sourcePath"],
        relocated_source.canonicalize().unwrap().to_str().unwrap()
    );
    setup(&new, "product", Some(&relocated_source)).unwrap();
    assert_eq!(fs::read(path).unwrap(), bytes);
    fs::write(
        &relocated_source,
        r#"{"schema":"central.project/v1","project_id":"different"}"#,
    )
    .unwrap();
    assert!(setup(&new, "product", Some(&relocated_source)).is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn public_cli_setup_and_locate_return_native_project() {
    let root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_factory"))
            .args(args)
            .output()
            .unwrap()
    };
    let path = root.path().to_str().unwrap();
    let created = run(&["project", "setup", path, "control:root", "--json"]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let located = run(&["project", "locate", path, "--json"]);
    assert!(located.status.success());
    let reading: serde_json::Value = serde_json::from_slice(&located.stdout).unwrap();
    assert_eq!(reading["projectRef"], data["projectRef"]);
    assert_eq!(reading["runCount"], 0);
    assert!(!run(&["project", "setup", path, "foreign", "--json"])
        .status
        .success());
    assert!(String::from_utf8_lossy(&run(&["help"]).stdout).contains("factory project setup"));
}

#[test]
fn typed_first_commission_and_replay_keep_initialized_identity_and_source() {
    let root = tempfile::tempdir().unwrap();
    let request = root.path().join("commission.json");
    let source = root.path().join("work.workflow.ts");
    fs::write(
        &request,
        include_str!("../workflow-sdk/examples/commission.json"),
    )
    .unwrap();
    fs::write(
        &source,
        include_str!("../workflow-sdk/examples/single.workflow.ts"),
    )
    .unwrap();
    let initialized = setup(root.path(), "source-inspection-example", None).unwrap();
    let state_path = initialized["statePath"].as_str().unwrap();
    let commission = || {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_factory"))
            .args([
                "workflow",
                "commission",
                state_path,
                request.to_str().unwrap(),
                source.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let receipt = commission();
    assert_eq!(
        receipt["contract"],
        "factory.workflow-commission-receipt/v1"
    );
    assert_eq!(receipt["execution"], "not-requested");
    let bytes = fs::read(state_path).unwrap();
    let provider =
        epilogos_factory::developmental_read::FactoryDevelopmentalFileProvider::open(state_path)
            .unwrap();
    let state = provider.state();
    assert_eq!(
        state.build.project().reference().to_string(),
        initialized["projectRef"]
    );
    assert_eq!(state.build.run_count(), 1);
    assert_eq!(state.journeys.len(), 1);
    assert_eq!(state.workflow_sources.len(), 1);
    assert_eq!(state.attempt_states.len(), 1);
    commission();
    assert_eq!(fs::read(state_path).unwrap(), bytes);
    // Adoption after a lost placement acknowledgement cannot recreate work.
    fs::remove_file(root.path().join(".factory/project.json")).unwrap();
    assert!(setup(root.path(), "wrong", None).is_err());
    assert!(!root.path().join(".factory/project.json").exists());
    setup(root.path(), "source-inspection-example", None).unwrap();
    assert_eq!(fs::read(state_path).unwrap(), bytes);
}

#[test]
fn public_attempt_action_addresses_one_run_in_a_two_run_native_state() {
    let root = tempfile::tempdir().unwrap();
    let state = setup(root.path(), "source-inspection-example", None).unwrap();
    let state_path = state["statePath"].as_str().unwrap();
    let source = include_str!("../workflow-sdk/examples/single.workflow.ts");
    let base_request: serde_json::Value =
        serde_json::from_str(include_str!("../workflow-sdk/examples/commission.json")).unwrap();
    let invoke = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_factory"))
            .args(args)
            .output()
            .unwrap()
    };
    let success = |output: std::process::Output| {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let mut runs = Vec::new();
    for index in 1..=2 {
        let mut request = base_request.clone();
        request["requestRef"] = format!("commission-request:source-inspection-{index}").into();
        request["runDestination"] = format!("source-inspection/{index}").into();
        request["rootAct"]["actRef"] = format!("act:source-inspection-{index}").into();
        let request_path = root.path().join(format!("commission-{index}.json"));
        fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
        let source_path = root.path().join(format!("work-{index}.workflow.ts"));
        let authored = source
            .replace(
                "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FAV",
                &format!("workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FA{index}"),
            )
            .replace("single-development", &format!("single-development-{index}"));
        fs::write(&source_path, authored).unwrap();
        let receipt = success(invoke(&[
            "workflow",
            "commission",
            state_path,
            request_path.to_str().unwrap(),
            source_path.to_str().unwrap(),
            "--json",
        ]));
        runs.push(
            receipt["commission"]["commission"]["runRef"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_ne!(runs[0], runs[1]);
    let first = success(invoke(&["attempt", "read", state_path, &runs[0], "--json"]));
    let second = success(invoke(&["attempt", "read", state_path, &runs[1], "--json"]));
    assert_eq!(first["revision"], second["revision"]);
    assert!(first["sourceCurrent"].as_bool().unwrap());
    assert!(second["sourceCurrent"].as_bool().unwrap());

    let action = serde_json::json!({
        "contract":"factory.attempt-action/v1",
        "projectionRef":"projection:two-run-exact-target",
        "caller":{"callerRef":"agent:two-run-native-test","projectionKind":"headless",
            "lineage":["agent:two-run-native-test"]},
        "runRef":runs[0], "expectedRevision":first["revision"],
        "authority":{"authorityRef":"authority:two-run-native-test","nativeOwner":"factory",
            "capabilityRef":"capability/factory/operate-attempt","capabilityGranted":true,
            "actionAuthorised":true},
        "operation":{"operation":"advance-subject",
            "subject_ref":"project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
            "revision":"two-run-native-observation"}
    });
    let action_path = root.path().join("attempt-action.json");
    fs::write(&action_path, serde_json::to_vec(&action).unwrap()).unwrap();
    let receipt = success(invoke(&[
        "attempt",
        "action",
        state_path,
        action_path.to_str().unwrap(),
        "--json",
    ]));
    assert_eq!(receipt["runRef"], runs[0]);
    assert_eq!(receipt["previousRevision"], first["revision"]);
    assert_eq!(receipt["operation"], "advance-subject");
    let after = success(invoke(&["attempt", "read", state_path, &runs[1], "--json"]));
    assert_eq!(after["runRef"], runs[1]);
    assert_eq!(after["runRevision"], second["runRevision"]);
    assert!(after["attempts"].as_array().unwrap().is_empty());
}

#[test]
fn native_central_source_is_verified_and_existing_files_survive_setup() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("ProjectCentral")).unwrap();
    let source = root.path().join("ProjectCentral/project.json");
    fs::write(
        &source,
        r#"{"schema":"central.project/v1","project_id":"product"}"#,
    )
    .unwrap();
    assert!(setup(root.path(), "wrong", Some(&source)).is_err());
    assert!(!root.path().join(".factory").exists());
    fs::create_dir(root.path().join(".factory")).unwrap();
    fs::write(
        root.path().join(".factory/authored.workflow.ts"),
        "preserve authored work",
    )
    .unwrap();
    let initialized = setup(root.path(), "product", Some(&source)).unwrap();
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(initialized["statePath"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(state["state"]["journeys"].as_array().unwrap().len(), 0);
    let links = state["state"]["centralProjectLinks"].as_object().unwrap();
    assert_eq!(
        links.values().next().unwrap()["centralProjectRef"],
        "product"
    );
    assert_eq!(
        fs::read_to_string(root.path().join(".factory/authored.workflow.ts")).unwrap(),
        "preserve authored work"
    );
}

#[test]
fn central_names_with_spaces_keep_their_identity_and_get_unambiguous_factory_keys() {
    let parent = tempfile::tempdir().unwrap();
    let mut keys = Vec::new();
    for (index, name) in ["My Project", "My%20Project", "日本語"].iter().enumerate() {
        let root = parent.path().join(index.to_string());
        fs::create_dir_all(root.join("ProjectCentral")).unwrap();
        let source = root.join("ProjectCentral/project.json");
        fs::write(
            &source,
            serde_json::json!({"schema":"central.project/v1","project_id":name}).to_string(),
        )
        .unwrap();
        let result = setup_central(&root, name, &source).unwrap();
        assert_eq!(result["centralProjectRef"], *name);
        let key = result["projectKey"].as_str().unwrap().to_owned();
        assert!(!key.chars().any(char::is_whitespace));
        assert!(!keys.contains(&key));
        keys.push(key);
    }
}

#[test]
fn concurrent_setup_converges_and_incomplete_placement_recovers() {
    let root = tempfile::tempdir().unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let root = root.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                setup(&root, "same", None).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|v| v["status"] == "created").count(),
        1
    );
    let state = results[0]["statePath"].as_str().unwrap();
    let bytes = fs::read(state).unwrap();
    fs::remove_file(root.path().join(".factory/project.json")).unwrap();
    assert_eq!(
        setup(root.path(), "same", None).unwrap()["status"],
        "already-present"
    );
    assert_eq!(fs::read(state).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn redirected_factory_directory_cannot_write_outside_project() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join(".factory")).unwrap();
    assert!(setup(root.path(), "scope", None).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
