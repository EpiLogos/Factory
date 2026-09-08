//! Real files and native Factory state; no mocked provider or transcript.
use epilogos_factory::artifact_evidence::{
    ArtifactEvidenceError, ArtifactLimits, ArtifactSnapshot, SubjectState,
};
use epilogos_factory::build::{CandidateRecord, EvidenceRecord, FactoryBuildState};
use epilogos_factory::core::identity::{Ref, Revision};
use epilogos_factory::core::run::{Project, ProjectRef, Run, RunRef};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const SUBJECT: &str = "candidate:01ARZ3NDEKTSV4RRFFQ69G5FAC";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAB";
fn state(candidate_revision: u64) -> FactoryBuildState {
    let project_ref: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap();
    let run_ref: RunRef = RUN.parse().unwrap();
    let run = Run::new(
        run_ref.clone(),
        project_ref.clone(),
        "Verify the declared artifact",
        "factory-test-owner",
    )
    .unwrap();
    let mut state = FactoryBuildState::new(Project::new(project_ref), run).unwrap();
    state
        .insert_candidate(CandidateRecord {
            run_ref,
            candidate_ref: SUBJECT.into(),
            revision: candidate_revision,
            label: "Artifact under observation".into(),
            status: "unrecognised".into(),
            producing_execution_refs: vec![],
            claim_refs: vec![],
            evidence_refs: vec![],
            artifact_refs: vec!["artifact/source".into()],
            preview_ref: None,
            tradeoffs: vec![],
        })
        .unwrap();
    state
}
fn evidence(id: &str) -> EvidenceRecord {
    EvidenceRecord {
        run_ref: RUN.parse().unwrap(),
        evidence_ref: id.into(),
        label: "Declared source files".into(),
        assessment: None,
        native_ref: None,
        producing_execution_ref: Some("execution:01ARZ3NDEKTSV4RRFFQ69G5FAF".into()),
    }
}
fn capture(root: &Path) -> ArtifactSnapshot {
    ArtifactSnapshot::capture(
        SUBJECT.parse().unwrap(),
        Revision::INITIAL,
        root,
        &[PathBuf::from("artifact")],
        ArtifactLimits::default(),
    )
    .unwrap()
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("artifact")).unwrap();
    fs::write(root.path().join("artifact/source.txt"), "original\n").unwrap();
    root
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn native_admission_rechecks_changed_bytes_with_unchanged_git_head_without_mutation() {
    let root = fixture();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["add", "artifact"]);
    git(
        root.path(),
        &[
            "-c",
            "user.name=Factory Test",
            "-c",
            "user.email=factory-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "source",
        ],
    );
    let head = git(root.path(), &["rev-parse", "HEAD"]);
    let snapshot = capture(root.path());
    // A successful earlier check is deliberately insufficient for admission.
    snapshot
        .validate_current(
            snapshot.subject_state(),
            &SUBJECT.parse().unwrap(),
            Revision::INITIAL,
        )
        .unwrap();
    fs::write(root.path().join("artifact/source.txt"), "modified\n").unwrap();
    assert_eq!(git(root.path(), &["rev-parse", "HEAD"]), head);
    let mut state = state(1);
    let before = serde_json::to_value(&state).unwrap();
    let result = state.insert_artifact_evidence(
        evidence("evidence:01ARZ3NDEKTSV4RRFFQ69G5FAH"),
        &snapshot,
        snapshot.subject_state(),
    );
    assert!(matches!(result, Err(ArtifactEvidenceError::StaleArtifacts)));
    assert_eq!(serde_json::to_value(state).unwrap(), before);
}

#[test]
fn admission_preserves_history_but_does_not_finish_or_recognise_the_run() {
    let root = fixture();
    let snapshot = capture(root.path());
    let mut state = state(1);
    let run_ref: RunRef = RUN.parse().unwrap();
    let lifecycle = state.run(&run_ref).unwrap().lifecycle();
    state
        .insert_artifact_evidence(
            evidence("evidence:01ARZ3NDEKTSV4RRFFQ69G5FAH"),
            &snapshot,
            snapshot.subject_state(),
        )
        .unwrap();
    assert_eq!(state.run(&run_ref).unwrap().lifecycle(), lifecycle);
    let admitted = serde_json::to_value(&state).unwrap();
    assert_eq!(
        admitted["evidence"]["evidence:01ARZ3NDEKTSV4RRFFQ69G5FAH"]["nativeRef"],
        snapshot.subject_state().state_ref
    );
    assert_eq!(
        admitted["evidence"]["evidence:01ARZ3NDEKTSV4RRFFQ69G5FAH"]["assessment"],
        "current-declared-artifact-scope"
    );
    assert_eq!(admitted["candidates"][SUBJECT]["status"], "unrecognised");
    // A new untracked member matters even though all original bytes are intact.
    fs::write(
        root.path().join("artifact/additional.txt"),
        "new requirement",
    )
    .unwrap();
    assert!(matches!(
        state.insert_artifact_evidence(
            evidence("evidence:01ARZ3NDEKTSV4RRFFQ69G5FAJ"),
            &snapshot,
            snapshot.subject_state()
        ),
        Err(ArtifactEvidenceError::StaleArtifacts)
    ));
    assert_eq!(serde_json::to_value(&state).unwrap(), admitted);
    let restored: FactoryBuildState = serde_json::from_value(admitted.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), admitted);
}

#[test]
fn native_candidate_revision_and_assessment_binding_cannot_be_overridden_by_caller() {
    let root = fixture();
    let snapshot = capture(root.path());
    let mut advanced = state(2);
    let before = serde_json::to_value(&advanced).unwrap();
    assert!(matches!(
        advanced.insert_artifact_evidence(
            evidence("evidence:01ARZ3NDEKTSV4RRFFQ69G5FAH"),
            &snapshot,
            snapshot.subject_state()
        ),
        Err(ArtifactEvidenceError::StaleSubject)
    ));
    assert_eq!(serde_json::to_value(advanced).unwrap(), before);
    let mut other = snapshot.subject_state().clone();
    other.state_ref.push_str("-not-this-observation");
    assert!(matches!(
        state(1).insert_artifact_evidence(
            evidence("evidence:01ARZ3NDEKTSV4RRFFQ69G5FAH"),
            &snapshot,
            &other
        ),
        Err(ArtifactEvidenceError::AssessmentSubjectMismatch)
    ));
}

#[test]
fn digest_covers_actual_scope_and_permissions_and_leaves_unselected_material_out() {
    let root = fixture();
    let snapshot = capture(root.path());
    fs::write(root.path().join("outside.txt"), "outside declared coverage").unwrap();
    assert_eq!(
        snapshot.subject_state(),
        capture(root.path()).subject_state()
    );
    let single = ArtifactSnapshot::capture(
        SUBJECT.parse().unwrap(),
        Revision::INITIAL,
        root.path(),
        &["artifact/source.txt".into()],
        ArtifactLimits::default(),
    )
    .unwrap();
    assert_ne!(
        single.subject_state(),
        snapshot.subject_state(),
        "directory coverage and a selected file are different evidence scopes"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            root.path().join("artifact/source.txt"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(matches!(
            snapshot.validate_current(
                snapshot.subject_state(),
                &SUBJECT.parse().unwrap(),
                Revision::INITIAL
            ),
            Err(ArtifactEvidenceError::StaleArtifacts)
        ));
    }
}

#[test]
fn bounded_scope_refuses_traversal_overlap_and_large_actual_files() {
    let root = fixture();
    let subject: Ref = SUBJECT.parse().unwrap();
    for paths in [
        vec![PathBuf::from("../outside")],
        vec!["artifact".into(), "artifact/source.txt".into()],
        vec![],
    ] {
        assert!(matches!(
            ArtifactSnapshot::capture(
                subject.clone(),
                Revision::INITIAL,
                root.path(),
                &paths,
                ArtifactLimits::default()
            ),
            Err(ArtifactEvidenceError::InvalidScope(_))
        ));
    }
    let limits = ArtifactLimits {
        max_bytes: 3,
        ..ArtifactLimits::default()
    };
    assert!(matches!(
        ArtifactSnapshot::capture(
            subject,
            Revision::INITIAL,
            root.path(),
            &["artifact".into()],
            limits
        ),
        Err(ArtifactEvidenceError::LimitExceeded)
    ));
}

#[cfg(unix)]
#[test]
fn selected_symlink_and_symlink_ancestor_are_rejected() {
    let root = fixture();
    std::os::unix::fs::symlink("source.txt", root.path().join("artifact/link")).unwrap();
    assert!(matches!(
        ArtifactSnapshot::capture(
            SUBJECT.parse().unwrap(),
            Revision::INITIAL,
            root.path(),
            &["artifact".into()],
            ArtifactLimits::default()
        ),
        Err(ArtifactEvidenceError::UnsupportedArtifact(_))
    ));
    std::os::unix::fs::symlink("artifact", root.path().join("alias")).unwrap();
    assert!(matches!(
        ArtifactSnapshot::capture(
            SUBJECT.parse().unwrap(),
            Revision::INITIAL,
            root.path(),
            &["alias/source.txt".into()],
            ArtifactLimits::default()
        ),
        Err(ArtifactEvidenceError::UnsupportedArtifact(_))
    ));
}

#[test]
fn subject_state_uses_existing_exact_interop_shape() {
    let value: serde_json::Value = serde_json::from_str(include_str!(
        "../../contracts/factory/fixtures/interop/evidenceAssessment.json"
    ))
    .unwrap();
    let state: SubjectState =
        serde_json::from_value(value["evidenceEnvelope"]["subjectState"].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        value["evidenceEnvelope"]["subjectState"]
    );
}
