#![cfg(unix)]

//! Fixture tests for the Workcell place lifecycle (`attempt_place.rs`). A
//! fake `workcell` binary stands in for the real owner client, exactly as the
//! material tests fake the world owner: grants parse verbatim, refusals
//! surface once as typed evidence and are never retried, and a granted place
//! is recorded as material provenance on the attempt record — never as run or
//! attempt identity.

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_place::{
    release_place, request_place, PlaceError, PlaceProvider, PlaceProviderPolicy, PlaceRefusal,
    PlaceReleaseRequest, PlaceRequest, WorkcellPlaceGrant, PLACE_GRANT_CONTRACT_REVISION,
};
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::RetryGrant;
use epilogos_factory::workflow::{compile_workflow, WorkflowSource};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const TIMEOUT_MS: u64 = 5_000;

fn tmux_grant_json() -> String {
    json!({
        "schema": PLACE_GRANT_CONTRACT_REVISION,
        "place_ref": "workcell:place:tmux:/tmp/workcell-place-test.sock:attempt-room",
        "provider": "tmux",
        "session_name": "attempt-room",
        "pane_id": "%3",
        "pane_pid": 4242,
        "process_start_marker": "1747344000.123456-3",
        "created_utc": "2026-09-15T09:30:00Z",
    })
    .to_string()
}

/// A Herdr grant carries no pane and therefore no pid/start-marker proof
/// tokens. Releasing one through this contract would mean killing without
/// proof, so Factory refuses before any call.
fn herdr_grant_json() -> String {
    json!({
        "schema": PLACE_GRANT_CONTRACT_REVISION,
        "place_ref": "workcell:place:herdr:attempt-room-7",
        "provider": "herdr",
        "session_name": "attempt-room-7",
        "pane_id": null,
        "pane_pid": null,
        "process_start_marker": null,
        "created_utc": "2026-09-15T09:30:00Z",
    })
    .to_string()
}

const FAKE_WORKCELL: &str = r#"#!/usr/bin/env python3
import json, pathlib, sys

root = pathlib.Path(__file__).resolve().parent
args = sys.argv[1:]
with (root / "calls").open("a") as calls:
    calls.write(json.dumps(args) + "\n")
if args[0:2] not in (["place", "request"], ["place", "release"]):
    print("unexpected invocation", args, file=sys.stderr)
    sys.exit(3)
mode = (root / "mode").read_text().strip() if (root / "mode").exists() else "ok"
if args[1] == "request":
    if mode == "refuse":
        print("workcell: already-exists: place attempt-room already exists", file=sys.stderr)
        sys.exit(9)
    if mode == "garbage":
        print("placement refused loudly, in words, not JSON")
        sys.exit(0)
    sys.stdout.write((root / "grant.json").read_text())
    sys.exit(0)
if "--pid" not in args or "--start-marker" not in args:
    print("workcell: release requires --pid and --start-marker proof tokens", file=sys.stderr)
    sys.exit(4)
if mode == "stale":
    print("workcell: stale-binding: pid 4242 no longer holds the granted place", file=sys.stderr)
    sys.exit(7)
print(json.dumps({"schema": "workcell.place-release/v1", "released": True}))
"#;

struct FakeWorkcell {
    dir: tempfile::TempDir,
    binary: PathBuf,
}

impl FakeWorkcell {
    fn new(grant: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("workcell.py");
        fs::write(&binary, FAKE_WORKCELL).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let workcell = Self { dir, binary };
        workcell.set_grant(grant);
        workcell
    }

    fn binary(&self) -> &PathBuf {
        &self.binary
    }

    fn set_grant(&self, grant: &str) {
        fs::write(self.dir.path().join("grant.json"), grant).unwrap();
    }

    fn set_mode(&self, mode: &str) {
        fs::write(self.dir.path().join("mode"), mode).unwrap();
    }

    fn calls(&self) -> Vec<Vec<String>> {
        fs::read_to_string(self.dir.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

fn request<'a>(
    workcell: &'a FakeWorkcell,
    provider: PlaceProviderPolicy,
    name: &'a str,
) -> PlaceRequest<'a> {
    PlaceRequest {
        binary: workcell.binary(),
        provider,
        name,
        timeout_ms: TIMEOUT_MS,
    }
}

fn release<'a>(
    workcell: &'a FakeWorkcell,
    grant: &'a WorkcellPlaceGrant,
) -> PlaceReleaseRequest<'a> {
    PlaceReleaseRequest {
        binary: workcell.binary(),
        grant,
        timeout_ms: TIMEOUT_MS,
    }
}

fn parse_grant(raw: &str) -> WorkcellPlaceGrant {
    serde_json::from_str(raw).unwrap()
}

fn refused<'a>(operation: &'static str, error: &'a PlaceError) -> &'a PlaceRefusal {
    match error {
        PlaceError::Refused(refusal) => {
            assert_eq!(refusal.operation, operation);
            refusal
        }
        other => panic!("expected a typed {operation} refusal, got {other:?}"),
    }
}

#[test]
fn valid_grant_is_parsed_verbatim_with_explicit_argv() {
    let workcell = FakeWorkcell::new(&tmux_grant_json());
    let grant = request_place(&request(
        &workcell,
        PlaceProviderPolicy::Auto,
        "attempt-room",
    ))
    .expect("a valid grant is accepted");
    assert_eq!(grant.schema, PLACE_GRANT_CONTRACT_REVISION);
    assert_eq!(grant.provider, PlaceProvider::Tmux);
    assert_eq!(grant.session_name, "attempt-room");
    assert_eq!(grant.pane_id.as_deref(), Some("%3"));
    assert_eq!(grant.pane_pid, Some(4242));
    assert_eq!(
        grant.process_start_marker.as_deref(),
        Some("1747344000.123456-3")
    );
    assert_eq!(
        grant.place_ref,
        "workcell:place:tmux:/tmp/workcell-place-test.sock:attempt-room"
    );
    let calls = workcell.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        vec![
            "place",
            "request",
            "--provider",
            "auto",
            "--name",
            "attempt-room"
        ]
    );
}

#[test]
fn request_refusal_is_typed_evidence_and_is_never_retried() {
    let workcell = FakeWorkcell::new(&tmux_grant_json());
    workcell.set_mode("refuse");
    let error = request_place(&request(
        &workcell,
        PlaceProviderPolicy::Auto,
        "attempt-room",
    ))
    .expect_err("an already-exists refusal is evidence, not a retry prompt");
    let refusal = refused("request", &error);
    assert_eq!(refusal.exit_code, Some(9));
    assert!(refusal.stderr.contains("already-exists"));
    // Exactly one invocation: the refusal is surfaced verbatim and deciding
    // what follows is an explicit act.
    assert_eq!(workcell.calls().len(), 1);
}

#[test]
fn foreign_schema_and_non_json_answers_are_refused_not_normalised() {
    let mut foreign = serde_json::from_str::<Value>(&tmux_grant_json()).unwrap();
    foreign["schema"] = json!("workcell.place-grant/v2");
    let workcell = FakeWorkcell::new(&foreign.to_string());
    let error = request_place(&request(
        &workcell,
        PlaceProviderPolicy::Herdr,
        "attempt-room",
    ))
    .expect_err("a foreign contract revision is never normalised into the pinned one");
    match &error {
        PlaceError::InvalidResponse(message) => {
            assert!(message.contains(PLACE_GRANT_CONTRACT_REVISION));
        }
        other => panic!("expected an invalid-response refusal, got {other:?}"),
    }
    assert_eq!(workcell.calls().len(), 1);

    workcell.set_grant(&tmux_grant_json());
    workcell.set_mode("garbage");
    let error = request_place(&request(
        &workcell,
        PlaceProviderPolicy::Auto,
        "attempt-room",
    ))
    .expect_err("non-JSON stdout is not a grant");
    assert!(matches!(error, PlaceError::InvalidResponse(_)));
    assert_eq!(workcell.calls().len(), 2);
}

#[test]
fn release_proves_the_grants_binding_and_surfaces_stale_refusal_once() {
    let workcell = FakeWorkcell::new(&tmux_grant_json());
    let grant = parse_grant(&tmux_grant_json());
    workcell.set_mode("stale");
    let error = release_place(&release(&workcell, &grant))
        .expect_err("a stale binding is observed evidence, never a retry prompt");
    let refusal = refused("release", &error);
    assert_eq!(refusal.exit_code, Some(7));
    assert!(refusal.stderr.contains("stale-binding"));
    let calls = workcell.calls();
    assert_eq!(calls.len(), 1, "no implicit retry after a stale refusal");
    assert_eq!(
        calls[0],
        vec![
            "place".into(),
            "release".into(),
            "--place-ref".into(),
            grant.place_ref.clone(),
            "--pid".into(),
            "4242".into(),
            "--start-marker".into(),
            "1747344000.123456-3".into(),
        ]
    );
}

#[test]
fn release_success_is_a_semantic_act_with_a_receipt() {
    let workcell = FakeWorkcell::new(&tmux_grant_json());
    let grant = parse_grant(&tmux_grant_json());
    let receipt =
        release_place(&release(&workcell, &grant)).expect("a proven release ends the room");
    assert_eq!(receipt.place_ref, grant.place_ref);
    assert!(receipt.stdout.contains("released"));
    assert!(receipt.stderr.is_empty());
}

#[test]
fn release_without_proof_tokens_refuses_before_any_call() {
    let workcell = FakeWorkcell::new(&herdr_grant_json());
    let grant = parse_grant(&herdr_grant_json());
    let error = release_place(&release(&workcell, &grant))
        .expect_err("releasing without pid and start marker would kill without proof");
    match &error {
        PlaceError::UnprovableBinding { place_ref } => {
            assert_eq!(place_ref, &grant.place_ref);
        }
        other => panic!("expected an unprovable-binding refusal, got {other:?}"),
    }
    assert!(
        workcell.calls().is_empty(),
        "nothing may be sent to Workcell without proof tokens"
    );
}

#[test]
fn release_refuses_foreign_grants_before_any_call() {
    let mut foreign = serde_json::from_str::<Value>(&tmux_grant_json()).unwrap();
    foreign["schema"] = json!("workcell.place-grant/v2");
    let workcell = FakeWorkcell::new(&foreign.to_string());
    let grant = parse_grant(&foreign.to_string());
    let error = release_place(&release(&workcell, &grant))
        .expect_err("a grant outside the pinned contract is never released");
    assert!(matches!(error, PlaceError::InvalidInvocation(_)));
    assert!(workcell.calls().is_empty());
}

// --- Attempt-record integration: provenance, not identity -------------------

fn run_factory(args: &[String], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_factory"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "factory command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn caller() -> FactoryActionCaller {
    FactoryActionCaller {
        caller_ref: "agent:place-test".into(),
        projection_kind: FactoryActionProjectionKind::Headless,
        lineage: vec!["agent:place-test".into()],
    }
}

fn authority() -> ProjectedFactoryActionAuthority {
    ProjectedFactoryActionAuthority {
        authority_ref: "authority:place-test".into(),
        native_owner: "factory".into(),
        capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
        capability_granted: true,
        action_authorised: true,
    }
}

fn owner_receipt(
    operation_ref: &str,
    receipt_ref: &str,
    phase: OwnerOperationPhase,
    evidence_refs: BTreeSet<String>,
    partial_effect_refs: BTreeSet<String>,
) -> OwnerOperationReceipt {
    OwnerOperationReceipt {
        owner_ref: "aikit/session-space".into(),
        contract: "aikit.encounter-delivery/v1".into(),
        operation_ref: operation_ref.into(),
        receipt_ref: receipt_ref.into(),
        source_revision: "controlled-test-owner-revision".into(),
        phase,
        evidence_refs,
        partial_effect_refs,
        payload: json!({"testOnly": true}),
    }
}

fn set<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(Into::into).collect()
}

/// Find the first nested JSON object holding `key` with exactly `value`.
fn find_object_with_key<'a>(value: &'a Value, key: &str, expected: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if map.get(key).and_then(Value::as_str) == Some(expected) {
                return Some(value);
            }
            map.values()
                .find_map(|nested| find_object_with_key(nested, key, expected))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|nested| find_object_with_key(nested, key, expected)),
        _ => None,
    }
}

/// The same fixture world the other attempt tests use: an init'd native
/// attempt store over the shared agent workflow source.
struct World {
    dir: tempfile::TempDir,
    run: Run,
    source: WorkflowSource,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let run = Run::new(
            RUN.parse::<RunRef>().unwrap(),
            PROJECT.parse().unwrap(),
            "place placement controlled world",
            "factory-test",
        )
        .unwrap();
        let source: WorkflowSource = serde_json::from_str(include_str!(
            "../../contracts/factory/fixtures/agent-workflow-source.json"
        ))
        .unwrap();
        let state = dir.path().join("state.json");
        let seed = serde_json::to_value(FactoryAttemptSeed {
            run: run.clone(),
            workflow_source: source.clone(),
        })
        .unwrap();
        assert_success(&run_factory(
            &[
                "attempt".into(),
                "init".into(),
                state.display().to_string(),
                "-".into(),
                "--json".into(),
            ],
            Some(&seed.to_string()),
        ));
        Self { dir, run, source }
    }

    fn state(&self) -> PathBuf {
        self.dir.path().join("state.json")
    }

    fn raw_state(&self) -> String {
        fs::read_to_string(self.state()).unwrap()
    }

    fn reading(&self) -> FactoryAttemptReading {
        let output = run_factory(
            &[
                "attempt".into(),
                "read".into(),
                self.state().display().to_string(),
                "--json".into(),
            ],
            None,
        );
        assert_success(&output);
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn action(&self, operation: FactoryAttemptOperation) -> Result<Value, String> {
        let reading = self.reading();
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: "projection:attempt-place-test".into(),
            caller: caller(),
            run_ref: self.run.reference().clone(),
            expected_revision: reading.revision,
            authority: authority(),
            operation,
        };
        let output = run_factory(
            &[
                "attempt".into(),
                "action".into(),
                self.state().display().to_string(),
                "-".into(),
                "--json".into(),
            ],
            Some(&serde_json::to_string(&request).unwrap()),
        );
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
    }

    fn disposition(&self, key: &str, retry: Option<&RetryGrant>) -> SituatedExecutionDisposition {
        let workflow = compile_workflow(self.source.clone()).unwrap();
        let unit = workflow.unit(key).unwrap();
        let demand = ExecutionDemand {
            project_ref: self.run.project_ref().to_string(),
            run_ref: self.run.reference().to_string(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some("agency:place-test".into()),
            profile_ref: None,
            use_type: "controlled-place-test".into(),
            required_capabilities: unit.capability_refs.clone(),
            required_modalities: BTreeSet::from(["text".into()]),
            required_actions: BTreeSet::new(),
            required_tools: BTreeSet::new(),
            context_characteristics: BTreeSet::new(),
            independence_from: BTreeSet::new(),
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            requires_local_materialisation: false,
        };
        let selection = accept_aikit_selection(
            demand,
            AikitModelRosterSelection {
                roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
                model_ref: "model:place-test".into(),
                provider_ref: "provider:place-test".into(),
                ranking_policy: "controlled-test".into(),
                ranking_explanation: json!({"testOnly": true}),
                provenance: vec!["selection:place-test".into()],
            },
            "2026-09-15T09:30:00+01:00",
        )
        .unwrap();
        SituatedExecutionDisposition {
            selected_inputs: Vec::new(),
            selection,
            participant: SituatedParticipant {
                agent_ref: unit
                    .agent_requirements
                    .agent_refs
                    .iter()
                    .next()
                    .cloned()
                    .unwrap_or_else(|| "agent:place-test".into()),
                agency_ref: "agency:place-test".into(),
                world_binding_ref: "world-binding:place-test".into(),
                profile_ref: None,
                position_ref: None,
                source_ref: workflow.source.reference.to_string(),
                source_revision: workflow.source.revision.clone(),
                source_digest: format!("blake3:{}", workflow.source.digest),
            },
            context_refs: BTreeSet::from(["context:place-test".into()]),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            body: ExecutionBody {
                model_ref: "model:place-test".into(),
                provider_ref: "provider:place-test".into(),
                route_ref: "route:place-test".into(),
                harness_ref: "harness:place-test".into(),
                harness_composition_ref: "harness-composition:place-test".into(),
                agent_session_ref: "agent-session:place-test".into(),
                session_space_ref: "session-space:place-test".into(),
                material_world_ref: None,
                workcell_ref: None,
            },
            placement: None,
            permitted_effects: unit.permitted_effects.clone(),
            verification_obligations: unit.verification_obligations.clone(),
            return_address: unit.return_address.clone(),
            stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(),
            budget: ExecutionBudget {
                cost_ceiling_usd: None,
                latency_preference_ms: None,
                wall_clock_timeout_ms: Some(120_000),
                retry_grant_ref: retry.map(|grant| grant.grant_ref.clone()),
                maximum_attempts: retry.map(|grant| grant.attempts_allowed),
            },
        }
    }
}

#[test]
fn a_place_is_provenance_not_identity_and_absence_stays_byte_identical() {
    // One world requests a room through the pinned contract and authors the
    // returned grant onto its attempt; the other never asks for a place.
    let workcell = FakeWorkcell::new(&tmux_grant_json());
    let placed = World::new();
    let granted = request_place(&request(
        &workcell,
        PlaceProviderPolicy::Auto,
        // The room name deliberately repeats the attempt ref: identity must
        // not notice.
        "attempt:placed",
    ))
    .unwrap();
    let unit = compile_workflow(placed.source.clone())
        .unwrap()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    placed
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:placed".into(),
            task_ref: "task:placed".into(),
            parent_journey_ref: "journey:place".into(),
            workflow_unit_ref: unit.clone(),
            disposition: placed.disposition("inspect-source", None),
            retry_grant: None,
            tracking: Vec::new(),
            place_grant: Some(granted.clone()),
        })
        .unwrap();

    let bare = World::new();
    bare.action(FactoryAttemptOperation::StartSerial {
        attempt_ref: "attempt:placed".into(),
        task_ref: "task:placed".into(),
        parent_journey_ref: "journey:place".into(),
        workflow_unit_ref: unit.clone(),
        disposition: bare.disposition("inspect-source", None),
        retry_grant: None,
        tracking: Vec::new(),
        place_grant: None,
    })
    .unwrap();

    // The grant is recorded as provenance under its native contract names.
    let raw = placed.raw_state();
    assert!(raw.contains("placeGrant"));
    let state: Value = serde_json::from_str(&raw).unwrap();
    let stored = find_object_with_key(&state, "place_ref", &granted.place_ref)
        .expect("the granted room is stored somewhere in the attempt state");
    let mut keys: Vec<&str> = stored
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "created_utc",
            "pane_id",
            "pane_pid",
            "place_ref",
            "process_start_marker",
            "provider",
            "schema",
            "session_name",
        ],
        "the native workcell.place-grant/v1 names travel verbatim inside the record"
    );

    // Read-back equality: the record carries exactly the granted room.
    let placed_reading = placed.reading();
    assert_eq!(placed_reading.attempts.len(), 1);
    assert_eq!(placed_reading.attempts[0].place_grant, Some(granted));

    // Identity lives in typed artifacts: attempt and execution refs derive
    // only from the authored attempt ref, identically with and without a room.
    for reading in [&placed_reading, &bare.reading()] {
        let record = &reading.attempts[0];
        assert_eq!(record.attempt_ref, "attempt:placed");
        assert_eq!(
            record.reserved_execution_ref,
            "factory-attempt:attempt:placed"
        );
        assert_eq!(record.task_ref, "task:placed");
        assert_eq!(record.execution_ref, None);
    }
    assert_eq!(bare.reading().attempts[0].place_grant, None);

    // An attempt without a place serialises byte-identically to earlier
    // state: no placeGrant key exists anywhere in the store.
    let bare_raw = bare.raw_state();
    assert!(
        !bare_raw.contains("placeGrant"),
        "absence of a place must not add provenance keys: {bare_raw}"
    );
}

#[test]
fn authored_grant_outside_the_pinned_contract_is_refused_at_the_boundary() {
    let placed = World::new();
    let mut foreign = serde_json::from_str::<Value>(&tmux_grant_json()).unwrap();
    foreign["schema"] = json!("workcell.place-grant/v2");
    let unit = compile_workflow(placed.source.clone())
        .unwrap()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    let error = placed
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:foreign".into(),
            task_ref: "task:foreign".into(),
            parent_journey_ref: "journey:place".into(),
            workflow_unit_ref: unit,
            disposition: placed.disposition("inspect-source", None),
            retry_grant: None,
            tracking: Vec::new(),
            place_grant: Some(parse_grant(&foreign.to_string())),
        })
        .expect_err("a grant outside the pinned contract is never admitted as provenance");
    assert!(
        error.contains("authored place grant refused"),
        "unexpected refusal: {error}"
    );
}

#[test]
fn a_retry_never_inherits_a_room() {
    let workcell = FakeWorkcell::new(&tmux_grant_json());
    let world = World::new();
    let grant = RetryGrant::new("grant:place", 2).unwrap();
    let unit = compile_workflow(world.source.clone())
        .unwrap()
        .unit("inspect-source")
        .unwrap()
        .reference
        .clone();
    let disposition = world.disposition("inspect-source", Some(&grant));
    let source_revision = disposition.participant.source_revision.clone();
    world
        .action(FactoryAttemptOperation::StartSerial {
            attempt_ref: "attempt:first".into(),
            task_ref: "task:place".into(),
            parent_journey_ref: "journey:place".into(),
            workflow_unit_ref: unit.clone(),
            disposition: disposition.clone(),
            retry_grant: Some(grant.clone()),
            tracking: Vec::new(),
            place_grant: Some(
                request_place(&request(
                    &workcell,
                    PlaceProviderPolicy::Auto,
                    "attempt-room",
                ))
                .unwrap(),
            ),
        })
        .unwrap();
    world
        .action(FactoryAttemptOperation::BindDispatch {
            attempt_ref: "attempt:first".into(),
            execution_ref: "execution:first".into(),
            receipt: owner_receipt(
                "delivery:first",
                "receipt:first",
                OwnerOperationPhase::Uncertain,
                set(["evidence:lost-ack"]),
                set(["effect:possible-provider-turn"]),
            ),
        })
        .unwrap();
    world
        .action(FactoryAttemptOperation::RecordObservation {
            attempt_ref: "attempt:first".into(),
            receipt: owner_receipt(
                "delivery:first",
                "receipt:reconcile",
                OwnerOperationPhase::ReconciledNoReplay,
                set(["evidence:operator-native-reconcile"]),
                BTreeSet::new(),
            ),
        })
        .unwrap();
    world
        .action(FactoryAttemptOperation::Fail {
            attempt_ref: "attempt:first".into(),
            reason: "reconciled as no replay; attempt failed".into(),
            evidence_refs: set([
                "evidence:operator-native-reconcile",
                "effect:possible-provider-turn",
            ]),
        })
        .unwrap();
    world
        .action(FactoryAttemptOperation::Retry {
            attempt_ref: "attempt:second".into(),
            task_ref: "task:place".into(),
            parent_journey_ref: "journey:place".into(),
            workflow_unit_ref: unit,
            grant_ref: grant.grant_ref.clone(),
            disposition,
            tracking: Vec::new(),
            reresolution: Some(ReresolutionRecord {
                resolution_ref: "resolution:place-retry".into(),
                reason: "owner delivery reconciled without replay".into(),
                source_revision,
                evidence_refs: set([
                    "evidence:operator-native-reconcile",
                    "effect:possible-provider-turn",
                ]),
                replacement_now_ref: None,
                replacement_material_ref: None,
                replacement_harness_ref: None,
            }),
            place_grant: None,
        })
        .unwrap();

    let reading = world.reading();
    assert_eq!(reading.attempts.len(), 2);
    let first = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == "attempt:first")
        .unwrap();
    let second = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == "attempt:second")
        .unwrap();
    assert!(
        first.place_grant.is_some(),
        "the attempt that held the room keeps its provenance"
    );
    assert_eq!(
        second.place_grant, None,
        "a successor attempt never inherits a predecessor's place implicitly"
    );
    assert_eq!(
        second.reserved_execution_ref,
        "factory-attempt:attempt:second"
    );
}

#[test]
fn the_full_arc_requests_a_room_and_ends_it_as_a_semantic_act() {
    // The full lifecycle over one fake Workcell: request, then release with
    // the grant's own proof tokens, then a second release whose stale proof
    // is disclosed once, without retry.
    let workcell = FakeWorkcell::new(&tmux_grant_json());
    let grant = request_place(&request(
        &workcell,
        PlaceProviderPolicy::Tmux,
        "attempt-room",
    ))
    .unwrap();
    assert_eq!(workcell.calls().len(), 1);
    release_place(&release(&workcell, &grant)).expect("the granted room ends as a semantic act");
    workcell.set_mode("stale");
    let error = release_place(&release(&workcell, &grant))
        .expect_err("the room is already ended; the stale proof is disclosed");
    assert!(refused("release", &error).stderr.contains("stale-binding"));
    assert_eq!(
        workcell.calls().len(),
        3,
        "one request, two releases, no retries"
    );
}
