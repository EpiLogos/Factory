use epilogos_factory::build::{
    CandidateRecord, FactoryActionAuthority, FactoryActionInvocation, FactoryBuildSelection,
    FactoryBuildState, FACTORY_NATIVE_OWNER, REQUEST_MORE_EVIDENCE_ACTION_REF,
    REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
};
use epilogos_factory::build_provider::FactoryBuildFileProvider;
use epilogos_factory::core::run::{Project, ProjectRef, Run, RunRef};
use std::fs;
use std::str::FromStr;

const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FCA";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FCB";
const CANDIDATE: &str = "candidate:01ARZ3NDEKTSV4RRFFQ69G5FCC";

#[test]
fn local_provider_persists_canonical_mutation_and_reopens_at_new_revision() {
    let project_ref = ProjectRef::from_str(PROJECT).unwrap();
    let run_ref = RunRef::from_str(RUN).unwrap();
    let project = Project::new(project_ref.clone());
    let run = Run::new(
        run_ref.clone(),
        project_ref.clone(),
        "Persist the Factory-owned local Build provider",
        "factory-provider-test",
    )
    .unwrap();
    let mut state = FactoryBuildState::new(project, run).unwrap();
    state
        .insert_candidate(CandidateRecord {
            run_ref: run_ref.clone(),
            candidate_ref: CANDIDATE.into(),
            revision: 1,
            label: "Persistent Candidate".into(),
            status: "ready".into(),
            producing_execution_refs: Vec::new(),
            claim_refs: Vec::new(),
            evidence_refs: Vec::new(),
            artifact_refs: Vec::new(),
            preview_ref: None,
            tradeoffs: Vec::new(),
        })
        .unwrap();
    let selection = FactoryBuildSelection {
        project_ref,
        run_ref: run_ref.clone(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");

    let mut provider = FactoryBuildFileProvider::create(&path, selection.clone(), state).unwrap();
    let before = provider.snapshot().unwrap();
    assert!(before.view.human_requests.is_empty());

    let receipt = provider
        .execute_action(
            &FactoryActionInvocation {
                action_ref: REQUEST_MORE_EVIDENCE_ACTION_REF.into(),
                subject_ref: CANDIDATE.into(),
                run_ref,
            },
            &FactoryActionAuthority {
                authority_ref: "authority/provider-test".into(),
                native_owner: FACTORY_NATIVE_OWNER.into(),
                capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
        )
        .unwrap();
    let after = provider.snapshot().unwrap();
    assert!(after.revision > before.revision);
    assert_eq!(receipt.next_revision, after.revision);
    assert_eq!(after.view.human_requests.len(), 1);

    drop(provider);
    let reopened = FactoryBuildFileProvider::open(&path, selection).unwrap();
    let persisted = reopened.snapshot().unwrap();
    assert_eq!(persisted.revision, after.revision);
    assert_eq!(persisted.view.human_requests, after.view.human_requests);

    fs::remove_file(path).unwrap();
}

fn pair_state() -> (FactoryBuildSelection, FactoryBuildState) {
    let project_ref: ProjectRef = PROJECT.parse().unwrap();
    let run_ref: RunRef = RUN.parse().unwrap();
    let run = Run::new(
        run_ref.clone(),
        project_ref.clone(),
        "Concurrent Build Actions",
        "factory",
    )
    .unwrap();
    let mut state = FactoryBuildState::new(Project::new(project_ref.clone()), run).unwrap();
    for candidate_ref in ["candidate:first", "candidate:second"] {
        state
            .insert_candidate(CandidateRecord {
                run_ref: run_ref.clone(),
                candidate_ref: candidate_ref.into(),
                revision: 1,
                label: candidate_ref.into(),
                status: "ready".into(),
                producing_execution_refs: vec![],
                claim_refs: vec![],
                evidence_refs: vec![],
                artifact_refs: vec![],
                preview_ref: None,
                tradeoffs: vec![],
            })
            .unwrap();
    }
    (
        FactoryBuildSelection {
            project_ref,
            run_ref,
        },
        state,
    )
}
fn authority() -> FactoryActionAuthority {
    FactoryActionAuthority {
        authority_ref: "authority/concurrent-provider-test".into(),
        native_owner: FACTORY_NATIVE_OWNER.into(),
        capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
        capability_granted: true,
        action_authorised: true,
    }
}
fn invocation(candidate: &str) -> FactoryActionInvocation {
    FactoryActionInvocation {
        action_ref: REQUEST_MORE_EVIDENCE_ACTION_REF.into(),
        subject_ref: candidate.into(),
        run_ref: RUN.parse().unwrap(),
    }
}
#[test]
fn already_open_handles_apply_to_current_owner_state_and_refuse_replay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let (selection, state) = pair_state();
    let mut first = FactoryBuildFileProvider::create(&path, selection.clone(), state).unwrap();
    let mut second = FactoryBuildFileProvider::open(&path, selection.clone()).unwrap();
    let mut stale_replay = FactoryBuildFileProvider::open(&path, selection.clone()).unwrap();
    let a = first
        .execute_action(&invocation("candidate:first"), &authority())
        .unwrap();
    let b = second
        .execute_action(&invocation("candidate:second"), &authority())
        .unwrap();
    assert_eq!(b.previous_revision, a.next_revision);
    let before_replay = fs::read(&path).unwrap();
    let rejected = stale_replay
        .execute_action(&invocation("candidate:first"), &authority())
        .unwrap_err();
    assert!(matches!(
        rejected,
        epilogos_factory::build_provider::FactoryBuildProviderError::Factory(
            epilogos_factory::build::FactoryBuildError::ActionAlreadyApplied(_)
        )
    ));
    assert_eq!(fs::read(&path).unwrap(), before_replay);
    let current = FactoryBuildFileProvider::open(&path, selection)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(current.view.human_requests.len(), 2);
    assert_eq!(current.revision, b.next_revision);
}
#[test]
fn create_refuses_existing_state_without_changing_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let (selection, state) = pair_state();
    FactoryBuildFileProvider::create(&path, selection.clone(), state.clone()).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(FactoryBuildFileProvider::create(&path, selection, state).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
#[test]
fn rejected_refresh_keeps_the_previously_valid_cached_reading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let (selection, state) = pair_state();
    let mut provider = FactoryBuildFileProvider::create(&path, selection, state).unwrap();
    let before = provider.snapshot().unwrap();
    let other_project: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
    let other_run: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FAX".parse().unwrap();
    let other = FactoryBuildState::new(
        Project::new(other_project.clone()),
        Run::new(other_run, other_project, "foreign", "factory").unwrap(),
    )
    .unwrap();
    let foreign = serde_json::json!({"schema":epilogos_factory::build_provider::FACTORY_BUILD_LOCAL_PROVIDER_STATE,"state":other});
    fs::write(&path, serde_json::to_vec(&foreign).unwrap()).unwrap();
    assert!(provider.refresh().is_err());
    assert_eq!(provider.snapshot().unwrap(), before);
}
#[test]
fn unknown_stored_fields_are_not_silently_dropped_by_an_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let (selection, state) = pair_state();
    let mut provider = FactoryBuildFileProvider::create(&path, selection, state).unwrap();
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["unrecognisedOwnerExtension"] = serde_json::json!({"retained":"source:legacy"});
    let before = serde_json::to_vec_pretty(&raw).unwrap();
    fs::write(&path, &before).unwrap();
    let cached = provider.snapshot().unwrap();
    let error = provider
        .execute_action(&invocation("candidate:first"), &authority())
        .unwrap_err();
    assert!(error.to_string().contains("unrecognisedOwnerExtension"));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(provider.snapshot().unwrap(), cached);
}
#[cfg(unix)]
#[test]
fn real_temporary_publication_failure_keeps_bytes_and_cached_state() {
    use std::os::unix::fs::PermissionsExt;
    struct Restore(std::path::PathBuf, fs::Permissions);
    impl Drop for Restore {
        fn drop(&mut self) {
            fs::set_permissions(&self.0, self.1.clone()).unwrap();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let (selection, state) = pair_state();
    let mut provider = FactoryBuildFileProvider::create(&path, selection, state).unwrap();
    let cached = provider.snapshot().unwrap();
    let before = fs::read(&path).unwrap();
    let restore = Restore(
        dir.path().to_owned(),
        fs::metadata(dir.path()).unwrap().permissions(),
    );
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let error = provider
        .execute_action(&invocation("candidate:first"), &authority())
        .unwrap_err();
    assert!(error.to_string().contains("I/O"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(provider.snapshot().unwrap(), cached);
    drop(restore);
    provider
        .execute_action(&invocation("candidate:first"), &authority())
        .unwrap();
    assert_eq!(provider.snapshot().unwrap().view.human_requests.len(), 1);
}

#[test]
fn explicit_empty_v1_fields_survive_a_canonical_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let (selection, state) = pair_state();
    let mut provider = FactoryBuildFileProvider::create(&path, selection, state).unwrap();
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["state"]["candidates"]["candidate:first"]["previewRef"] = serde_json::Value::Null;
    raw["emptyRetainedExtension"] = serde_json::json!({});
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    provider
        .execute_action(&invocation("candidate:first"), &authority())
        .unwrap();
    let current: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(current["state"]["candidates"]["candidate:first"]
        .as_object()
        .unwrap()
        .contains_key("previewRef"));
    assert_eq!(
        current["state"]["candidates"]["candidate:first"]["previewRef"],
        serde_json::Value::Null
    );
    assert_eq!(current["emptyRetainedExtension"], serde_json::json!({}));
}

#[cfg(unix)]
#[test]
fn actual_build_action_preserves_private_mode_and_refuses_read_only_source() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let (selection, state) = pair_state();
    let mut provider = FactoryBuildFileProvider::create(&path, selection, state).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::metadata(&path).unwrap();
    provider
        .execute_action(&invocation("candidate:first"), &authority())
        .unwrap();
    let after = fs::metadata(&path).unwrap();
    assert_eq!(
        (after.mode() & 0o7777, after.uid(), after.gid()),
        (before.mode() & 0o7777, before.uid(), before.gid())
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let retained = fs::read(&path).unwrap();
    let cached = provider.snapshot().unwrap();
    assert!(provider
        .execute_action(&invocation("candidate:second"), &authority())
        .is_err());
    assert_eq!(fs::read(&path).unwrap(), retained);
    assert_eq!(provider.snapshot().unwrap(), cached);
}
