//! Public sensing behavior over the real native developmental provider. These
//! tests run the shipped CLI, reopen its persisted state, and never substitute
//! an in-memory provider or a mock Actuation response.

use epilogos_factory::commission::{
    BoundedRootAct, FactoryCommissionRequest, FactoryParticipantRequirement,
    FACTORY_COMMISSION_REQUEST,
};
use epilogos_factory::conformance::create_developmental_conformance_state;
use epilogos_factory::developmental_read::{FactoryDevelopmentalFileProvider, FactoryGitBasis};
use epilogos_factory::project_development_store::transact_developmental_state;
use epilogos_factory::sensing::{retain, Observation, WorkRelation};
use epilogos_factory::work_custody::{
    assign_in, update_in, AssignRequest, CustodyState, UpdateRequest,
};
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .output()
        .expect("Factory binary runs")
}

fn run_with_authority_store(args: &[&str], store: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .env("ACTUATION_AUTHORITY_STORE", store)
        .output()
        .expect("Factory binary runs")
}

fn issue_authority(store: &Path, world: &str, action: &str, temp: &Path) -> Value {
    let broad = format!("bound:factory-sensing:{world}");
    let narrow = format!("{broad}:{}", action.trim_start_matches("factory:action/"));
    let bounds = vec![broad, narrow];
    let record = json!({
        "schema":"actuation.local-authority/v1",
        "authority_source_ref":"authority-source:factory-sensing-test",
        "holder":"human:owner", "issued_by":"human:owner",
        "governing_binding":{
            "schema":"actuation.agency/v1", "binding_ref":"binding:sensing-governor",
            "agent_ref":"agent:sensing-governor", "agency_ref":"agency:sensing-governor",
            "world_ref":"control:root", "scope_ref":"control:root",
            "bounds_refs":bounds, "authority_refs":["authority:metagency","authority:factory-sensing"],
            "return_relation_ref":"return-relation:factory-sensing"
        },
        "metagency_grant":{
            "schema":"actuation.agency/v1", "grant_ref":"grant:factory-sensing-test",
            "agency_ref":"agency:sensing-governor", "world_binding_ref":"binding:sensing-governor",
            "authority_ref":"authority:metagency", "bounds_refs":bounds,
            "operations":["determine-agency","actualise-agency"]
        },
        "allowed_world_refs":[world],
        "issued_at_unix_seconds": chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).timestamp()
    });
    let path = temp.join("authority.json");
    std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    let issued = Command::new("actuation")
        .args([
            "authority",
            "issue",
            "--store",
            store.to_str().unwrap(),
            path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("installed Actuation runs");
    assert!(
        issued.status.success(),
        "Actuation issue failed: {}",
        String::from_utf8_lossy(&issued.stderr)
    );
    json!({
        "schema":"actuation.local-authority/v1",
        "resolution_ref":"resolution:factory-sensing-test",
        "request_ref":"actualisation-request:factory-sensing-test",
        "requester_ref":"human:owner",
        "authority_source_ref":"authority-source:factory-sensing-test",
        "determination_ref":"determination:factory-sensing-test",
        "differentiated_binding":{
            "schema":"actuation.agency/v1", "binding_ref":"binding:sensing-agent",
            "agent_ref":"agent:sensing-agent", "agency_ref":"agency:sensing-agent",
            "world_ref":world, "scope_ref":world,
            "determining_agency_ref":"agency:sensing-governor",
            "bounds_refs":bounds, "authority_refs":["authority:factory-sensing"],
            "return_relation_ref":"return-relation:factory-sensing",
            "continuity_ref":"continuity:sensing-agent"
        },
        "agent_identity":{"standing":"existing","evidence_refs":["evidence:identity-registry"]},
        "requested_bounds_refs":bounds,
        "delegated_autonomy":{"allowed_action_refs":[action],"denied_action_refs":[],"may_determine_within_bounds":true},
        "return_policy":{"mode":"required","return_relation_ref":"return-relation:factory-sensing"},
        "provenance":{"source_refs":["source:factory-sensing-test"],"context_refs":[]}
    })
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "Factory refused: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("Factory returned JSON")
}

fn policy(path: &Path, world: &str) {
    let document = json!({
        "schema": "factory.sensing-policy/v1",
        "version": 1,
        "project_world_ref": world,
        "sources": [
            {"id":"native-anomalies","provider":"factory","scope":world,"source_ref":"factory:conformance-correlations","arguments":{"kind":"correlations"}},
            {"id":"native-empty","provider":"factory","scope":world,"source_ref":"factory:conformance-custody","arguments":{"kind":"custody"}},
            {"id":"unsupported","provider":"sentry","scope":world,"source_ref":"provider:sentry:unconfigured"}
        ],
        "workflows": {"collect":{"enabled":true,"sources":["native-anomalies","native-empty","unsupported"]}}
    });
    std::fs::write(path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
}

#[test]
fn public_collect_preserves_coverage_and_reads_do_not_mutate_native_state() {
    let temp = tempfile::TempDir::new().unwrap();
    let state = temp.path().join("state.json");
    let manifest = create_developmental_conformance_state(&state).unwrap();
    let policy_path = temp.path().join("factory-policy.json");
    let world = manifest.project_ref.to_string();
    policy(&policy_path, &world);
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let since = (now - chrono::Duration::hours(1)).to_rfc3339();
    let until = (now + chrono::Duration::hours(1)).to_rfc3339();
    let state_s = state.to_str().unwrap();
    let policy_s = policy_path.to_str().unwrap();
    let collection = success(run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        policy_s,
        "--since",
        &since,
        "--until",
        &until,
        "--json",
    ]));
    assert_eq!(collection["schema"], "factory.signal-collection/v1");
    let coverage = collection["collection"]["coverage"].as_array().unwrap();
    assert_eq!(coverage.len(), 3);
    assert_eq!(coverage[0]["state"], "truncated");
    assert_eq!(coverage[1]["state"], "empty");
    assert_eq!(coverage[2]["state"], "unavailable");
    assert_eq!(
        collection["collection"]["signal_refs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let saved = std::fs::read(&state).unwrap();
    let replay = success(run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        policy_s,
        "--since",
        &since,
        "--until",
        &until,
        "--json",
    ]));
    assert_eq!(
        replay["collection"]["collection_ref"],
        collection["collection"]["collection_ref"]
    );
    assert_eq!(
        std::fs::read(&state).unwrap(),
        saved,
        "a replay must not append a second collection"
    );

    let field = success(run(&["telemetry", "field", state_s, "--json"]));
    assert_eq!(field["schema"], "factory.telemetry-field/v1");
    assert_eq!(field["counts"]["signals"], 1);
    assert_eq!(field["signals"][0]["classification"], "needs-evidence");
    let day = success(run(&[
        "telemetry",
        "day",
        state_s,
        "--since",
        &since,
        "--until",
        &until,
        "--json",
    ]));
    assert_eq!(day["read_only"], true);
    assert_eq!(day["counts"]["records"], 1);
    assert_eq!(day["source_coverage_complete"], false);
    let digest = success(run(&["telemetry", "digest", state_s, "--json"]));
    assert_eq!(digest["read_only"], true);
    assert_eq!(digest["counts"]["records"], 0);
    assert_eq!(std::fs::read(&state).unwrap(), saved);

    let wrong = temp.path().join("wrong-policy.json");
    policy(&wrong, "project:another-world");
    let refused = run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        wrong.to_str().unwrap(),
        "--json",
    ]);
    assert!(!refused.status.success());
    assert_eq!(std::fs::read(&state).unwrap(), saved);
}

#[test]
fn public_classification_requires_native_authority_and_keeps_revision() {
    let temp = tempfile::TempDir::new().unwrap();
    let state = temp.path().join("state.json");
    let manifest = create_developmental_conformance_state(&state).unwrap();
    let policy_path = temp.path().join("factory-policy.json");
    policy(&policy_path, &manifest.project_ref.to_string());
    let state_s = state.to_str().unwrap();
    let collected = success(run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        policy_path.to_str().unwrap(),
        "--json",
    ]));
    let reference = collected["collection"]["signal_refs"][0].as_str().unwrap();
    let before = std::fs::read(&state).unwrap();
    let request_path = temp.path().join("classify.json");
    std::fs::write(
        &request_path,
        serde_json::to_vec(&json!({
            "expected_revision": 1,
            "signal_ref": reference,
            "source_revision": "a claimant cannot establish the revision",
            "classification": "verified-defect",
            "evidence_refs": ["evidence:claim"],
            "reason": "A claim without native authority must not change the signal"
        }))
        .unwrap(),
    )
    .unwrap();
    let refused = run(&[
        "telemetry",
        "classify",
        state_s,
        "--request",
        request_path.to_str().unwrap(),
        "--json",
    ]);
    assert!(!refused.status.success());
    assert_eq!(std::fs::read(&state).unwrap(), before);
    let field = success(run(&["telemetry", "field", state_s, "--json"]));
    assert_eq!(field["source_sequence"], 1);
    assert_eq!(field["signals"][0]["classification"], "needs-evidence");
}

#[test]
fn admitted_actuation_grant_classifies_once_and_stale_revision_refuses() {
    if Command::new("actuation").arg("--version").output().is_err() {
        eprintln!("Actuation binary unavailable; native authority integration requires an installed suite");
        return;
    }
    let temp = tempfile::TempDir::new().unwrap();
    let state = temp.path().join("state.json");
    let manifest = create_developmental_conformance_state(&state).unwrap();
    let world = manifest.project_ref.to_string();
    let policy_path = temp.path().join("factory-policy.json");
    policy(&policy_path, &world);
    let state_s = state.to_str().unwrap();
    let collected = success(run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        policy_path.to_str().unwrap(),
        "--json",
    ]));
    let signal_ref = collected["collection"]["signal_refs"][0].as_str().unwrap();
    let signal = success(run(&["telemetry", "signal", state_s, signal_ref, "--json"]));
    let revision = signal["signal"]["observation"]["source_revision"]
        .as_str()
        .unwrap();
    let store = temp.path().join("authority-store");
    let authority = issue_authority(
        &store,
        &world,
        "factory:action/telemetry.classify",
        temp.path(),
    );
    let request = json!({
        "expected_revision":1,"signal_ref":signal_ref,"source_revision":revision,
        "classification":"human-decision","evidence_refs":["evidence:human-choice"],
        "reason":"This source needs a human policy judgement", "decision_needed":"Choose the intended product behavior",
        "authority_request":authority
    });
    let path = temp.path().join("classify-authorised.json");
    std::fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
    let first = success(run_with_authority_store(
        &[
            "telemetry",
            "classify",
            state_s,
            "--request",
            path.to_str().unwrap(),
            "--json",
        ],
        &store,
    ));
    assert_eq!(first["revision"], 2);
    let saved = std::fs::read(&state).unwrap();
    let stale = run_with_authority_store(
        &[
            "telemetry",
            "classify",
            state_s,
            "--request",
            path.to_str().unwrap(),
            "--json",
        ],
        &store,
    );
    assert!(!stale.status.success());
    assert_eq!(std::fs::read(&state).unwrap(), saved);
    let digest = success(run(&["telemetry", "digest", state_s, "--json"]));
    assert_eq!(digest["counts"]["records"], 1);
    assert_eq!(
        digest["signals"][0]["decision_needed"],
        "Choose the intended product behavior"
    );
}

#[test]
fn prior_observation_is_read_back_at_its_original_cut() {
    let temp = tempfile::TempDir::new().unwrap();
    let state = temp.path().join("state.json");
    let manifest = create_developmental_conformance_state(&state).unwrap();
    let world = manifest.project_ref.to_string();
    let policy_path = temp.path().join("policy.json");
    policy(&policy_path, &world);
    let state_s = state.to_str().unwrap();
    let policy_s = policy_path.to_str().unwrap();
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let since = (now - chrono::Duration::hours(1)).to_rfc3339();
    let until = (now + chrono::Duration::hours(1)).to_rfc3339();
    let first = success(run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        policy_s,
        "--since",
        &since,
        "--until",
        &until,
        "--json",
    ]));
    let reference = first["collection"]["signal_refs"][0].as_str().unwrap();
    let original = success(run(&["telemetry", "signal", state_s, reference, "--json"]));
    let original_revision = original["signal"]["observation"]["source_revision"]
        .as_str()
        .unwrap()
        .to_owned();
    let first_observed = original["signal"]["observation"]["observed_at_unix_ms"]
        .as_i64()
        .unwrap();

    // Change the actual native correlation through the provider's locked
    // transaction. The next collection observes a distinct owner revision.
    transact_developmental_state(&state, |native| {
        native.execution_correlations[0].git_basis = Some(FactoryGitBasis {
            repository: "EpiLogos/Factory".into(),
            base_head: "0123456789abcdef".into(),
            branch: Some("review/test".into()),
            worktree_clean: Some(true),
        });
        Ok(())
    })
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let second = success(run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        policy_s,
        "--since",
        &since,
        "--until",
        &until,
        "--json",
    ]));
    assert_ne!(
        first["collection"]["collection_ref"],
        second["collection"]["collection_ref"]
    );
    let current = success(run(&["telemetry", "signal", state_s, reference, "--json"]));
    assert_eq!(
        current["signal"]["prior_observations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_ne!(
        current["signal"]["observation"]["source_revision"],
        original_revision
    );
    let second_observed = current["signal"]["observation"]["observed_at_unix_ms"]
        .as_i64()
        .unwrap();
    assert!(second_observed > first_observed);
    let cutoff = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(first_observed + 1)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let historical = success(run(&[
        "telemetry",
        "day",
        state_s,
        "--since",
        &since,
        "--until",
        &cutoff,
        "--json",
    ]));
    assert_eq!(historical["counts"]["records"], 1);
    assert_eq!(
        historical["signals"][0]["source_revision"],
        original_revision
    );
}

#[test]
fn historical_coverage_uses_queried_interval_not_collection_clock() {
    let temp = tempfile::TempDir::new().unwrap();
    let state = temp.path().join("state.json");
    let manifest = create_developmental_conformance_state(&state).unwrap();
    let world = manifest.project_ref.to_string();
    let policy_path = temp.path().join("empty-policy.json");
    let policy = json!({
        "schema":"factory.sensing-policy/v1", "version":1, "project_world_ref":world,
        "sources":[{"id":"custody","provider":"factory","scope":world,
                    "source_ref":"factory:conformance-custody","arguments":{"kind":"custody"}}],
        "workflows":{"collect":{"enabled":true,"sources":["custody"]}}
    });
    std::fs::write(&policy_path, serde_json::to_vec(&policy).unwrap()).unwrap();
    let state_s = state.to_str().unwrap();
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let since = (now - chrono::Duration::days(2)).to_rfc3339();
    let until = (now - chrono::Duration::days(1)).to_rfc3339();
    let collected = success(run(&[
        "telemetry",
        "collect",
        state_s,
        "--policy",
        policy_path.to_str().unwrap(),
        "--since",
        &since,
        "--until",
        &until,
        "--json",
    ]));
    assert_eq!(collected["collection"]["coverage"][0]["state"], "empty");
    let day = success(run(&[
        "telemetry",
        "day",
        state_s,
        "--since",
        &since,
        "--until",
        &until,
        "--json",
    ]));
    assert_eq!(day["coverage"].as_array().unwrap().len(), 1);
    assert_eq!(day["source_coverage_complete"], true);
    assert_eq!(day["counts"]["records"], 0);
}

#[test]
fn admitted_return_authority_cannot_substitute_claimed_evidence_for_native_attempt() {
    if Command::new("actuation").arg("--version").output().is_err() {
        eprintln!("Actuation binary unavailable; native authority integration requires an installed suite");
        return;
    }
    let temp = tempfile::TempDir::new().unwrap();
    let state = temp.path().join("state.json");
    let fixture_now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let commissioned_at = (fixture_now - chrono::Duration::hours(2)).to_rfc3339();
    let historical_until = (fixture_now - chrono::Duration::hours(1)).to_rfc3339();
    let historical_since = (fixture_now - chrono::Duration::hours(3)).to_rfc3339();
    let commission = FactoryCommissionRequest {
        contract: FACTORY_COMMISSION_REQUEST.into(),
        request_ref: "commission-request:sensing-return-native".into(),
        project_key: "sensing-return-native".into(),
        purpose: "Verify a signal Return against native owner evidence".into(),
        frontier: "Signal observed; Attempt not yet admitted".into(),
        run_destination: "factory/sensing-return".into(),
        write_owner: "factory".into(),
        commissioned_at: commissioned_at.clone(),
        central_composition: None,
        root_act: BoundedRootAct {
            act_ref: "act:sensing-return-native".into(),
            agent_ref: "agent:sensing-test".into(),
            purpose: "Verify only the native evidence for this signal".into(),
            scope_refs: vec!["source:commission-test".into()],
            standing: "commissioned-not-executed".into(),
        },
        participant_requirements: vec![FactoryParticipantRequirement {
            reference: "agent:sensing-test".into(),
            description: "Inspect the native owner evidence".into(),
            source_owner: "factory".into(),
            source_ref: "source:commission-test".into(),
            source_revision: "revision:1".into(),
        }],
    };
    let admitted = FactoryDevelopmentalFileProvider::commission(&state, commission).unwrap();
    let world = admitted.commission.project_ref.to_string();
    let run_ref = admitted.commission.run_ref.clone();
    let journey_ref = admitted.commission.journey_ref.clone();
    let revision = format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(&admitted.commission).unwrap()).to_hex()
    );
    let signal_ref = transact_developmental_state(&state, |native| {
        native.sensing.project_world_ref = Some(world.clone());
        let observed_at =
            chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).timestamp_millis();
        let reference = retain(
            &mut native.sensing,
            &world,
            Observation {
                source_ref: admitted.commission.request.request_ref.clone(),
                provider_ref: "factory:commission".into(),
                source_revision: revision.clone(),
                // The authored Commission time predates Factory's observation.
                occurred_at_unix_ms: Some(
                    chrono::DateTime::parse_from_rfc3339(&commissioned_at)
                        .unwrap()
                        .timestamp_millis(),
                ),
                observed_at_unix_ms: observed_at,
                summary: "A real admitted Commission supplies the source basis".into(),
                dimension: "commission".into(),
                standing: "observed".into(),
                relation_refs: vec![run_ref.to_string()],
            },
        );
        let custody = assign_in(
            native,
            AssignRequest {
                position_ref: "central:position:project:sensing-return-native:test".into(),
                work_ref: reference.clone(),
                reason: "Verify signal work against this admitted Run".into(),
                run_ref: Some(run_ref.clone()),
                journey_ref: Some(journey_ref.clone()),
                ..Default::default()
            },
            observed_at,
        )
        .unwrap();
        native.sensing.signals.get_mut(&reference).unwrap().work = Some(WorkRelation {
            custody_ref: custody.custody.custody_ref,
            work_ref: reference.clone(),
            run_ref: Some(run_ref.to_string()),
            position_ref: custody.custody.position_ref,
            now_ref: None,
            authority_ref: "authority:commission-test".into(),
            created_at_unix_ms: observed_at,
        });
        native.sensing.revision += 1;
        Ok(reference)
    })
    .unwrap();
    let before_observed = success(run(&[
        "telemetry",
        "day",
        state.to_str().unwrap(),
        "--since",
        &historical_since,
        "--until",
        &historical_until,
        "--json",
    ]));
    assert_eq!(
        before_observed["counts"]["records"], 0,
        "a past event cannot appear in Factory's as-of reading before native observation"
    );
    let store = temp.path().join("authority-store");
    let authority = issue_authority(
        &store,
        &world,
        "factory:action/telemetry.return",
        temp.path(),
    );
    let request_path = temp.path().join("return.json");
    std::fs::write(
        &request_path,
        serde_json::to_vec(&json!({
            "expected_revision":1,
            "signal_ref":signal_ref,
            "return_ref":"return:caller-claim",
            "source_revision":revision,
            "change_revision":"revision:installed-claim",
            "evidence_refs":["evidence:caller-claim"],
            "installed_revision":"revision:installed-claim",
            "running_revision":"revision:installed-claim",
            "live_evidence_ref":"evidence:caller-claim",
            "attempt_ref":"attempt:caller-claim",
            "verification_ref":"verification:caller-claim",
            "outcome":"live-resolved",
            "authority_request":authority
        }))
        .unwrap(),
    )
    .unwrap();
    let before = std::fs::read(&state).unwrap();
    let refused = run_with_authority_store(
        &[
            "telemetry",
            "return",
            state.to_str().unwrap(),
            "--request",
            request_path.to_str().unwrap(),
            "--json",
        ],
        &store,
    );
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("native Attempt evidence"),
        "unexpected refusal: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(std::fs::read(&state).unwrap(), before);
    let policy_path = temp.path().join("custody-policy.json");
    std::fs::write(&policy_path, serde_json::to_vec(&json!({
        "schema":"factory.sensing-policy/v1", "version":1, "project_world_ref":world,
        "sources":[{"id":"custody","provider":"factory","scope":world,
            "source_ref":"factory:custody:sensing-return-native","arguments":{"kind":"custody"}}],
        "workflows":{"collect":{"enabled":true,"sources":["custody"]}}
    })).unwrap()).unwrap();
    let collected = success(run(&[
        "telemetry",
        "collect",
        state.to_str().unwrap(),
        "--policy",
        policy_path.to_str().unwrap(),
        "--json",
    ]));
    assert_eq!(
        collected["collection"]["signal_refs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let missing_ref = collected["collection"]["signal_refs"][0].as_str().unwrap();
    let missing = success(run(&[
        "telemetry",
        "signal",
        state.to_str().unwrap(),
        missing_ref,
        "--json",
    ]));
    assert!(missing["signal"]["observation"]["source_ref"]
        .as_str()
        .unwrap()
        .starts_with("factory:signal-now:"));
    assert_eq!(
        missing["signal"]["observation"]["summary"],
        "Commissioned work has no joined child NOW"
    );
    assert!(missing["signal"]["observation"]["relation_refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reference| reference == &signal_ref));

    // A current missing NOW join can outlive the selected interval. Its
    // owner-created timestamp and revision must remain factual in that read.
    let past = success(run(&[
        "telemetry",
        "collect",
        state.to_str().unwrap(),
        "--policy",
        policy_path.to_str().unwrap(),
        "--since",
        "2020-01-01T00:00:00Z",
        "--until",
        "2020-01-02T00:00:00Z",
        "--json",
    ]));
    assert_eq!(past["collection"]["signal_refs"][0], missing_ref);
    let retained = success(run(&[
        "telemetry",
        "signal",
        state.to_str().unwrap(),
        missing_ref,
        "--json",
    ]));
    assert_eq!(
        retained["signal"]["observation"]["source_revision"],
        missing["signal"]["observation"]["source_revision"]
    );
    assert_eq!(
        retained["signal"]["observation"]["occurred_at_unix_ms"],
        missing["signal"]["observation"]["occurred_at_unix_ms"]
    );
    assert!(retained["signal"]["observation"]["occurred_at_unix_ms"].is_number());
    assert_eq!(past["collection"]["coverage"][0]["state"], "truncated");

    let commissioned_signal = success(run(&[
        "telemetry",
        "signal",
        state.to_str().unwrap(),
        &signal_ref,
        "--json",
    ]));
    let custody_ref = commissioned_signal["signal"]["work"]["custody_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    transact_developmental_state(&state, |native| {
        update_in(
            native,
            UpdateRequest {
                custody_ref: custody_ref.clone(),
                state: Some(CustodyState::Completed),
                reason: "Owner marked the joined work complete without an Attempt Return".into(),
                ..Default::default()
            },
            chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).timestamp_millis(),
        )
        .unwrap();
        Ok(())
    })
    .unwrap();
    let completed = success(run(&[
        "telemetry",
        "collect",
        state.to_str().unwrap(),
        "--policy",
        policy_path.to_str().unwrap(),
        "--json",
    ]));
    let mut found_missing_return = false;
    for reference in completed["collection"]["signal_refs"].as_array().unwrap() {
        let signal = success(run(&[
            "telemetry",
            "signal",
            state.to_str().unwrap(),
            reference.as_str().unwrap(),
            "--json",
        ]));
        if signal["signal"]["observation"]["source_ref"]
            == format!("factory:missing-return:{custody_ref}")
        {
            found_missing_return = true;
            assert_eq!(
                signal["signal"]["observation"]["summary"],
                "Completed custody has no native readable Attempt Return"
            );
            assert!(signal["signal"]["observation"]["relation_refs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reference| reference == &run_ref.to_string()));
        }
    }
    assert!(
        found_missing_return,
        "completed native custody must surface its missing Return"
    );
}
