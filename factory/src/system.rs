//! Wave 5 System contribution: the `factory system` settings disclosure.
//!
//! Emits the O:I System settings descriptor (`oi.product-settings-disclosure/v2`,
//! `wave-5/system.1`) for `software-factory`. Exposes the configuration,
//! developmental state and canonical Actions the current Factory owner contract
//! genuinely supplies. The Factory<->Actuation discovery/intent/authority seam
//! is NOT represented here: it is not resolved in current native contracts and
//! is implemented independent of this descriptor.
//!
//! Read-only: this command performs no mutation and writes nothing.

use crate::action_projection::FACTORY_ACTION_PROJECTION_CONTRACT;
use crate::build::{
    FACTORY_BUILD_PROVIDER_CONTRACT, FACTORY_BUILD_VIEW_CONTRACT, REQUEST_MORE_EVIDENCE_ACTION_REF,
    REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
};
use crate::build_provider::FACTORY_BUILD_LOCAL_PROVIDER_STATE;
use crate::cli::FACTORY_CLI_CONTRACT;
use crate::developmental_read::{
    FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER, FACTORY_DEVELOPMENTAL_STATE_SCHEMA,
    FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT, FACTORY_JOURNEY_READING_CONTRACT,
    FACTORY_PROJECT_READING_CONTRACT, FACTORY_RUN_READING_CONTRACT,
    FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT, FACTORY_WORKFLOW_UNIT_READING_CONTRACT,
};
use crate::project_development::PROJECT_DEVELOPMENT_VERSION;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SCHEMA: &str = "oi.product-settings-disclosure/v2";
const PRODUCT_ID: &str = "software-factory";
const CONTRACT_REVISION: &str = "wave-5/system.1";
const OWNER_REF: &str = FACTORY_CLI_CONTRACT;

const ABOUT: &str = "Build and trace: the Factory Build view/provider over project bindings and \
Runs, developmental reads (project, journey, run, workflow units, execution telemetry), a Run \
development ledger, and the canonical Action seam (request-evidence). Native owner is `factory`.";

pub fn system_command(json_out: bool) -> Result<String, crate::cli::CliError> {
    let descriptor = build_descriptor()?;
    if json_out {
        return serde_json::to_string_pretty(&descriptor).map_err(crate::cli::CliError::from);
    }

    let availability = descriptor["availability"]["state"]
        .as_str()
        .unwrap_or("unknown");
    let build_provider = descriptor["sections"][0]["settings"][0]["axes"]["active"]["value"]
        .as_str()
        .unwrap_or("?");
    let developmental = descriptor["sections"][1]["settings"][0]["axes"]["active"]["value"]
        .as_str()
        .unwrap_or("?");
    let action_count = descriptor["actions"].as_array().map_or(0, Vec::len);

    Ok(format!(
        "software-factory System disclosure ({SCHEMA}):\n\
         build provider: {build_provider}\n\
         developmental state: {developmental}\n\
         canonical actions: {action_count}\n\
         availability: {availability}"
    ))
}

fn build_descriptor() -> Result<Value, crate::cli::CliError> {
    let observed = now_ms();

    let reading_contracts: Vec<&str> = vec![
        FACTORY_PROJECT_READING_CONTRACT,
        FACTORY_JOURNEY_READING_CONTRACT,
        FACTORY_RUN_READING_CONTRACT,
        FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT,
        FACTORY_WORKFLOW_UNIT_READING_CONTRACT,
        FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT,
    ];

    let sections = vec![
        json!({
            "id": "configuration",
            "title": "Build provider",
            "settings": [
                setting("build.provider-contract", "Build provider contract", "scalar", json!(FACTORY_BUILD_PROVIDER_CONTRACT), "factory build snapshot <state> <project-ref> <run-ref>", observed),
                setting("build.view-contract", "Build view contract", "scalar", json!(FACTORY_BUILD_VIEW_CONTRACT), "factory build snapshot <state> <project-ref> <run-ref>", observed),
                setting("build.local-provider-state", "Local provider state schema", "scalar", json!(FACTORY_BUILD_LOCAL_PROVIDER_STATE), "factory build snapshot <state> <project-ref> <run-ref>", observed),
                setting("build.action-projection-contract", "Action projection contract", "scalar", json!(FACTORY_ACTION_PROJECTION_CONTRACT), "factory action invoke <state> <project-ref> <run-ref> <request>", observed),
            ],
        }),
        json!({
            "id": "developmental-state",
            "title": "Developmental reads",
            "settings": [
                setting("development.state-schema", "Developmental state schema", "scalar", json!(FACTORY_DEVELOPMENTAL_STATE_SCHEMA), "factory development <operation> <state>", observed),
                setting("development.local-provider", "Local provider contract", "scalar", json!(FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER), "factory development <operation> <state>", observed),
                setting("development.reading-contracts", "Reading contracts", "table", json!(reading_contracts), "factory development <operation> <state>", observed),
                setting("development.ledger-contract", "Run development ledger contract", "scalar", json!(PROJECT_DEVELOPMENT_VERSION), "factory development observe <ledger-root> <run-ref>", observed),
            ],
        }),
    ];

    let actions = vec![
        json!({
            "action_ref": REQUEST_MORE_EVIDENCE_ACTION_REF,
            "title": "Request more evidence",
            "args": [
                {"name": "subject_ref", "kind": "string"},
                {"name": "run_ref", "kind": "string"},
            ],
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["candidate"],
            "authority": {
                "requires": [REQUEST_MORE_EVIDENCE_CAPABILITY_REF, "factory.action-authorised"],
                "granted_by": "caller",
                "evidence_ref": null,
            },
            "exposure": { "ui": true, "agent": true, "headless": true },
            "explain": { "ref": "factory action list", "command": ["factory", "action", "list", "<state>", "<project-ref>", "<run-ref>"] },
            "history": { "ref": "factory build snapshot", "command": ["factory", "build", "snapshot", "<state>", "<project-ref>", "<run-ref>"] },
        }),
        json!({
            "action_ref": "factory.development.observe",
            "title": "Record a development observation",
            "args": [
                {"name": "run_ref", "kind": "string"},
                {"name": "request", "kind": "reference"},
            ],
            "availability": "disclosed",
            "unavailable_reason": null,
            "subject_kinds": ["software-factory.run"],
            "authority": { "requires": [], "granted_by": "caller", "evidence_ref": null },
            "exposure": { "ui": true, "agent": true, "headless": true },
            "explain": { "ref": "factory development observe", "command": ["factory", "development", "observe", "<ledger-root>", "<run-ref>"] },
            "history": { "ref": "factory development observations", "command": ["factory", "development", "observations", "<ledger-root>", "<run-ref>"] },
        }),
    ];

    let body = json!({
        "schema": SCHEMA,
        "product_id": PRODUCT_ID,
        "contract_revision": CONTRACT_REVISION,
        "about": ABOUT,
        "sections": sections,
        "actions": actions,
        "availability": { "state": "available", "reason": null },
        "degradations": [],
        "obligations": [],
    });

    let mut descriptor = body;
    descriptor["disclosed_at_unix_ms"] = json!(observed);
    descriptor["owner"] = json!({
        "owner_id": PRODUCT_ID,
        "owner_ref": OWNER_REF,
        "owner_version": env!("CARGO_PKG_VERSION"),
        "reading_command": ["factory", "system", "--json"],
        "reading_digest": null,
        "reading_digest_covers": "descriptor with every *_unix_ms field zeroed (disclosed_at_unix_ms, owner.observed_at_unix_ms, every axes.*.provenance.observed_at_unix_ms) and owner.reading_digest set to null",
        "observed_at_unix_ms": observed,
    });

    // §4.5 canonical body: the whole descriptor with every `*_unix_ms` zeroed and
    // `owner.reading_digest` null, so two readings of an unchanged world hash the
    // same and a changed digest means a changed reading, never a changed clock.
    let mut canonical = descriptor.clone();
    zero_unix_ms(&mut canonical);
    if let Some(owner) = canonical.get_mut("owner").and_then(Value::as_object_mut) {
        owner.insert("reading_digest".into(), Value::Null);
    }
    let digest =
        sha256_hex(&serde_json::to_string(&canonical).map_err(crate::cli::CliError::from)?);
    descriptor["owner"]["reading_digest"] = json!(digest);
    Ok(descriptor)
}

fn setting(
    key: &str,
    title: &str,
    kind: &str,
    value: Value,
    native_path: &str,
    observed_at: i64,
) -> Value {
    json!({
        "key": key,
        "title": title,
        "kind": kind,
        "axes": axes(value, native_path, observed_at),
        "mutable": false,
        "native_path": native_path,
        "bootstrap": false,
        "drift": {
            "state": "none",
            "between": ["declared", "effective"],
            "remediation_action_ref": null,
        },
    })
}

fn axes(value: Value, native_path: &str, observed_at: i64) -> Value {
    let provenance = json!({
        "owner_ref": OWNER_REF,
        "path": native_path,
        "observed_at_unix_ms": observed_at,
    });
    json!({
        "declared":  { "value": value.clone(), "provenance": provenance.clone() },
        "effective": { "value": value.clone(), "provenance": provenance.clone() },
        "active":    { "value": value, "provenance": provenance, "materialisation_ref": null },
        "staged":    { "value": {}, "provenance": provenance.clone(), "stage_ref": null, "stage_state": "none" },
        "expected_effect": { "summary": "Nothing is staged: this Factory setting is disclosed read-only and is never staged for application, so applying it would change nothing.", "ref": null },
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Zero every `*_unix_ms` field in the descriptor, recursively, so the canonical
/// reading body is independent of when the reading was taken (§4.5).
fn zero_unix_ms(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map
                .keys()
                .filter(|key| key.ends_with("_unix_ms"))
                .cloned()
                .collect();
            for key in keys {
                map.insert(key, json!(0i64));
            }
            for (_, child) in map.iter_mut() {
                zero_unix_ms(child);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                zero_unix_ms(item);
            }
        }
        _ => {}
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> Value {
        let output = system_command(true).unwrap();
        serde_json::from_str(&output).unwrap()
    }

    #[test]
    fn disclosure_matches_frozen_v2_contract() {
        let value = descriptor();
        assert_eq!(value["schema"], SCHEMA);
        assert_eq!(value["product_id"], PRODUCT_ID);
        assert_eq!(value["contract_revision"], CONTRACT_REVISION);
        assert_eq!(value["owner"]["owner_id"], "software-factory");
        assert_eq!(value["owner"]["owner_ref"], OWNER_REF);
        assert_eq!(
            value["owner"]["reading_command"],
            json!(["factory", "system", "--json"])
        );
        assert_eq!(value["availability"]["state"], "available");
        assert!(value["disclosed_at_unix_ms"].is_i64());
        assert!(value["owner"]["observed_at_unix_ms"].is_i64());
    }

    #[test]
    fn digest_verifies_against_canonical_body() {
        let value = descriptor();
        let mut body = value.clone();
        zero_unix_ms(&mut body);
        body["owner"]["reading_digest"] = Value::Null;
        let recomputed = sha256_hex(&serde_json::to_string(&body).unwrap());
        assert_eq!(value["owner"]["reading_digest"], recomputed);
        let digest = value["owner"]["reading_digest"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        assert!(digest
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn every_setting_carries_all_axes_with_provenance() {
        let value = descriptor();
        let sections = value["sections"].as_array().unwrap();
        assert_eq!(sections.len(), 2);
        for section in sections {
            for setting in section["settings"].as_array().unwrap() {
                for axis in ["declared", "effective", "active"] {
                    assert!(
                        setting["axes"][axis]["provenance"]["owner_ref"].is_string(),
                        "missing provenance on {axis}"
                    );
                    assert!(setting["axes"][axis]["provenance"]["observed_at_unix_ms"].is_i64());
                }
                assert_eq!(setting["axes"]["staged"]["stage_state"], "none");
                assert_eq!(setting["axes"]["staged"]["value"], json!({}));
                let summary = setting["axes"]["expected_effect"]["summary"]
                    .as_str()
                    .unwrap();
                assert!(
                    !summary.is_empty() && summary != "none",
                    "expected_effect.summary must be a real sentence, not \"none\""
                );
                assert_eq!(setting["mutable"], false);
                assert_eq!(setting["drift"]["state"], "none");
            }
        }
    }

    #[test]
    fn canonical_actions_are_disclosed_with_authority() {
        let value = descriptor();
        let actions = value["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 2);
        let request_evidence = &actions[0];
        assert_eq!(
            request_evidence["action_ref"],
            REQUEST_MORE_EVIDENCE_ACTION_REF
        );
        assert_eq!(request_evidence["availability"], "disclosed");
        assert_eq!(request_evidence["subject_kinds"], json!(["candidate"]));
        assert!(request_evidence["authority"]["requires"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == REQUEST_MORE_EVIDENCE_CAPABILITY_REF));
        assert_eq!(actions[1]["action_ref"], "factory.development.observe");
        for action in actions {
            for surface in ["ui", "agent", "headless"] {
                assert_eq!(action["exposure"][surface], true);
            }
        }
    }

    #[test]
    fn disclosure_does_not_invent_the_actuation_seam() {
        let output = system_command(true).unwrap();
        assert!(!output.contains("actuation"));
        assert!(!output.contains("Actuation"));
    }
}
