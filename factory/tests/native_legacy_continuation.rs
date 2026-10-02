//! Compatibility uses bytes produced by the real installed Factory 34b owner.
//! No mocked provider, worker, Agency, model or whole completion is constructed.
use epilogos_factory::project_development_store::read_developmental_state;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Case {
    state: &'static [u8],
    request: &'static [u8],
    provenance: &'static [u8],
}
fn cases() -> [Case; 3] {
    [
        Case {
            state: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-opaque-state.json"
            ),
            request: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-opaque-request.json"
            ),
            provenance: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-opaque-provenance.json"
            ),
        },
        Case {
            state: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-authored-basis-state.json"
            ),
            request: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-authored-basis-request.json"
            ),
            provenance: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-authored-basis-provenance.json"
            ),
        },
        Case {
            state: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-both-complete-state.json"
            ),
            request: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-both-complete-request.json"
            ),
            provenance: include_bytes!(
                "../../contracts/factory/fixtures/native-legacy-both-complete-provenance.json"
            ),
        },
    ]
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}
fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value[field].as_str().unwrap()
}
fn copy_case(case: &Case) -> (tempfile::TempDir, PathBuf, Value) {
    let provenance = value(case.provenance);
    assert_eq!(
        provenance["binarySha256"],
        "c1b663307336a096a4e84c62c88c0d04f52e0968536d42ce8e374172184b5202"
    );
    assert_eq!(
        provenance["reportedVersion"],
        "factory 0.1.0 (34b6143ce56d)"
    );
    assert_eq!(hash(case.state), provenance["state"]["sha256"]);
    assert_eq!(
        hash(case.request),
        provenance["continuationRequest"]["sha256"]
    );
    assert_eq!(provenance["workerOrModelExecuted"], false);
    assert_eq!(provenance["wholeCompletion"], false);
    assert_eq!(provenance["repairRunTouched"], false);
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("genuine-previous-owner.json");
    fs::write(&state, case.state).unwrap();
    (directory, state, provenance)
}
fn native(state: &Path, group: &str, verb: &str, reference: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_factory"))
        .args([group, verb])
        .arg(state)
        .args([reference, "--json"])
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .env_remove("FACTORY_NATIVE_CENTRAL_BINARY")
        .env_remove("FACTORY_NATIVE_CENTRAL_ROOT")
        .env_remove("FACTORY_NATIVE_CENTRAL_PROJECT")
        .output()
        .unwrap()
}
fn successful(output: Output) -> Value {
    assert!(
        output.status.success(),
        "actual Factory CLI refused\nstdout:{}\nstderr:{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    value(&output.stdout)
}
fn assert_source_tuple(reading: &Value, source: &Value) {
    assert_eq!(reading["workflowSourceRef"], source["ref"]);
    assert_eq!(reading["workflowSourceRevision"], source["revision"]);
    assert_eq!(reading["workflowSourceDigest"], source["digest"]);
    assert_eq!(reading["wholeRunState"], "incomplete");
    assert_eq!(reading["completionVerified"], false);
    assert!(reading["attempts"].as_array().unwrap().is_empty());
}
fn legacy_successor_ref(provenance: &Value) -> &str {
    if let Some(reference) = provenance["requiredLegacySuccessorBasisRef"].as_str() {
        reference
    } else {
        provenance["legacyBasisRefs"][1].as_str().unwrap()
    }
}
fn legacy_origin_ref(provenance: &Value) -> &str {
    if let Some(reference) = provenance["requiredLegacyOriginBasisRef"].as_str() {
        reference
    } else {
        provenance["legacyBasisRefs"][0].as_str().unwrap()
    }
}

#[test]
fn genuine_previous_owner_restart_and_traversal_retain_exact_raw_source_basis() {
    for case in cases() {
        let (_directory, state, provenance) = copy_case(&case);
        read_developmental_state(&state).expect("real earlier owner history remains readable");
        for (run_field, source_field) in [
            ("originRunRef", "originSource"),
            ("successorRunRef", "successorSource"),
        ] {
            let run = text(&provenance, run_field);
            let attempts = successful(native(&state, "attempt", "read", run));
            assert_source_tuple(&attempts, &provenance[source_field]);
            let read = successful(native(&state, "development", "run", run));
            assert_eq!(read["runRef"], run);
            assert_source_tuple(&read["nativeAttempts"], &provenance[source_field]);
        }
        let commission = successful(native(
            &state,
            "development",
            "commission-read",
            text(&provenance, "commissionRef"),
        ));
        assert_eq!(
            commission["commission"]["runRef"],
            provenance["originRunRef"]
        );
        assert!(commission["traversal"].as_array().unwrap().iter().any(|edge| {
            edge["relation"] == "source-basis-for"
                && edge["objectRef"] == provenance["successorRunRef"]
                && edge["subjectRef"] == legacy_successor_ref(&provenance)
        }), "historical traversal must select the old owner's complete ancestry spelling, even when encoded-looking context is also retained");
        assert_eq!(fs::read(&state).unwrap(), case.state);
    }
}

#[test]
fn genuine_previous_owner_continuation_exact_public_retry_does_not_rewrite_history() {
    for case in cases() {
        let (directory, state, provenance) = copy_case(&case);
        let request = directory.path().join("exact-previous-native-request.json");
        fs::write(&request, case.request).unwrap();
        let reply = successful(native(
            &state,
            "development",
            "mutate",
            request.to_str().unwrap(),
        ));
        assert_eq!(reply["status"], "already-applied");
        assert_eq!(fs::read(&state).unwrap(), case.state);
        // A second process restarts from exactly the same retained owner bytes.
        let repeated = successful(native(
            &state,
            "development",
            "mutate",
            request.to_str().unwrap(),
        ));
        assert_eq!(repeated["status"], "already-applied");
        assert_eq!(fs::read(&state).unwrap(), case.state);
        let attempts = successful(native(
            &state,
            "attempt",
            "read",
            text(&provenance, "successorRunRef"),
        ));
        assert_source_tuple(&attempts, &provenance["successorSource"]);
    }
}

#[test]
fn corrupt_complete_legacy_ancestry_is_refused_without_rewriting_owner_bytes() {
    for case in cases() {
        for strip_successor in [false, true] {
            let (_directory, state, provenance) = copy_case(&case);
            let mut corrupted = value(case.state);
            let journey = corrupted["state"]["journeys"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|journey| journey["ref"] == provenance["journeyRef"])
                .unwrap();
            let link = journey["runs"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|link| link["run_ref"] == provenance["successorRunRef"])
                .unwrap();
            let removed = if strip_successor {
                legacy_successor_ref(&provenance)
            } else {
                legacy_origin_ref(&provenance)
            };
            let basis = link["basis_refs"].as_array_mut().unwrap();
            assert!(basis.contains(&json!(removed)));
            basis.retain(|reference| reference != removed);
            if let Some(authored) = provenance["authoredEncodedBasisRef"].as_str() {
                assert!(basis.contains(&json!(authored)), "authored context alone must not substitute for a missing required ancestry member");
            }
            let bad_bytes = serde_json::to_vec_pretty(&corrupted).unwrap();
            fs::write(&state, &bad_bytes).unwrap();
            assert!(read_developmental_state(&state).is_err());
            let output = native(
                &state,
                "development",
                "commission-read",
                text(&provenance, "commissionRef"),
            );
            assert!(
                !output.status.success(),
                "the actual native reader admitted partial/mixed history"
            );
            assert!(
                !output.stderr.is_empty(),
                "native refusal must remain observable"
            );
            assert_eq!(fs::read(&state).unwrap(), bad_bytes);
        }
    }
}
