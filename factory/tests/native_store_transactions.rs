//! Real process and OS-pipe regressions for the two native file owners.
//! Opening the FIFO writer proves the actual CLI reader reached input after
//! its original cached load. No timing guess, substitute provider or worker
//! execution is used as evidence for the owner transaction.
#![cfg(unix)]

use epilogos_factory::build::{
    CandidateRecord, FactoryBuildSelection, FactoryBuildState, FACTORY_NATIVE_OWNER,
    REQUEST_MORE_EVIDENCE_ACTION_REF, REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
};
use epilogos_factory::build_provider::FactoryBuildFileProvider;
use epilogos_factory::core::run::{Project, ProjectRef, Run, RunRef};
use epilogos_factory::project_development_store::{
    FileProjectDevelopmentStore, ProjectDevelopmentStore,
};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FCA";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FCB";

fn child(args: &[String]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
fn bounded_output(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "native CLI timed out: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn successful(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
struct InputGate {
    child: Option<Child>,
    writer: Option<File>,
}
impl InputGate {
    fn open(args: &[String], fifo: &Path) -> Self {
        assert!(Command::new("mkfifo").arg(fifo).status().unwrap().success());
        let mut process = child(args);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(fifo)
            {
                Ok(writer) => {
                    return Self {
                        child: Some(process),
                        writer: Some(writer),
                    }
                }
                Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {}
                Err(error) => panic!("real FIFO admission failed: {error}"),
            }
            if process.try_wait().unwrap().is_some() {
                let output = process.wait_with_output().unwrap();
                panic!(
                    "native CLI exited before reading input: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            if Instant::now() >= deadline {
                process.kill().unwrap();
                process.wait().unwrap();
                panic!("native CLI never opened its real request FIFO");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn finish(mut self, request: &Value) -> Output {
        let bytes = serde_json::to_vec(request).unwrap();
        assert!(
            bytes.len() < 4096,
            "bounded FIFO request must fit one pipe write"
        );
        self.writer.as_mut().unwrap().write_all(&bytes).unwrap();
        drop(self.writer.take());
        bounded_output(self.child.take().unwrap())
    }
}
impl Drop for InputGate {
    fn drop(&mut self) {
        drop(self.writer.take());
        if let Some(mut process) = self.child.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}
fn request_file(directory: &Path, label: &str, request: &Value) -> String {
    let path = directory.join(format!("{label}.json"));
    fs::write(&path, serde_json::to_vec(request).unwrap()).unwrap();
    path.display().to_string()
}
fn action_args(state: &Path, input: &str) -> Vec<String> {
    vec![
        "action".into(),
        "invoke".into(),
        state.display().to_string(),
        PROJECT.into(),
        RUN.into(),
        input.into(),
        "--json".into(),
    ]
}
fn action_request(candidate: &str) -> Value {
    json!({"contract":"factory.action-projection/v1","projectionRef":"projection:real-cli-regression",
        "caller":{"callerRef":"agent:native-store-regression","projectionKind":"situated-agent","lineage":["agent:native-store-regression"]},
        "actionRef":REQUEST_MORE_EVIDENCE_ACTION_REF,"subjectRef":candidate,"runRef":RUN,
        "authority":{"authorityRef":"authority:native-store-regression","nativeOwner":FACTORY_NATIVE_OWNER,
            "capabilityRef":REQUEST_MORE_EVIDENCE_CAPABILITY_REF,"capabilityGranted":true,"actionAuthorised":true}})
}
#[test]
fn native_cli_stale_open_build_actions_keep_both_attributable_results() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("build.json");
    let project: ProjectRef = PROJECT.parse().unwrap();
    let run: RunRef = RUN.parse().unwrap();
    let mut state = FactoryBuildState::new(
        Project::new(project.clone()),
        Run::new(
            run.clone(),
            project.clone(),
            "Real CLI concurrent owner Actions",
            "factory",
        )
        .unwrap(),
    )
    .unwrap();
    for reference in ["candidate:first", "candidate:second"] {
        state
            .insert_candidate(CandidateRecord {
                run_ref: run.clone(),
                candidate_ref: reference.into(),
                revision: 1,
                label: reference.into(),
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
    FactoryBuildFileProvider::create(
        &path,
        FactoryBuildSelection {
            project_ref: project.clone(),
            run_ref: run.clone(),
        },
        state,
    )
    .unwrap();
    let fifo = dir.path().join("action.fifo");
    let first = InputGate::open(&action_args(&path, &fifo.display().to_string()), &fifo);
    let second = successful(bounded_output(child(&action_args(
        &path,
        &request_file(
            dir.path(),
            "second-action",
            &action_request("candidate:second"),
        ),
    ))));
    let first = successful(first.finish(&action_request("candidate:first")));
    assert_eq!(
        first["nativeResult"]["previousRevision"],
        second["nativeResult"]["nextRevision"]
    );
    assert_eq!(
        first["caller"]["callerRef"],
        "agent:native-store-regression"
    );
    assert_ne!(
        first["nativeResult"]["createdHumanRequestRef"],
        second["nativeResult"]["createdHumanRequestRef"]
    );
    let current = FactoryBuildFileProvider::open(
        &path,
        FactoryBuildSelection {
            project_ref: project,
            run_ref: run,
        },
    )
    .unwrap()
    .snapshot()
    .unwrap();
    assert_eq!(current.view.human_requests.len(), 2);
    assert_eq!(
        current.revision,
        first["nativeResult"]["nextRevision"].as_u64().unwrap()
    );
    let before = fs::read(&path).unwrap();
    let duplicate = bounded_output(child(&action_args(
        &path,
        &request_file(
            dir.path(),
            "duplicate-action",
            &action_request("candidate:first"),
        ),
    )));
    assert!(!duplicate.status.success());
    assert_eq!(fs::read(&path).unwrap(), before);
}
fn field_args(root: &Path, operation: &str, input: &str) -> Vec<String> {
    vec![
        "development".into(),
        "field".into(),
        operation.into(),
        root.display().to_string(),
        RUN.into(),
        input.into(),
        "--json".into(),
    ]
}
fn field() -> Value {
    json!({"schema":"factory.development-field/v1","fieldRef":"development-field:real-cli",
        "projectRef":PROJECT,"runRef":RUN,"journeyRef":"journey:01ARZ3NDEKTSV4RRFFQ69G5FCC",
        "commissionRef":"commission:real-cli","requiredDifference":"Retain exact native material and Return histories",
        "targets":{"planRef":"plan:owner-transaction","sourceRefs":["source:primitive-integrity-commission"]},"requiredProof":["P"],
        "gitBasis":{"gitDevelopmentRef":"git-development:real-cli","projectRef":PROJECT,"runRef":RUN,
            "repositoryRef":"repo:factory","baseRevision":"revision:source-base","initialWorktreeClean":true,
            "initialWorktreeRef":"git-worktree:real-cli","sourceBasisRefs":["source:primitive-integrity-commission"]}})
}
fn material(reference: &str) -> Value {
    json!({"owner":"workcell","bindingRef":reference,"providerRef":"workcell","revision":"revision:material","provenanceRefs":["source:workcell-receipt"]})
}
fn operative(reference: &str) -> Value {
    json!({"resolutionRef":reference,"providerRef":"aikit","providerRevision":"revision:operative","sourceBasisRefs":["source:context-resolution"]})
}
fn returned(reference: &str) -> Value {
    json!({"returnRef":reference,"gitDevelopmentRef":"git-development:real-cli","baseRevision":"revision:source-base",
        "resultRevision":"revision:result","verificationEvidenceRefs":["evidence:source-test"],"materialBindingRefs":["material:first"]})
}
#[test]
fn native_cli_field_updates_retain_concurrent_material_operative_and_return_relations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ledger");
    successful(bounded_output(child(&field_args(
        &root,
        "set",
        &request_file(dir.path(), "field", &field()),
    ))));
    let fifo = dir.path().join("material.fifo");
    let first = InputGate::open(
        &field_args(&root, "bind-material", &fifo.display().to_string()),
        &fifo,
    );
    successful(bounded_output(child(&field_args(
        &root,
        "bind-material",
        &request_file(dir.path(), "material-second", &material("material:second")),
    ))));
    let reading = successful(first.finish(&material("material:first")));
    assert_eq!(
        reading["field"]["materialBindings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    successful(bounded_output(child(&field_args(
        &root,
        "operative",
        &request_file(
            dir.path(),
            "operative-initial",
            &operative("resolution:initial"),
        ),
    ))));
    let fifo = dir.path().join("operative.fifo");
    let changing = InputGate::open(
        &field_args(&root, "operative", &fifo.display().to_string()),
        &fifo,
    );
    successful(bounded_output(child(&field_args(
        &root,
        "return",
        &request_file(dir.path(), "return-first", &returned("return:first")),
    ))));
    let reading = successful(changing.finish(&operative("resolution:subsequent")));
    assert_eq!(reading["field"]["returns"].as_array().unwrap().len(), 1);
    assert_eq!(
        reading["field"]["aikitOperative"]["resolutionRef"],
        "resolution:subsequent"
    );
    let fifo = dir.path().join("return.fifo");
    let returning = InputGate::open(
        &field_args(&root, "return", &fifo.display().to_string()),
        &fifo,
    );
    successful(bounded_output(child(&field_args(
        &root,
        "return",
        &request_file(dir.path(), "return-second", &returned("return:second")),
    ))));
    let reading = successful(returning.finish(&returned("return:third")));
    assert_eq!(reading["field"]["returns"].as_array().unwrap().len(), 3);
    assert_eq!(
        reading["field"]["materialBindings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let run: RunRef = RUN.parse().unwrap();
    let reloaded = FileProjectDevelopmentStore::new(&root)
        .load(&run)
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.development_field.unwrap().returns.len(), 3);
    let before = fs::read(root.join(format!("{}.json", run.as_ref().id()))).unwrap();
    let rejected = bounded_output(child(&field_args(
        &root,
        "return",
        &request_file(dir.path(), "duplicate-return", &returned("return:first")),
    )));
    assert!(!rejected.status.success());
    assert_eq!(
        fs::read(root.join(format!("{}.json", run.as_ref().id()))).unwrap(),
        before
    );
}

#[test]
fn native_action_process_interrupted_during_real_file_write_recovers_with_same_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("build.json");
    let project: ProjectRef = PROJECT.parse().unwrap();
    let run: RunRef = RUN.parse().unwrap();
    let mut state = FactoryBuildState::new(
        Project::new(project.clone()),
        Run::new(
            run.clone(),
            project.clone(),
            "Real publication interruption",
            "factory",
        )
        .unwrap(),
    )
    .unwrap();
    state
        .insert_candidate(CandidateRecord {
            run_ref: run.clone(),
            candidate_ref: "candidate:interrupted".into(),
            revision: 1,
            label: "interrupted publication".into(),
            status: "ready".into(),
            producing_execution_refs: vec![],
            claim_refs: vec![],
            evidence_refs: vec![],
            artifact_refs: vec![],
            preview_ref: None,
            tradeoffs: vec![],
        })
        .unwrap();
    FactoryBuildFileProvider::create(
        &path,
        FactoryBuildSelection {
            project_ref: project.clone(),
            run_ref: run.clone(),
        },
        state,
    )
    .unwrap();
    let before = fs::read(&path).unwrap();
    let input = request_file(
        dir.path(),
        "interrupted-action",
        &action_request("candidate:interrupted"),
    );
    // An actual OS file-size limit interrupts the native publisher. The static
    // shell program passes all caller paths as argv, without interpolation.
    let failed = bounded_output(
        Command::new("/bin/sh")
            .args([
                "-c",
                "ulimit -c 0; ulimit -f 0; exec \"$@\"",
                "factory-file-limit",
            ])
            .arg(env!("CARGO_BIN_EXE_factory"))
            .args(action_args(&path, &input))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    assert!(
        !failed.status.success(),
        "OS file limit must reject the actual publisher"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    let recovered = successful(bounded_output(child(&action_args(&path, &input))));
    assert_eq!(recovered["subjectRef"], "candidate:interrupted");
    let current = FactoryBuildFileProvider::open(
        &path,
        FactoryBuildSelection {
            project_ref: project,
            run_ref: run,
        },
    )
    .unwrap()
    .snapshot()
    .unwrap();
    assert_eq!(current.view.human_requests.len(), 1);
}

#[test]
#[ignore = "subprocess body; exercised by killed_native_ledger_owner test"]
fn ledger_lock_holder_child() {
    let Some(root) = std::env::var_os("FACTORY_LEDGER_LOCK_CHILD_ROOT") else {
        return;
    };
    let ready = std::env::var_os("FACTORY_LEDGER_LOCK_CHILD_READY").unwrap();
    let run: RunRef = RUN.parse().unwrap();
    let store = FileProjectDevelopmentStore::new(root);
    store
        .transact(&run, false, |ledger| {
            ledger
                .set_intent(
                    epilogos_factory::project_development::BoundedIntentCondition {
                        run_ref: run.clone(),
                        condition_ref: "condition:unpublished-child".into(),
                        intent_source_ref: "source:author".into(),
                        focus_ref: None,
                        success_condition_refs: vec![],
                        constraint_refs: vec![],
                        context_resolution_ref: "context:child".into(),
                    },
                )
                .unwrap();
            fs::write(ready, b"native Run lock held; candidate not published").unwrap();
            let mut input = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).unwrap();
            Ok(())
        })
        .unwrap();
}

#[test]
fn killed_native_ledger_owner_releases_lock_without_publishing_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ledger");
    let run: RunRef = RUN.parse().unwrap();
    let store = FileProjectDevelopmentStore::new(&root);
    store
        .save(&epilogos_factory::project_development::ProjectDevelopmentLedger::new(run.clone()))
        .unwrap();
    let path = root.join(format!("{}.json", run.as_ref().id()));
    let before = fs::read(&path).unwrap();
    let ready = dir.path().join("child-lock-ready");
    let process = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ledger_lock_holder_child",
            "--ignored",
            "--nocapture",
        ])
        .env("FACTORY_LEDGER_LOCK_CHILD_ROOT", &root)
        .env("FACTORY_LEDGER_LOCK_CHILD_READY", &ready)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut holder = InputGate {
        child: Some(process),
        writer: None,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "real native ledger child did not acquire its Run lock"
        );
        assert!(
            holder.child.as_mut().unwrap().try_wait().unwrap().is_none(),
            "native ledger child exited early"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // A second actual CLI process must return a bounded lock refusal rather
    // than wait forever or evaluate the unpublished child candidate.
    let contender = json!({"observation_ref":"observation:contended", "kind":"insufficient-evidence",
        "statement":"bounded native contention", "subject_refs":[RUN], "evidence_refs":[]});
    let started = Instant::now();
    let refused = bounded_output(child(&[
        "development".into(),
        "observe".into(),
        root.display().to_string(),
        RUN.into(),
        request_file(dir.path(), "contended-observation", &contender),
        "--json".into(),
    ]));
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("lock deadline exceeded"));
    assert!(started.elapsed() < Duration::from_secs(4));
    assert_eq!(fs::read(&path).unwrap(), before);
    holder.child.as_mut().unwrap().kill().unwrap();
    holder.child.take().unwrap().wait().unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
    let request = json!({"observation_ref":"observation:post-interruption","kind":"insufficient-evidence",
        "statement":"Native Run lock recovered after actual owner process termination","subject_refs":[RUN],"evidence_refs":[]});
    let output = successful(bounded_output(child(&[
        "development".into(),
        "observe".into(),
        root.display().to_string(),
        RUN.into(),
        request_file(dir.path(), "post-interruption", &request),
        "--json".into(),
    ])));
    assert_eq!(output["observationCount"], 1);
    let current = store.load(&run).unwrap().unwrap();
    assert!(current.intent.is_none());
    assert_eq!(
        current.observations[0].observation_ref,
        "observation:post-interruption"
    );
}

#[test]
fn same_native_developmental_provider_and_transaction_retain_interleaved_owner_actions() {
    use epilogos_factory::build::{ClaimRecord, FactoryActionAuthority, FactoryActionInvocation};
    use epilogos_factory::developmental_read::{
        FactoryDevelopmentalFileProvider, FactoryDevelopmentalState,
    };
    use epilogos_factory::project_development_store::{
        transact_developmental_state, ProjectDevelopmentStoreError,
    };
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("development-state.json");
    let project: ProjectRef = PROJECT.parse().unwrap();
    let run: RunRef = RUN.parse().unwrap();
    let mut build = FactoryBuildState::new(
        Project::new(project.clone()),
        Run::new(run.clone(), project, "one native physical state", "factory").unwrap(),
    )
    .unwrap();
    for candidate_ref in ["candidate:first", "candidate:second"] {
        build
            .insert_candidate(CandidateRecord {
                run_ref: run.clone(),
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
    FactoryDevelopmentalFileProvider::create_new(
        &path,
        FactoryDevelopmentalState::new(build, vec![]).unwrap(),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let privacy = fs::metadata(&path).unwrap();
    let mut first = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    let mut second = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    let grant = FactoryActionAuthority {
        authority_ref: "authority:native-store-regression".into(),
        native_owner: FACTORY_NATIVE_OWNER.into(),
        capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
        capability_granted: true,
        action_authorised: true,
    };
    let invoke = |subject: &str| FactoryActionInvocation {
        action_ref: REQUEST_MORE_EVIDENCE_ACTION_REF.into(),
        subject_ref: subject.into(),
        run_ref: run.clone(),
    };
    let one = first
        .execute_action(&invoke("candidate:first"), &grant)
        .unwrap();
    transact_developmental_state(&path, |current| {
        current
            .build
            .insert_claim(ClaimRecord {
                run_ref: run.clone(),
                claim_ref: "claim:interleaved".into(),
                statement: "native transaction retained".into(),
                status: "proposed".into(),
                evidence_refs: vec![],
            })
            .map_err(|error| ProjectDevelopmentStoreError::Native(error.to_string()))
    })
    .unwrap();
    let two = second
        .execute_action(&invoke("candidate:second"), &grant)
        .unwrap();
    assert!(two.previous_revision > one.next_revision);
    let reopened = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    let snapshot = reopened.build_snapshot(&run).unwrap();
    assert_eq!(snapshot.view.human_requests.len(), 2);
    assert_eq!(snapshot.view.claims[0].claim_ref, "claim:interleaved");
    let after = fs::metadata(&path).unwrap();
    assert_eq!(
        (after.mode() & 0o7777, after.uid(), after.gid()),
        (privacy.mode() & 0o7777, privacy.uid(), privacy.gid())
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let retained = fs::read(&path).unwrap();
    assert!(transact_developmental_state::<()>(&path, |_| panic!(
        "read-only source cannot reach mutation"
    ))
    .is_err());
    assert_eq!(fs::read(&path).unwrap(), retained);
}
