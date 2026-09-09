//! Real Factory provider/CLI proof for AIKit Routine-backed Journey continuation.

use epilogos_factory::build::FactoryBuildState;
use epilogos_factory::core::run::{Project, ProjectRef, Run, RunRef};
use epilogos_factory::developmental_read::{
    FactoryDevelopmentalFileProvider, FactoryDevelopmentalState,
};
use epilogos_factory::journey::{Journey, JourneyCommission, JourneyStatus};
use epilogos_factory::routine_continuation::{
    AikitRoutineInvocationEvidence, FactoryRoutineContinuationAdmissionStatus,
    FactoryRoutineContinuationRequest, FACTORY_ROUTINE_CONTINUATION_REQUEST,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use tempfile::TempDir;

const OWNER_EVIDENCE: &str =
    include_str!("../../contracts/factory/fixtures/aikit-routine-invocation-evidence.json");
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE";
const INITIAL_RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA";
const JOURNEY: &str = "journey:01ARZ3NDEKTSV4RRFFQ69G5FAD";

fn evidence() -> AikitRoutineInvocationEvidence {
    serde_json::from_str(OWNER_EVIDENCE).unwrap()
}

fn request(evidence: AikitRoutineInvocationEvidence) -> FactoryRoutineContinuationRequest {
    FactoryRoutineContinuationRequest {
        contract: FACTORY_ROUTINE_CONTINUATION_REQUEST.into(),
        journey_ref: JOURNEY.parse().unwrap(),
        destination: "Return one bounded verified research increment".into(),
        write_owner: "factory".into(),
        invocation_evidence: evidence,
    }
}

fn create_state(path: &Path, status: JourneyStatus) {
    let project_ref: ProjectRef = PROJECT.parse().unwrap();
    let initial_run_ref: RunRef = INITIAL_RUN.parse().unwrap();
    let build = FactoryBuildState::new(
        Project::new(project_ref.clone()),
        Run::new(
            initial_run_ref.clone(),
            project_ref.clone(),
            "Initial Journey formation",
            "factory",
        )
        .unwrap(),
    )
    .unwrap();
    let mut journey = Journey::new(
        JOURNEY.parse().unwrap(),
        project_ref,
        JourneyCommission {
            purpose: "Carry recurring verified research as bounded development".into(),
            commission_ref: Some("commission:research".into()),
            why_refs: vec!["source:project-intent".into()],
        },
        "Await the next valid Routine occurrence",
        "2026-09-09T08:00:00Z",
    )
    .unwrap();
    journey
        .add_run(
            initial_run_ref,
            vec!["basis:journey-formation".into()],
            Vec::new(),
        )
        .unwrap();
    journey.status = status;
    FactoryDevelopmentalFileProvider::create(
        path,
        FactoryDevelopmentalState::new(build, vec![journey]).unwrap(),
    )
    .unwrap();
}

#[test]
fn accepted_owner_evidence_atomically_creates_one_run_and_explainable_relation() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("state.json");
    create_state(&path, JourneyStatus::Active);
    let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    let admission = provider
        .admit_routine_continuation(request(evidence()))
        .unwrap();
    assert_eq!(
        admission.status,
        FactoryRoutineContinuationAdmissionStatus::Applied
    );
    assert_eq!(
        admission.continuation.invocation_evidence.proof_standing,
        "current-on-supplied-basis"
    );
    assert_eq!(
        admission
            .continuation
            .invocation_evidence
            .authority_validation
            .standing,
        "owner-attested"
    );
    assert!(admission.continuation.activity_refs.is_empty());
    assert!(admission.continuation.return_refs.is_empty());

    let reopened = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    let reading = reopened
        .routine_continuation_reading(&admission.continuation.invocation_evidence.invocation_ref)
        .unwrap();
    assert_eq!(reading.continuation, admission.continuation);
    for relation in [
        "continued-by",
        "admitted-from",
        "invokes",
        "proven-by",
        "triggered-by",
        "authority-attested-by",
        "delivered-by",
    ] {
        assert!(reading
            .traversal
            .iter()
            .any(|edge| edge.relation == relation));
    }
    let journey = reopened.journey_reading(&JOURNEY.parse().unwrap()).unwrap();
    assert_eq!(journey.run_refs.len(), 2);
    assert_eq!(journey.routine_invocation_refs.len(), 1);
    assert!(journey.activity_refs.is_empty());
    assert!(journey.returns.is_empty());
    let run = reopened.run_reading(&reading.continuation.run_ref).unwrap();
    assert_eq!(run.routine_invocation_refs, journey.routine_invocation_refs);
    assert!(run.executions.is_empty());
    assert!(run.evidence.is_empty());
    assert!(run.human_requests.is_empty());
}

#[test]
fn exact_replay_is_byte_stable_and_delivery_retry_updates_no_run_identity() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("state.json");
    create_state(&path, JourneyStatus::Paused);
    let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    let first = provider
        .admit_routine_continuation(request(evidence()))
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let replay = provider
        .admit_routine_continuation(request(evidence()))
        .unwrap();
    assert_eq!(
        replay.status,
        FactoryRoutineContinuationAdmissionStatus::AlreadyApplied
    );
    assert_eq!(replay.continuation.run_ref, first.continuation.run_ref);
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let mut retry = evidence();
    retry.provider_deliveries.push(
        serde_json::from_value(json!({
            "provider": "provider:cron",
            "delivery_ref": "provider-delivery:2026-09-09:retry-2",
            "provider_job_id": "job-43",
            "restart_ref": "provider-restart:cron:9"
        }))
        .unwrap(),
    );
    let delivery = provider.admit_routine_continuation(request(retry)).unwrap();
    assert_eq!(
        delivery.status,
        FactoryRoutineContinuationAdmissionStatus::DeliveryRecorded
    );
    assert_eq!(delivery.continuation.run_ref, first.continuation.run_ref);
    assert_eq!(delivery.continuation.revision.get(), 2);
    assert_eq!(
        delivery
            .continuation
            .invocation_evidence
            .provider_deliveries
            .len(),
        2
    );
    let reopened = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    assert_eq!(
        reopened
            .journey_reading(&JOURNEY.parse().unwrap())
            .unwrap()
            .run_refs
            .len(),
        2
    );
}

#[test]
fn invalid_owner_standing_and_changed_replay_fail_without_mutation() {
    for (name, mutate) in [
        ("disabled", disabled as fn(&mut Value)),
        ("stale", stale),
        ("revoked", revoked),
        ("unattested", unattested),
        ("bad-order", bad_order),
        ("pre-epoch", pre_epoch),
    ] {
        let home = TempDir::new().unwrap();
        let path = home.path().join(format!("{name}.json"));
        create_state(&path, JourneyStatus::Active);
        let before = std::fs::read(&path).unwrap();
        let mut value: Value = serde_json::from_str(OWNER_EVIDENCE).unwrap();
        mutate(&mut value);
        let parsed: AikitRoutineInvocationEvidence = serde_json::from_value(value).unwrap();
        let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
        assert!(provider
            .admit_routine_continuation(request(parsed))
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}");
    }

    let home = TempDir::new().unwrap();
    let path = home.path().join("conflict.json");
    create_state(&path, JourneyStatus::Active);
    let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    provider
        .admit_routine_continuation(request(evidence()))
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut changed = evidence();
    changed.method_revision = "method-rev-forged".into();
    assert!(provider
        .admit_routine_continuation(request(changed))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn strict_input_terminal_journey_and_manual_provider_absence_are_truthful() {
    let mut unknown = serde_json::to_value(request(evidence())).unwrap();
    unknown["invocationEvidence"]["schedulerState"] = json!("active");
    assert!(serde_json::from_value::<FactoryRoutineContinuationRequest>(unknown).is_err());

    let home = TempDir::new().unwrap();
    let terminal_path = home.path().join("terminal.json");
    create_state(&terminal_path, JourneyStatus::Abandoned);
    let before = std::fs::read(&terminal_path).unwrap();
    let mut terminal = FactoryDevelopmentalFileProvider::open(&terminal_path).unwrap();
    assert!(terminal
        .admit_routine_continuation(request(evidence()))
        .is_err());
    assert_eq!(std::fs::read(&terminal_path).unwrap(), before);

    let manual_path = home.path().join("manual.json");
    create_state(&manual_path, JourneyStatus::Active);
    let mut manual = evidence();
    manual.invocation_ref = "routine-invocation:manual:1".into();
    manual.trigger_observation_ref = "trigger-observation:manual:1".into();
    manual.trigger = serde_json::from_value(json!({"kind": "manual"})).unwrap();
    manual.authority_validation.unattended = false;
    manual.provider_deliveries.clear();
    let mut provider = FactoryDevelopmentalFileProvider::open(&manual_path).unwrap();
    let admitted = provider
        .admit_routine_continuation(request(manual))
        .unwrap();
    assert!(admitted
        .continuation
        .invocation_evidence
        .provider_deliveries
        .is_empty());
}

#[test]
fn distinct_occurrences_create_distinct_runs_but_shared_trigger_or_delivery_rejects() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("state.json");
    create_state(&path, JourneyStatus::Active);
    let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    let first = provider
        .admit_routine_continuation(request(evidence()))
        .unwrap();

    let mut second = evidence();
    second.invocation_ref = "routine-invocation:2026-09-09:daily-research:2".into();
    second.trigger_observation_ref = "trigger-observation:2026-09-09:2".into();
    second.trigger_observed_at = "2026-09-09T10:30:00Z".into();
    second.authority_validation.validation_ref = "authority-validation:2026-09-09:2".into();
    second.authority_validation.validated_at = "2026-09-09T10:30:01Z".into();
    second.provider_deliveries[0].delivery_ref = "provider-delivery:2026-09-09:2".into();
    second.provider_deliveries[0].provider_job_id = Some("job-99".into());
    let second_admission = provider
        .admit_routine_continuation(request(second.clone()))
        .unwrap();
    assert_ne!(
        first.continuation.run_ref,
        second_admission.continuation.run_ref
    );
    assert_eq!(
        provider
            .journey_reading(&JOURNEY.parse().unwrap())
            .unwrap()
            .run_refs
            .len(),
        3
    );

    let before = std::fs::read(&path).unwrap();
    let mut reused_delivery = second.clone();
    reused_delivery.invocation_ref = "routine-invocation:2026-09-09:daily-research:3".into();
    reused_delivery.trigger_observation_ref = "trigger-observation:2026-09-09:3".into();
    reused_delivery.provider_deliveries[0].provider = "provider:different".into();
    reused_delivery.provider_deliveries[0].delivery_ref =
        evidence().provider_deliveries[0].delivery_ref.clone();
    assert!(provider
        .admit_routine_continuation(request(reused_delivery))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);

    second.invocation_ref = "routine-invocation:2026-09-09:daily-research:3".into();
    second.trigger_observation_ref = evidence().trigger_observation_ref;
    assert!(provider
        .admit_routine_continuation(request(second))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn concurrent_native_admission_has_one_consequence() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("state.json");
    create_state(&path, JourneyStatus::Active);
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let path = path.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
            barrier.wait();
            provider
                .admit_routine_continuation(request(evidence()))
                .unwrap()
                .status
        }));
    }
    barrier.wait();
    let statuses = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert!(statuses.contains(&FactoryRoutineContinuationAdmissionStatus::Applied));
    assert!(statuses.contains(&FactoryRoutineContinuationAdmissionStatus::AlreadyApplied));
    let reopened = FactoryDevelopmentalFileProvider::open(&path).unwrap();
    assert_eq!(
        reopened
            .journey_reading(&JOURNEY.parse().unwrap())
            .unwrap()
            .run_refs
            .len(),
        2
    );
}

#[test]
fn real_factory_binary_admits_and_reads_exact_ai_kit_owner_fixture() {
    let home = TempDir::new().unwrap();
    let state = home.path().join("state.json");
    create_state(&state, JourneyStatus::Active);
    let request_path = home.path().join("request.json");
    std::fs::write(
        &request_path,
        serde_json::to_vec_pretty(&request(evidence())).unwrap(),
    )
    .unwrap();
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_factory"));
    let output = Command::new(&binary)
        .args([
            "development",
            "admit-routine-continuation",
            state.to_str().unwrap(),
            request_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let admission: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(admission["status"], "applied");
    let run_ref = admission["continuation"]["runRef"].as_str().unwrap();
    let output = Command::new(binary)
        .args([
            "development",
            "routine-continuation",
            state.to_str().unwrap(),
            "routine-invocation:2026-09-09:daily-research:1",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let reading: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reading["continuation"]["runRef"], run_ref);
    assert_eq!(
        reading["continuation"]["invocationEvidence"]["proof_standing"],
        "current-on-supplied-basis"
    );
}

fn disabled(value: &mut Value) {
    value["routine_state"] = json!("disabled");
}
fn stale(value: &mut Value) {
    value["proof_standing"] = json!("stale");
}
fn revoked(value: &mut Value) {
    value["authority_validation"]["granted"] = json!(false);
}
fn unattested(value: &mut Value) {
    value["authority_validation"]["standing"] = json!("observed");
}
fn bad_order(value: &mut Value) {
    value["authority_validation"]["validated_at"] = json!("2026-09-09T09:29:59Z");
}
fn pre_epoch(value: &mut Value) {
    value["trigger_observed_at"] = json!("1969-12-31T23:59:59Z");
    value["authority_validation"]["validated_at"] = json!("1970-01-01T00:00:00Z");
}
