use epilogos_factory::core::run::RunRef;
use epilogos_factory::project_development::{
    BoundedIntentCondition, BoundedIntentReturn, IntentCriterionEvaluation, IntentCriterionState,
    ProjectDevelopmentLedger,
};
use epilogos_factory::project_development_store::{
    FileProjectDevelopmentStore, ProjectDevelopmentStore,
};
use std::fs;
use ulid::Ulid;

fn run_ref() -> RunRef {
    "run:01ARZ3NDEKTSV4RRFFQ69G5FAV"
        .parse()
        .expect("valid deterministic RunRef")
}

#[test]
fn developmental_context_survives_owner_store_reload() {
    let run_ref = run_ref();
    let mut ledger = ProjectDevelopmentLedger::new(run_ref.clone());
    ledger
        .set_intent(BoundedIntentCondition {
            run_ref: run_ref.clone(),
            condition_ref: "condition:bounded-intent".to_owned(),
            intent_source_ref: "source:intent".to_owned(),
            focus_ref: Some("focus:current".to_owned()),
            success_condition_refs: vec!["success:context-returned".to_owned()],
            constraint_refs: vec!["constraint:preserve-owner-refs".to_owned()],
            context_resolution_ref: "context-resolution:project-a".to_owned(),
        })
        .expect("bounded Intent is valid");
    ledger
        .set_intent_return(BoundedIntentReturn {
            run_ref: run_ref.clone(),
            return_ref: "return:recognized".to_owned(),
            intent_source_ref: "source:intent".to_owned(),
            context_resolution_ref: "context-resolution:project-a".to_owned(),
            artifact_refs: vec!["artifact:one".to_owned()],
            claim_refs: vec!["claim:one".to_owned()],
            evidence_refs: vec!["evidence:whole".to_owned()],
            criterion_evaluations: vec![IntentCriterionEvaluation {
                criterion_ref: "success:context-returned".to_owned(),
                state: IntentCriterionState::Satisfied,
                evidence_refs: vec!["evidence:criterion".to_owned()],
            }],
        })
        .expect("returned reality matches the bounded Intent");

    let root = std::env::temp_dir().join(format!(
        "epilogos-factory-project-development-{}",
        Ulid::new()
    ));
    let first_process = FileProjectDevelopmentStore::new(&root);
    first_process
        .save(&ledger)
        .expect("owner store persists ledger");
    drop(first_process);

    let reloaded_process = FileProjectDevelopmentStore::new(&root);
    let reloaded = reloaded_process
        .load(&run_ref)
        .expect("owner store reload succeeds")
        .expect("run-scoped ledger exists");

    assert_eq!(reloaded, ledger);
    assert_eq!(
        reloaded.intent.as_ref().unwrap().context_resolution_ref,
        "context-resolution:project-a"
    );
    assert_eq!(
        reloaded.intent_return.as_ref().unwrap().return_ref,
        "return:recognized"
    );

    fs::remove_dir_all(root).expect("test store cleanup");
}

#[test]
fn missing_run_is_not_fabricated() {
    let root = std::env::temp_dir().join(format!(
        "epilogos-factory-project-development-missing-{}",
        Ulid::new()
    ));
    let store = FileProjectDevelopmentStore::new(root);
    assert!(store
        .load(&run_ref())
        .expect("missing provider state is a valid observation")
        .is_none());
}

fn bounded_intent(run: &RunRef) -> BoundedIntentCondition {
    BoundedIntentCondition {
        run_ref: run.clone(),
        condition_ref: "condition:retained-intent".into(),
        intent_source_ref: "source:authored-intent".into(),
        focus_ref: None,
        success_condition_refs: vec!["condition:success".into()],
        constraint_refs: vec![],
        context_resolution_ref: "context:operative".into(),
    }
}
fn observation(
    run: &RunRef,
    reference: &str,
) -> epilogos_factory::project_development::DevelopmentObservation {
    epilogos_factory::project_development::DevelopmentObservation {
        run_ref: run.clone(),
        observation_ref: reference.into(),
        kind:
            epilogos_factory::project_development::DevelopmentObservationKind::InsufficientEvidence,
        statement: "Source-bound retained observation".into(),
        subject_refs: vec!["source:native-owner".into()],
        evidence_refs: vec!["evidence:actual-store".into()],
        owner_return: None,
    }
}
#[test]
fn stale_legacy_save_cannot_replace_retained_intent_when_appending_observation() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileProjectDevelopmentStore::new(dir.path());
    let run = run_ref();
    let mut current = ProjectDevelopmentLedger::new(run.clone());
    let mut stale = current.clone();
    current.set_intent(bounded_intent(&run)).unwrap();
    store.save(&current).unwrap();
    let path = dir.path().join(format!("{}.json", run.as_ref().id()));
    let before = fs::read(&path).unwrap();
    stale
        .add_observation(observation(&run, "observation:stale"))
        .unwrap();
    assert!(store.save(&stale).is_err());
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(store.load(&run).unwrap().unwrap(), current);
}
#[test]
fn two_open_owners_serialize_metadata_and_observation_on_current_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_ref();
    let first = FileProjectDevelopmentStore::new(dir.path());
    first
        .save(&ProjectDevelopmentLedger::new(run.clone()))
        .unwrap();
    let second = FileProjectDevelopmentStore::new(dir.path());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let first_barrier = barrier.clone();
    let first_run = run.clone();
    let worker = std::thread::spawn(move || {
        first_barrier.wait();
        first.transact(&first_run, false, |ledger| {
            ledger.set_intent(bounded_intent(&first_run)).map_err(|error|
                epilogos_factory::project_development_store::ProjectDevelopmentStoreError::Native(error.to_string()))
        }).unwrap()
    });
    barrier.wait();
    second.transact(&run, false, |ledger| {
        ledger.add_observation(observation(&run,"observation:concurrent")).map_err(|error|
            epilogos_factory::project_development_store::ProjectDevelopmentStoreError::Native(error.to_string()))
    }).unwrap();
    worker.join().unwrap();
    drop(second);
    let reopened = FileProjectDevelopmentStore::new(dir.path())
        .load(&run)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.intent, Some(bounded_intent(&run)));
    assert_eq!(
        reopened.observations,
        vec![observation(&run, "observation:concurrent")]
    );
}
#[test]
fn failed_owner_operation_and_unknown_extension_leave_exact_retained_bytes() {
    use epilogos_factory::project_development_store::ProjectDevelopmentStoreError;
    let dir = tempfile::tempdir().unwrap();
    let run = run_ref();
    let store = FileProjectDevelopmentStore::new(dir.path());
    store
        .save(&ProjectDevelopmentLedger::new(run.clone()))
        .unwrap();
    let path = dir.path().join(format!("{}.json", run.as_ref().id()));
    let before = fs::read(&path).unwrap();
    assert!(store
        .transact(&run, false, |ledger| {
            ledger.set_intent(bounded_intent(&run)).unwrap();
            Err(ProjectDevelopmentStoreError::Native(
                "semantic rejection before commit".into(),
            ))
        })
        .is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    let mut raw: serde_json::Value = serde_json::from_slice(&before).unwrap();
    raw["unrecognisedOwnerExtension"] = serde_json::json!({"retained":"source:legacy"});
    let extended = serde_json::to_vec_pretty(&raw).unwrap();
    fs::write(&path, &extended).unwrap();
    assert!(store.load(&run).unwrap().is_some());
    let error = store
        .transact(&run, false, |ledger| {
            ledger
                .add_observation(observation(&run, "observation:extension"))
                .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))
        })
        .unwrap_err();
    assert!(error.to_string().contains("unrecognisedOwnerExtension"));
    assert_eq!(fs::read(&path).unwrap(), extended);
}
#[test]
fn transaction_creation_policy_and_foreign_run_are_enforced_before_publish() {
    use epilogos_factory::project_development_store::ProjectDevelopmentStoreError;
    let dir = tempfile::tempdir().unwrap();
    let run = run_ref();
    let store = FileProjectDevelopmentStore::new(dir.path());
    assert!(store.transact(&run, false, |_| Ok(())).is_err());
    assert!(store.load(&run).unwrap().is_none());
    store
        .transact(&run, true, |ledger| {
            ledger
                .add_observation(observation(&run, "observation:first"))
                .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))
        })
        .unwrap();
    let before = store.load(&run).unwrap().unwrap();
    assert!(store
        .transact(&run, false, |ledger| {
            ledger.run_ref = "run:01ARZ3NDEKTSV4RRFFQ69G5FAX".parse().unwrap();
            Ok(())
        })
        .is_err());
    assert_eq!(store.load(&run).unwrap().unwrap(), before);
}

#[test]
fn old_defaulted_v1_fields_and_explicit_empty_forms_remain_compatible() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_ref();
    let store = FileProjectDevelopmentStore::new(dir.path());
    store
        .save(&ProjectDevelopmentLedger::new(run.clone()))
        .unwrap();
    let path = dir.path().join(format!("{}.json", run.as_ref().id()));
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw.as_object_mut().unwrap().remove("intent");
    raw.as_object_mut().unwrap().remove("intent_return");
    raw["development_field"] = serde_json::Value::Null;
    raw["emptyRetainedExtension"] = serde_json::json!([]);
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    store.transact(&run,false,|ledger| {
        ledger.add_observation(observation(&run,"observation:old-v1")).map_err(|error|
            epilogos_factory::project_development_store::ProjectDevelopmentStoreError::Native(error.to_string()))
    }).unwrap();
    let current: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(current
        .as_object()
        .unwrap()
        .contains_key("development_field"));
    assert_eq!(current["development_field"], serde_json::Value::Null);
    assert_eq!(current["emptyRetainedExtension"], serde_json::json!([]));
    assert_eq!(store.load(&run).unwrap().unwrap().observations.len(), 1);
}

#[test]
fn new_native_return_cannot_inherit_empty_extension_from_previous_criterion_position() {
    use epilogos_factory::project_development_store::ProjectDevelopmentStoreError;
    let dir = tempfile::tempdir().unwrap();
    let run = run_ref();
    let store = FileProjectDevelopmentStore::new(dir.path());
    let mut ledger = ProjectDevelopmentLedger::new(run.clone());
    let mut intent = bounded_intent(&run);
    intent.success_condition_refs = vec!["criterion:first".into(), "criterion:second".into()];
    ledger.set_intent(intent).unwrap();
    let returned = |reference: &str, order: &[&str]| BoundedIntentReturn {
        run_ref: run.clone(),
        return_ref: reference.into(),
        intent_source_ref: "source:authored-intent".into(),
        context_resolution_ref: "context:operative".into(),
        artifact_refs: vec![],
        claim_refs: vec![],
        evidence_refs: vec![],
        criterion_evaluations: order
            .iter()
            .map(|reference| IntentCriterionEvaluation {
                criterion_ref: (*reference).into(),
                state: IntentCriterionState::Indeterminate,
                evidence_refs: vec![],
            })
            .collect(),
    };
    ledger
        .set_intent_return(returned(
            "return:first",
            &["criterion:first", "criterion:second"],
        ))
        .unwrap();
    store.save(&ledger).unwrap();
    let path = dir.path().join(format!("{}.json", run.as_ref().id()));
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["intent_return"]["criterion_evaluations"][0]["emptyLegacyCriterionExtension"] =
        serde_json::json!({});
    let retained = serde_json::to_vec_pretty(&raw).unwrap();
    fs::write(&path, &retained).unwrap();
    let error = store
        .transact(&run, false, |ledger| {
            ledger
                .set_intent_return(returned(
                    "return:successor",
                    &["criterion:second", "criterion:first"],
                ))
                .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous retained empty-field transfer"),
        "{error}"
    );
    assert_eq!(fs::read(&path).unwrap(), retained);
    assert_eq!(
        store
            .load(&run)
            .unwrap()
            .unwrap()
            .intent_return
            .unwrap()
            .return_ref,
        "return:first"
    );
}

#[test]
fn native_observation_append_preserves_empty_extension_by_exact_observation_identity() {
    use epilogos_factory::project_development_store::ProjectDevelopmentStoreError;
    let dir = tempfile::tempdir().unwrap();
    let run = run_ref();
    let store = FileProjectDevelopmentStore::new(dir.path());
    let mut ledger = ProjectDevelopmentLedger::new(run.clone());
    ledger
        .add_observation(observation(&run, "observation:original"))
        .unwrap();
    store.save(&ledger).unwrap();
    let path = dir.path().join(format!("{}.json", run.as_ref().id()));
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["observations"][0]["emptyLegacyObservationExtension"] = serde_json::json!([]);
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    store
        .transact(&run, false, |ledger| {
            ledger
                .add_observation(observation(&run, "observation:successor"))
                .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))
        })
        .unwrap();
    let current: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        current["observations"][0]["observation_ref"],
        "observation:original"
    );
    assert_eq!(
        current["observations"][0]["emptyLegacyObservationExtension"],
        serde_json::json!([])
    );
    assert_eq!(
        current["observations"][1]["observation_ref"],
        "observation:successor"
    );
    assert!(current["observations"][1]
        .get("emptyLegacyObservationExtension")
        .is_none());
}

#[cfg(unix)]
#[test]
fn actual_ledger_permission_admission_and_private_mode_survive_native_mutation() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let run = run_ref();
    let store = FileProjectDevelopmentStore::new(dir.path());
    store
        .save(&ProjectDevelopmentLedger::new(run.clone()))
        .unwrap();
    let path = dir.path().join(format!("{}.json", run.as_ref().id()));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::metadata(&path).unwrap();
    store
        .transact(&run, false, |ledger| {
            ledger
                .add_observation(observation(&run, "observation:private"))
                .unwrap();
            Ok(())
        })
        .unwrap();
    let after = fs::metadata(&path).unwrap();
    assert_eq!(
        (after.mode() & 0o7777, after.uid(), after.gid()),
        (before.mode() & 0o7777, before.uid(), before.gid())
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let retained = fs::read(&path).unwrap();
    assert!(store
        .transact(&run, false, |_| panic!(
            "read-only source must refuse before semantic mutation"
        ))
        .is_err());
    assert_eq!(fs::read(&path).unwrap(), retained);
    assert!(store.load(&run).unwrap().is_some());
}
