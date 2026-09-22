use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, FactoryActionProjectionReceipt,
    FactoryActionProjectionRequest, ProjectedFactoryActionAuthority,
    FACTORY_ACTION_PROJECTION_CONTRACT,
};
use epilogos_factory::build::{
    CandidateRecord, FactoryBuildSelection, FactoryBuildSnapshot, FactoryBuildState,
    REQUEST_MORE_EVIDENCE_ACTION_REF, REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
};
use epilogos_factory::build_provider::FactoryBuildFileProvider;
use epilogos_factory::core::run::{Project, ProjectRef, Run, RunRef};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FCA";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FCB";
const CANDIDATE: &str = "candidate:01ARZ3NDEKTSV4RRFFQ69G5FCC";
const AUTHORITY: &str = "authority/factory-cli-parity";

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn state() -> (FactoryBuildState, FactoryBuildSelection) {
    let project_ref = ProjectRef::from_str(PROJECT).unwrap();
    let run_ref = RunRef::from_str(RUN).unwrap();
    let project = Project::new(project_ref.clone());
    let run = Run::new(
        run_ref.clone(),
        project_ref.clone(),
        "Prove the native Factory command",
        "factory-owner",
    )
    .unwrap();
    let mut state = FactoryBuildState::new(project, run).unwrap();
    state
        .insert_candidate(CandidateRecord {
            run_ref: run_ref.clone(),
            candidate_ref: CANDIDATE.into(),
            revision: 1,
            label: "Native CLI parity Candidate".into(),
            status: "ready".into(),
            producing_execution_refs: Vec::new(),
            claim_refs: Vec::new(),
            evidence_refs: vec!["evidence:native-cli-parity".into()],
            artifact_refs: Vec::new(),
            preview_ref: None,
            tradeoffs: Vec::new(),
        })
        .unwrap();
    (
        state,
        FactoryBuildSelection {
            project_ref,
            run_ref,
        },
    )
}

fn temp_state_path(label: &str) -> PathBuf {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "factory-native-cli-{}-{serial}-{label}.json",
        std::process::id()
    ))
}

fn create_state(label: &str) -> (PathBuf, FactoryBuildSelection) {
    let (state, selection) = state();
    let path = temp_state_path(label);
    let _ = fs::remove_file(&path);
    FactoryBuildFileProvider::create(&path, selection.clone(), state).unwrap();
    (path, selection)
}

fn request() -> FactoryActionProjectionRequest {
    FactoryActionProjectionRequest {
        contract: FACTORY_ACTION_PROJECTION_CONTRACT.into(),
        projection_ref: "projection:headless/factory-native-cli".into(),
        caller: FactoryActionCaller {
            caller_ref: "automation:factory-native-cli".into(),
            projection_kind: FactoryActionProjectionKind::Headless,
            lineage: vec!["automation:factory-native-cli".into()],
        },
        action_ref: REQUEST_MORE_EVIDENCE_ACTION_REF.into(),
        subject_ref: CANDIDATE.into(),
        run_ref: RUN.into(),
        authority: ProjectedFactoryActionAuthority {
            authority_ref: AUTHORITY.into(),
            native_owner: "factory".into(),
            capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
            capability_granted: true,
            action_authorised: true,
        },
    }
}

fn run_binary(binary: &str, args: &[String], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn factory(args: &[String], stdin: Option<&[u8]>) -> Output {
    run_binary(env!("CARGO_BIN_EXE_factory"), args, stdin)
}

#[test]
fn snapshot_refresh_and_action_listing_are_native_provider_projections() {
    let (path, _) = create_state("read");
    let path = path.to_string_lossy().into_owned();

    let snapshot_output = factory(
        &[
            "build".into(),
            "snapshot".into(),
            path.clone(),
            PROJECT.into(),
            RUN.into(),
            "--json".into(),
        ],
        None,
    );
    assert!(
        snapshot_output.status.success(),
        "factory snapshot failed: {}",
        String::from_utf8_lossy(&snapshot_output.stderr)
    );
    let snapshot: FactoryBuildSnapshot = serde_json::from_slice(&snapshot_output.stdout).unwrap();

    let refresh_output = factory(
        &[
            "build".into(),
            "refresh".into(),
            path.clone(),
            PROJECT.into(),
            RUN.into(),
            "--json".into(),
        ],
        None,
    );
    assert!(refresh_output.status.success());
    let refreshed: FactoryBuildSnapshot = serde_json::from_slice(&refresh_output.stdout).unwrap();
    assert_eq!(refreshed, snapshot);

    let actions_output = factory(
        &[
            "action".into(),
            "list".into(),
            path.clone(),
            PROJECT.into(),
            RUN.into(),
            "--json".into(),
        ],
        None,
    );
    assert!(actions_output.status.success());
    let actions: serde_json::Value = serde_json::from_slice(&actions_output.stdout).unwrap();
    assert_eq!(
        actions,
        serde_json::to_value(&snapshot.view.actions).unwrap(),
        "native CLI Action inventory must be the selected Build snapshot inventory"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn conformance_developmental_state_drives_build_refresh_and_action_listing() {
    // The document `factory conformance developmental-state` writes is the
    // documented provider state for exercising the real binary. `action list`
    // and `build snapshot`/`refresh` must serve it through its owning
    // developmental provider instead of failing as a JSON schema error
    // (`missing field 'project'`).
    let path = temp_state_path("conformance-read");
    let _ = fs::remove_file(&path);
    let state_path = path.to_string_lossy().into_owned();

    let generated = factory(
        &[
            "conformance".into(),
            "developmental-state".into(),
            state_path.clone(),
            "--json".into(),
        ],
        None,
    );
    assert!(
        generated.status.success(),
        "conformance generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let manifest: serde_json::Value = serde_json::from_slice(&generated.stdout).unwrap();
    let project = manifest["projectRef"].as_str().unwrap().to_string();
    let run = manifest["runRef"].as_str().unwrap().to_string();

    let snapshot_output = factory(
        &[
            "build".into(),
            "snapshot".into(),
            state_path.clone(),
            project.clone(),
            run.clone(),
            "--json".into(),
        ],
        None,
    );
    assert!(
        snapshot_output.status.success(),
        "build snapshot failed against the conformance state: {}",
        String::from_utf8_lossy(&snapshot_output.stderr)
    );
    let snapshot: FactoryBuildSnapshot = serde_json::from_slice(&snapshot_output.stdout).unwrap();
    assert_eq!(snapshot.view.run.run_ref, run);
    assert!(!snapshot.view.actions.is_empty());

    let refresh_output = factory(
        &[
            "build".into(),
            "refresh".into(),
            state_path.clone(),
            project.clone(),
            run.clone(),
            "--json".into(),
        ],
        None,
    );
    assert!(
        refresh_output.status.success(),
        "build refresh failed against the conformance state: {}",
        String::from_utf8_lossy(&refresh_output.stderr)
    );

    let actions_output = factory(
        &[
            "action".into(),
            "list".into(),
            state_path.clone(),
            project,
            run,
            "--json".into(),
        ],
        None,
    );
    assert!(
        actions_output.status.success(),
        "action list failed against the conformance state: {}",
        String::from_utf8_lossy(&actions_output.stderr)
    );
    let actions: serde_json::Value = serde_json::from_slice(&actions_output.stdout).unwrap();
    assert_eq!(
        actions,
        serde_json::to_value(&snapshot.view.actions).unwrap(),
        "Action listing against the developmental state must be the selected Build snapshot inventory"
    );
    assert!(actions
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["actionRef"] == REQUEST_MORE_EVIDENCE_ACTION_REF));

    // The document on disk must still be the developmental provider state:
    // serving it through the Build/Action commands must not have converted it.
    let persisted: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        persisted["schema"],
        epilogos_factory::developmental_read::FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER
    );

    let _ = fs::remove_file(path);
}

#[test]
fn wrong_state_document_kinds_are_refused_by_name() {
    // The conformance manifest (the command's stdout) is a locator document,
    // not a provider state: feeding it back must be refused by naming the
    // manifest, never as a bare serde field error.
    let state_path = temp_state_path("refusal-provider");
    let _ = fs::remove_file(&state_path);
    let generated = factory(
        &[
            "conformance".into(),
            "developmental-state".into(),
            state_path.to_string_lossy().into_owned(),
            "--json".into(),
        ],
        None,
    );
    assert!(generated.status.success());

    let manifest_path = temp_state_path("refusal-manifest");
    fs::write(&manifest_path, &generated.stdout).unwrap();
    let refused = factory(
        &[
            "action".into(),
            "list".into(),
            manifest_path.to_string_lossy().into_owned(),
            PROJECT.into(),
            RUN.into(),
        ],
        None,
    );
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("developmental-conformance-manifest")
            && stderr.contains("not a provider state"),
        "manifest refusal must name the document kind: {stderr}"
    );

    let unknown_path = temp_state_path("refusal-unknown-schema");
    fs::write(&unknown_path, r#"{"schema":"example.other-document/v1"}"#).unwrap();
    let refused = factory(
        &[
            "action".into(),
            "list".into(),
            unknown_path.to_string_lossy().into_owned(),
            PROJECT.into(),
            RUN.into(),
        ],
        None,
    );
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("example.other-document/v1"),
        "unknown-schema refusal must name the found schema: {stderr}"
    );

    let bare_path = temp_state_path("refusal-no-schema");
    fs::write(&bare_path, r#"{"contract":"example.bare/v1"}"#).unwrap();
    let refused = factory(
        &[
            "action".into(),
            "list".into(),
            bare_path.to_string_lossy().into_owned(),
            PROJECT.into(),
            RUN.into(),
        ],
        None,
    );
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("no top-level `schema`"),
        "schema-less refusal must say the document carries no schema: {stderr}"
    );

    let _ = fs::remove_file(state_path);
    let _ = fs::remove_file(manifest_path);
    let _ = fs::remove_file(unknown_path);
    let _ = fs::remove_file(bare_path);
}

#[test]
fn native_factory_action_matches_legacy_headless_and_persists_caller_lineage() {
    let (native_path, native_selection) = create_state("native-action");
    let (legacy_path, _) = create_state("legacy-action");
    let input = serde_json::to_vec(&request()).unwrap();

    let native_output = factory(
        &[
            "action".into(),
            "invoke".into(),
            native_path.to_string_lossy().into_owned(),
            PROJECT.into(),
            RUN.into(),
            "-".into(),
            "--json".into(),
        ],
        Some(&input),
    );
    assert!(
        native_output.status.success(),
        "factory action invoke failed: {}",
        String::from_utf8_lossy(&native_output.stderr)
    );
    let native: FactoryActionProjectionReceipt =
        serde_json::from_slice(&native_output.stdout).unwrap();

    let legacy_output = run_binary(
        env!("CARGO_BIN_EXE_factory-action-headless"),
        &[
            legacy_path.to_string_lossy().into_owned(),
            PROJECT.into(),
            RUN.into(),
        ],
        Some(&input),
    );
    assert!(
        legacy_output.status.success(),
        "legacy headless action failed: {}",
        String::from_utf8_lossy(&legacy_output.stderr)
    );
    let legacy: FactoryActionProjectionReceipt =
        serde_json::from_slice(&legacy_output.stdout).unwrap();

    assert_eq!(native.contract, legacy.contract);
    assert_eq!(native.action_ref, legacy.action_ref);
    assert_eq!(native.subject_ref, legacy.subject_ref);
    assert_eq!(native.run_ref, legacy.run_ref);
    assert_eq!(native.authority_ref, legacy.authority_ref);
    assert_eq!(native.caller, legacy.caller);
    assert_eq!(native.native_result, legacy.native_result);
    assert_eq!(native.caller.lineage, vec!["automation:factory-native-cli"]);

    let reopened = FactoryBuildFileProvider::open(&native_path, native_selection).unwrap();
    let returned = reopened.snapshot().unwrap();
    assert_eq!(returned.revision, native.native_result.next_revision);
    assert_eq!(returned.view.human_requests.len(), 1);
    assert_eq!(
        returned.view.human_requests[0].human_request_ref,
        native.native_result.created_human_request_ref
    );

    let _ = fs::remove_file(native_path);
    let _ = fs::remove_file(legacy_path);
}

#[test]
fn native_factory_command_preserves_native_failure_semantics_and_deterministic_verify() {
    let (path, _) = create_state("rejected");
    let mut denied = request();
    denied.authority.action_authorised = false;
    let input = serde_json::to_vec(&denied).unwrap();

    let denied_output = factory(
        &[
            "action".into(),
            "invoke".into(),
            path.to_string_lossy().into_owned(),
            PROJECT.into(),
            RUN.into(),
            "-".into(),
            "--json".into(),
        ],
        Some(&input),
    );
    assert!(!denied_output.status.success());
    assert_eq!(denied_output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&denied_output.stderr).contains("MissingActionAuthority"),
        "CLI must surface the native Factory rejection rather than inventing a CLI error"
    );

    let verify_args = vec!["verify".into(), "--json".into()];
    let first = factory(&verify_args, None);
    let second = factory(&verify_args, None);
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let value: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["providerStateChecked"], false);

    let _ = fs::remove_file(path);
}
