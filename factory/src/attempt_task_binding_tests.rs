//! The dispatched attempt, recorded on the session's bound AIKit task
//! (`attempt_owner_dispatch::record_on_task`). A fake `aikit` stands for the task
//! owner: it keeps the argv it was called with and answers what the real verb
//! answers (the contract is tested on AIKit's side with the same record).
//! These are ordinary tests, so they live outside `attempt_owner_dispatch::`,
//! whose test names are the native-evidence census (scripts/native_evidence_census.py).
use crate::attempt_learning::publication_tests::native_retention_source;
use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_owner_dispatch::{
    action_request, intent_ref, record, record_on_task, stamp, FactoryAttemptOwnerRequest,
    AIKIT_TASK_CONTRACT_REVISION, FACTORY_ATTEMPT_OWNER_ACTION, TASK_BINDING_CONTRACT,
    TRANSPORT_OBSERVATION,
};
use crate::attempt_runtime::{FactoryAttemptOperation, OwnerOperationPhase, OwnerOperationReceipt};
use crate::native_owner::NativeOwnerInvocation;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const DELIVERY: &str = "delivery:task-binding";

/// A fake `aikit` that logs its argv (one word per line) and answers
/// `reply` with `exit`.
fn fake_aikit(dir: &Path, exit: i32, stdout: &str, stderr: &str) -> std::path::PathBuf {
    let binary = dir.join("fake-aikit");
    std::fs::write(
        &binary,
        format!(
            "#!/bin/sh\nfor word in \"$@\"; do printf '%s\\n' \"$word\"; done > '{log}'\nprintf '%s' '{stdout}'\nprintf '%s' '{stderr}' >&2\nexit {exit}\n",
            log = dir.join("argv.log").display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    binary
}

fn argv(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("argv.log"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn recorded(attempt_ref: &str) -> String {
    json!({"schema": "aikit.encounter-task-attempts/v1", "revision": "task-attempts/ONE",
        "attempts": [{"attempt_ref": attempt_ref}]})
    .to_string()
}

/// The Run's attempt with its dispatch bound, as a task-bound send leaves it.
fn bound_attempt(
    task_admitted: bool,
) -> (
    tempfile::TempDir,
    FileAttemptStore,
    FactoryAttemptOwnerRequest,
) {
    let (directory, mut store, base) = native_retention_source();
    let reading = store.reading().unwrap();
    let attempt = record(&reading, &base.attempt_ref).unwrap();
    let execution = "execution:task-binding".to_owned();
    let session = attempt.disposition.body.agent_session_ref.clone();
    let agent = attempt.disposition.participant.agent_ref.clone();
    let request = FactoryAttemptOwnerRequest {
        contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
        request_ref: format!("owner-native:{}", base.request_ref),
        projection_ref: base.projection_ref,
        caller: base.caller.clone(),
        run_ref: base.run_ref,
        expected_revision: reading.revision,
        authority: base.authority,
        attempt_ref: base.attempt_ref,
        execution_ref: execution.clone(),
        invocation: NativeOwnerInvocation::AikitEncounter {
            binary: directory.path().join("fake-aikit"),
            cwd: directory.path().to_path_buf(),
            transport: None,
            contract_revision: AIKIT_TASK_CONTRACT_REVISION.into(),
            request: json!({"action": "send", "agent_session": session,
                "turn": {"delivery_ref": DELIVERY, "sender": base.caller.caller_ref,
                    "expected_binding_revision": "binding:task-binding",
                    "packet": {"audience": [agent], "text": "Record the attempt."}}}),
        },
    };
    let sent = stamp(
        OwnerOperationReceipt {
            owner_ref: "factory".into(),
            contract: TRANSPORT_OBSERVATION.into(),
            operation_ref: intent_ref(&request),
            receipt_ref: String::new(),
            source_revision: format!("factory-state:{}", request.expected_revision),
            phase: OwnerOperationPhase::Dispatching,
            evidence_refs: BTreeSet::new(),
            partial_effect_refs: BTreeSet::new(),
            payload: json!({"agentSession": session, "deliveryRef": DELIVERY, "action": "send",
                "executionRef": execution,
                "taskAdmission": if task_admitted {
                    json!({"contract": "factory.aikit-task-admission/v1"})
                } else { Value::Null }}),
        },
        OwnerOperationPhase::Dispatching,
        json!({"meaning": "test intent"}),
    );
    store
        .apply(action_request(
            &request,
            request.expected_revision,
            "intent",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: sent.clone(),
            },
        ))
        .unwrap();
    let bind = stamp(
        sent,
        OwnerOperationPhase::Submitted,
        json!({"meaning": "test bind"}),
    );
    let revision = store.reading().unwrap().revision;
    store
        .apply(action_request(
            &request,
            revision,
            "bind",
            FactoryAttemptOperation::BindDispatch {
                attempt_ref: request.attempt_ref.clone(),
                execution_ref: execution,
                receipt: bind,
            },
        ))
        .unwrap();
    (directory, store, request)
}

fn bindings(store: &FileAttemptStore, request: &FactoryAttemptOwnerRequest) -> Vec<Value> {
    let reading = store.reading().unwrap();
    record(&reading, &request.attempt_ref)
        .unwrap()
        .observations
        .iter()
        .filter(|receipt| receipt.contract == TASK_BINDING_CONTRACT)
        .map(|receipt| receipt.payload.clone())
        .collect()
}

#[test]
fn a_bound_task_dispatch_is_recorded_on_the_session_task_and_the_record_is_retained() {
    let (directory, mut store, request) = bound_attempt(true);
    fake_aikit(directory.path(), 0, &recorded(&request.attempt_ref), "");
    let state = directory.path().join("state.json");
    record_on_task(&mut store, &request, &state, DELIVERY);
    let words = argv(directory.path());
    assert_eq!(words[0], "-C");
    assert_eq!(
        &words[2..6],
        [
            "session-space",
            "encounter-task-attempts",
            "--agent-session",
            "session:test"
        ]
    );
    assert_eq!(words[6], "--request-json");
    let sent: Value = serde_json::from_str(&words[7]).unwrap();
    let attempt = &sent["attempt"];
    assert_eq!(attempt["task_ref"], "task:learning");
    assert_eq!(attempt["attempt_ref"], request.attempt_ref);
    assert_eq!(attempt["run_ref"], request.run_ref.to_string());
    assert_eq!(attempt["execution_ref"], request.execution_ref);
    assert_eq!(attempt["delivery_ref"], DELIVERY);
    assert_eq!(attempt["recorded_by"], "factory");
    assert_eq!(
        std::path::Path::new(attempt["state_path"].as_str().unwrap()),
        state.canonicalize().unwrap()
    );
    assert!(attempt["workflow_unit"].is_object() || attempt["workflow_unit"].is_string());
    assert_eq!(sent.as_object().unwrap().len(), 1, "one attempt per record");
    let kept = bindings(&store, &request);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0]["status"], "recorded");
    assert_eq!(kept[0]["owner"]["taskRevision"], "task-attempts/ONE");
    assert_eq!(kept[0]["agentSession"], "session:test");
    // A repeat of the same record is the same observation, not a new revision.
    let revision = store.reading().unwrap().revision;
    record_on_task(&mut store, &request, &state, DELIVERY);
    assert_eq!(store.reading().unwrap().revision, revision);
    assert_eq!(bindings(&store, &request).len(), 1);
}

#[test]
fn a_task_owner_that_refuses_is_retained_in_its_words_and_the_dispatch_stands() {
    let (directory, mut store, request) = bound_attempt(true);
    fake_aikit(
        directory.path(),
        1,
        "",
        "encounter.runtime: No task is bound to this session",
    );
    let before = store.reading().unwrap();
    record_on_task(
        &mut store,
        &request,
        &directory.path().join("state.json"),
        DELIVERY,
    );
    let kept = bindings(&store, &request);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0]["status"], "refused");
    assert!(kept[0]["owner"]["refusal"]
        .as_str()
        .unwrap()
        .contains("No task is bound"));
    let after = store.reading().unwrap();
    let attempt = record(&after, &request.attempt_ref).unwrap();
    assert_eq!(
        attempt.execution_ref,
        record(&before, &request.attempt_ref).unwrap().execution_ref,
        "the binding of the dispatch itself is untouched"
    );
    // The owner takes it later: the same operation, a new observation.
    fake_aikit(directory.path(), 0, &recorded(&request.attempt_ref), "");
    record_on_task(
        &mut store,
        &request,
        &directory.path().join("state.json"),
        DELIVERY,
    );
    let kept = bindings(&store, &request);
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[1]["status"], "recorded");
}

#[test]
fn an_owner_that_cannot_be_reached_or_answers_something_else_is_named() {
    let (directory, mut store, request) = bound_attempt(true);
    // No executable at the owner's path.
    record_on_task(
        &mut store,
        &request,
        &directory.path().join("state.json"),
        DELIVERY,
    );
    assert_eq!(bindings(&store, &request)[0]["status"], "unavailable");
    // An answer without the attempt is not a record.
    fake_aikit(
        directory.path(),
        0,
        r#"{"schema":"aikit.encounter-task-attempts/v1","attempts":[]}"#,
        "",
    );
    record_on_task(
        &mut store,
        &request,
        &directory.path().join("state.json"),
        DELIVERY,
    );
    let kept = bindings(&store, &request);
    assert_eq!(kept[1]["status"], "refused");
    assert!(kept[1]["owner"]["refusal"]
        .as_str()
        .unwrap()
        .contains("without the recorded attempt"));
}

#[test]
fn a_dispatch_that_is_not_on_a_task_or_not_bound_records_nothing() {
    // A plain encounter delivery has no AIKit task to record on.
    let (directory, mut store, request) = bound_attempt(false);
    fake_aikit(directory.path(), 0, &recorded(&request.attempt_ref), "");
    let revision = store.reading().unwrap().revision;
    record_on_task(
        &mut store,
        &request,
        &directory.path().join("state.json"),
        DELIVERY,
    );
    assert!(
        !directory.path().join("argv.log").exists(),
        "AIKit was not called"
    );
    assert_eq!(store.reading().unwrap().revision, revision);
    // A delivery that is not this attempt's own is no basis either.
    let (directory, mut store, request) = bound_attempt(true);
    fake_aikit(directory.path(), 0, &recorded(&request.attempt_ref), "");
    record_on_task(
        &mut store,
        &request,
        &directory.path().join("state.json"),
        "delivery:another",
    );
    assert!(!directory.path().join("argv.log").exists());
    // Nor an execution the attempt was not bound to.
    let other = FactoryAttemptOwnerRequest {
        execution_ref: "execution:not-bound".into(),
        ..request.clone()
    };
    record_on_task(
        &mut store,
        &other,
        &directory.path().join("state.json"),
        DELIVERY,
    );
    assert!(!directory.path().join("argv.log").exists());
    assert!(bindings(&store, &request).is_empty());
}
