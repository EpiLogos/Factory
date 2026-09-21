//! O:I Configuration Plane owner lane (#299 C3D): Factory's configuration
//! contribution and its owner-native mutation transport.
//!
//! Factory contributes only the developmental/binding configuration it
//! genuinely owns and can mutate through its own native services. The one
//! honest setting today is the Factory↔Central project binding held in the
//! project's developmental state document (`factory development
//! central-project-link`): Factory owns the join, verifies it against the
//! Central project source document with the exact check its native admission
//! performs, and — with `remove_central_project_link` — owns its unbinding.
//! Model/provider, agency, session and Workcell semantics belong to their own
//! products and are deliberately absent here, as is the unresolved
//! Factory↔Actuation relation (#299 §9).
//!
//! Documents follow the frozen C0 contract
//! (`docs/cradle/09-CONFIGURATION-PLANE.md`):
//!
//! - `factory config-contribution --json` emits a bare
//!   `oi.configuration-contribution/v1` document (no envelope);
//! - `factory config validate|plan|apply|reset --json` speak
//!   `oi.config-validation/v1`, `oi.config-plan/v1` (owner-minted `plan_id`
//!   and `plan_digest`) and `oi.config-receipt/v1` (owner-minted
//!   `receipt_id`, `native_ref` into Factory's own history);
//! - failures exit non-zero with an `oi.config-error/v1` document on stdout;
//! - the idempotency key `(owner_ref, changeset_id, setting_ref, scope,
//!   plan_digest)` is enforced owner-side: replay returns outcome `no_op`
//!   with `original_receipt_id` naming the executed receipt.
//!
//! Owner-side records (minted plans, emitted receipts) live in a journal
//! beside the addressed developmental state document: the state document is
//! Factory's record of the binding itself; the journal is Factory's receipt
//! history, which is what makes idempotent replay answerable.

use crate::developmental_read::{
    verify_central_project_link, FactoryCentralProjectLinkRequest, FactoryDevelopmentalFileProvider,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;

pub const CONFIGURATION_CONTRIBUTION_SCHEMA: &str = "oi.configuration-contribution/v1";
pub const CONFIGURATION_CONTRACT_REVISION: &str = "configuration-plane/contribution.1";
pub const CONFIG_VALIDATION_SCHEMA: &str = "oi.config-validation/v1";
pub const CONFIG_PLAN_SCHEMA: &str = "oi.config-plan/v1";
pub const CONFIG_RECEIPT_SCHEMA: &str = "oi.config-receipt/v1";
pub const CONFIG_ERROR_SCHEMA: &str = "oi.config-error/v1";
pub const CONFIG_JOURNAL_SCHEMA: &str = "factory.config-journal/v1";

/// The frozen product_id of the Wave-5 mount (C0 §3) — never the CLI name.
pub const CONFIG_OWNER_REF: &str = "software-factory";

/// The one setting Factory contributes today: the Factory↔Central project
/// binding held in the project's own developmental state document.
pub const CENTRAL_PROJECT_SETTING_REF: &str = "software-factory:binding:central-project";

const CENTRAL_PROJECT_NATIVE_REF: &str = "factory:developmental-state:central-project-links";
const CENTRAL_PROJECT_EFFECT_SUMMARY: &str = "Future Factory central-project-link reads resolve through the new binding; the developmental state document gains the link.";
const SCOPE_NAMESPACE_NOTE: &str = "The project scope_ref is the path of the Factory developmental state document that owns the binding — Factory's native address for one project's mutable state.";

/// Frozen scope-kind registry (C0 §5 seed kinds). New kinds are a contract
/// minor revision, not a local addition.
const SCOPE_KINDS: &[&str] = &[
    "world",
    "ground",
    "project",
    "machine",
    "workcell",
    "agency",
    "agent",
    "session-space",
    "agent-session",
    "provider",
    "connector-relation",
    "invocation",
];

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// True for the two command heads this module owns.
pub fn is_config_command(head: Option<&str>) -> bool {
    matches!(head, Some("config") | Some("config-contribution"))
}

/// Process entry for the configuration commands. The configuration contract
/// puts failure documents on stdout while still exiting non-zero, so these
/// commands report through here rather than through the plain CLI error path.
pub fn config_main(args: &[String]) -> ExitCode {
    let json = args.iter().any(|argument| argument == "--json");
    let stripped: Vec<String> = args
        .iter()
        .filter(|argument| argument.as_str() != "--json")
        .cloned()
        .collect();
    match execute_config(&stripped, None, json) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(document) => {
            println!("{document}");
            ExitCode::from(2)
        }
    }
}

/// Execute `config-contribution` / `config <verb>`. `json` is supplied by the
/// caller because the shared CLI dispatcher strips `--json` before dispatch.
/// The `Err` payload is the complete serialized `oi.config-error/v1` document.
pub fn execute_config(args: &[String], stdin: Option<&str>, json: bool) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("config-contribution") => config_contribution_command(json),
        Some("config") => config_verb_command(&args[1..], json, stdin),
        _ => Err(error_document(
            "internal",
            "configuration commands are `config-contribution` and `config <verb>`",
            None,
            None,
        )),
    }
}

fn config_contribution_command(json_out: bool) -> Result<String, String> {
    let document = build_contribution();
    if json_out {
        return serde_json::to_string_pretty(&document).map_err(internal_error);
    }
    let availability = document["availability"]["state"]
        .as_str()
        .unwrap_or("unknown");
    let setting_count: usize = document["sections"]
        .as_array()
        .map(|sections| {
            sections
                .iter()
                .filter_map(|section| section["settings"].as_array())
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0);
    Ok(format!(
        "software-factory configuration contribution ({CONFIGURATION_CONTRIBUTION_SCHEMA}):\n\
         sections: {}\n\
         settings: {setting_count}\n\
         mutation transport: cli/v1 (validate, plan, apply, reset)\n\
         availability: {availability}",
        document["sections"].as_array().map_or(0, Vec::len)
    ))
}

fn config_verb_command(args: &[String], json: bool, stdin: Option<&str>) -> Result<String, String> {
    let verb = args.first().map(String::as_str).ok_or_else(|| {
        error_document(
            "validation_failed",
            "missing config verb: expected validate, plan, apply or reset",
            None,
            None,
        )
    })?;
    let parsed = parse_verb_args(&args[1..])?;
    match verb {
        "validate" => validate_command(&parsed, json, stdin),
        "plan" => plan_command(&parsed, json, stdin),
        "apply" => apply_command(&parsed, json, stdin),
        "reset" => reset_command(&parsed, json),
        other => Err(error_document(
            "validation_failed",
            &format!("unknown config verb `{other}`; expected validate, plan, apply or reset"),
            None,
            None,
        )),
    }
}

// ---------------------------------------------------------------------------
// Contribution document
// ---------------------------------------------------------------------------

fn build_contribution() -> Value {
    let observed = now_ms();

    let telemetry_section = json!({
        "id": "telemetry",
        "title": "Telemetry surfaces",
        "settings": [
            {
                "setting_ref": TELEMETRY_SEARCH_LIMIT_SETTING,
                "section_ref": "telemetry",
                "title": "Search result limit default",
                "description": "Default --limit for `factory telemetry search` when the flag is absent; delegated AIKit queries stay within this bound. Override per call with --limit.",
                "value_schema": { "type": "number", "minimum": 1, "maximum": 100 },
                "allowed_scopes": [{ "scope_kind": "project", "scope_ref": null }],
                "writable": true,
                "profileable": false,
                "sensitive": false,
                "default_semantics": "constant",
                "effect": {
                    "kind": "value-change",
                    "summary": "Takes effect on the next telemetry command; no restart, no reindex",
                    "ref": null
                },
                "operations": { "validate": true, "plan": true, "apply": true, "reset": true },
                "native_ref": "software-factory:telemetry-config"
            },
            {
                "setting_ref": TELEMETRY_SEARCH_TIMEOUT_SETTING,
                "section_ref": "telemetry",
                "title": "Delegated search subprocess budget (seconds)",
                "description": "Wall-clock budget for the delegated AIKit knowledge search subprocess. A timed-out search is disclosed as provider-unavailable, never retried silently.",
                "value_schema": { "type": "number", "minimum": 5, "maximum": 600 },
                "allowed_scopes": [{ "scope_kind": "project", "scope_ref": null }],
                "writable": true,
                "profileable": false,
                "sensitive": false,
                "default_semantics": "constant",
                "effect": {
                    "kind": "value-change",
                    "summary": "Takes effect on the next telemetry command; no restart, no reindex",
                    "ref": null
                },
                "operations": { "validate": true, "plan": true, "apply": true, "reset": true },
                "native_ref": "software-factory:telemetry-config"
            },
            {
                "setting_ref": TELEMETRY_WATCH_INTERVAL_SETTING,
                "section_ref": "telemetry",
                "title": "Watch poll interval (seconds)",
                "description": "Default poll interval for `factory telemetry watch`; the stream stays bounded by --max-events and --duration regardless.",
                "value_schema": { "type": "number", "minimum": 0.5, "maximum": 60 },
                "allowed_scopes": [{ "scope_kind": "project", "scope_ref": null }],
                "writable": true,
                "profileable": false,
                "sensitive": false,
                "default_semantics": "constant",
                "effect": {
                    "kind": "value-change",
                    "summary": "Takes effect on the next telemetry command; no restart, no reindex",
                    "ref": null
                },
                "operations": { "validate": true, "plan": true, "apply": true, "reset": true },
                "native_ref": "software-factory:telemetry-config"
            }
        ]
    });
    let sections = vec![
        json!({
            "id": "binding",
            "title": "Project bindings",
            "settings": [
                {
                    "setting_ref": CENTRAL_PROJECT_SETTING_REF,
                    "section_ref": "binding",
                    "title": "Central project binding",
                    "description": format!(
                        "Which Central project this Factory project is bound to, verified against the Central project source document by the same native check the admission performs. {}",
                        SCOPE_NAMESPACE_NOTE
                    ),
                    "value_schema": {
                        "type": "table",
                        "columns": [
                            { "name": "central_project_ref", "type": "scalar" },
                            { "name": "source_path", "type": "path" }
                        ]
                    },
                    "allowed_scopes": [{ "scope_kind": "project", "scope_ref": null }],
                    "writable": true,
                    "profileable": false,
                    "sensitive": false,
                    "default_semantics": "none",
                    "effect": {
                        "kind": "value-change",
                        "summary": CENTRAL_PROJECT_EFFECT_SUMMARY,
                        "ref": null
                    },
                    "operations": { "validate": true, "plan": true, "apply": true, "reset": true },
                    "native_ref": CENTRAL_PROJECT_NATIVE_REF
                }
            ]
        }),
        telemetry_section,
    ];

    let body = json!({
        "schema": CONFIGURATION_CONTRIBUTION_SCHEMA,
        "contract_revision": CONFIGURATION_CONTRACT_REVISION,
        "owner": {
            "owner_ref": CONFIG_OWNER_REF,
            "owner_kind": "product",
            "owner_version": env!("CARGO_PKG_VERSION"),
            "contribution_command": ["factory", "config-contribution", "--json"],
            "disclosed_at_unix_ms": observed,
            "reading_digest": null,
            "reading_digest_covers": "07 §4.5 convention"
        },
        "about": "Project bindings: the Factory-owned binding between a Factory project and its Central project source, addressable through Factory's native developmental-state admission.",
        "sections": sections,
        "operations": {
            "transport": "cli/v1",
            "validate": { "availability": "disclosed", "reason": null },
            "plan": { "availability": "disclosed", "reason": null },
            "apply": { "availability": "disclosed", "reason": null },
            "reset": { "availability": "disclosed", "reason": null }
        },
        "availability": { "state": "available", "reason": null },
        "degradations": [],
        "obligations": []
    });

    // 07 §4.5 convention: the digest covers the document with every
    // `*_unix_ms` field zeroed and `reading_digest` null, so two disclosures
    // of an unchanged surface hash the same.
    let mut canonical = body.clone();
    zero_unix_ms(&mut canonical);
    if let Some(owner) = canonical.get_mut("owner").and_then(Value::as_object_mut) {
        owner.insert("reading_digest".into(), Value::Null);
    }
    let digest =
        sha256_hex(&serde_json::to_string(&canonical).expect("canonical contribution serialises"));
    let mut document = body;
    document["owner"]["reading_digest"] = json!(digest);
    document
}

// ---------------------------------------------------------------------------
// Structured documents
// ---------------------------------------------------------------------------

/// Field names are the frozen snake_case wire names of the C0 schemas.
/// Unknown fields are tolerated on every read (C0 §15): the owner emits the
/// frozen fields and never rejects a caller document for carrying more.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigScope {
    pub scope_kind: String,
    /// Always serialised (`null` for singular kinds) so every emitted document
    /// carries the `scope_ref` member the frozen schemas require. Reads accept
    /// an absent member (C0 §15 unknown-field/additive tolerance).
    #[serde(default)]
    pub scope_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigViolation {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedEffect {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValidation {
    pub schema: String,
    pub setting_ref: String,
    pub scope: ConfigScope,
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<ConfigViolation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_effect: Option<ExpectedEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanChange {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPlan {
    pub schema: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub setting_ref: String,
    pub scope: ConfigScope,
    pub changes: Vec<PlanChange>,
    pub expected_effect: ExpectedEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigReceipt {
    pub schema: String,
    pub receipt_id: String,
    pub owner_ref: String,
    pub changeset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    pub setting_ref: String,
    pub scope: ConfigScope,
    pub operation: String,
    pub outcome: String,
    pub applied_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_effect: Option<ExpectedEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ReceiptError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

/// Factory-local (camelCase) journal: the plans this owner minted and the
/// receipts it has emitted for one developmental state document. The state
/// document itself remains the record of the binding; the journal is the
/// receipt history that makes idempotent replay answerable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigJournal {
    pub schema: String,
    #[serde(default)]
    pub plans: Vec<StoredPlan>,
    #[serde(default)]
    pub receipts: Vec<ConfigReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPlan {
    pub plan_id: String,
    pub plan_digest: String,
    pub setting_ref: String,
    pub scope: ConfigScope,
    pub value: Value,
    pub minted_at_unix_ms: i64,
}

impl ConfigJournal {
    fn new() -> Self {
        Self {
            schema: CONFIG_JOURNAL_SCHEMA.into(),
            plans: Vec::new(),
            receipts: Vec::new(),
        }
    }

    /// The receipt that already executed exactly this idempotency key —
    /// `(owner_ref, changeset_id, setting_ref, scope, plan_digest)`, the frozen
    /// key of C0 §9 (the owner_ref is constant in this journal). Only an exact
    /// key replays as `no_op`: a new changeset is a new key and executes
    /// again, and the native admission itself refuses to double-apply an
    /// identical binding. A `failed` receipt never satisfies the key — a
    /// failure was not an execution, so a retry under the same changeset
    /// re-attempts.
    fn executed_receipt(
        &self,
        changeset_id: &str,
        setting_ref: &str,
        scope: &ConfigScope,
        plan_digest: Option<&str>,
    ) -> Option<&ConfigReceipt> {
        self.receipts.iter().find(|receipt| {
            receipt.changeset_id == changeset_id
                && receipt.setting_ref == setting_ref
                && receipt.scope == *scope
                && receipt.plan_digest.as_deref() == plan_digest
                && matches!(receipt.outcome.as_str(), "applied" | "no_op")
        })
    }

    /// The receipt that originally applied this exact change (same setting,
    /// scope and plan digest) with outcome `applied`, for naming on a
    /// truthfully-`no_op` answer whose key was not a replay — e.g. the binding
    /// was already applied natively before this plane saw it.
    fn original_apply_receipt(
        &self,
        setting_ref: &str,
        scope: &ConfigScope,
        plan_digest: &str,
    ) -> Option<String> {
        self.receipts
            .iter()
            .find(|receipt| {
                receipt.setting_ref == setting_ref
                    && receipt.scope == *scope
                    && receipt.plan_digest.as_deref() == Some(plan_digest)
                    && receipt.outcome == "applied"
            })
            .map(|receipt| receipt.receipt_id.clone())
    }
}

// ---------------------------------------------------------------------------
// Verb argument parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct VerbArgs {
    setting: Option<String>,
    scope: Option<String>,
    value: Option<String>,
    value_file: Option<String>,
    plan_file: Option<String>,
    changeset: Option<String>,
}

fn parse_verb_args(args: &[String]) -> Result<VerbArgs, String> {
    let mut parsed = VerbArgs::default();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let field = match flag {
            "--setting" => &mut parsed.setting,
            "--scope" => &mut parsed.scope,
            "--value" => &mut parsed.value,
            "--value-file" => &mut parsed.value_file,
            "--plan-file" => &mut parsed.plan_file,
            "--changeset" => &mut parsed.changeset,
            other => {
                return Err(error_document(
                    "validation_failed",
                    &format!("unknown config argument `{other}`"),
                    None,
                    None,
                ))
            }
        };
        let value = args.get(index + 1).ok_or_else(|| {
            error_document(
                "validation_failed",
                &format!("config argument `{flag}` requires a value"),
                None,
                None,
            )
        })?;
        *field = Some(value.clone());
        index += 2;
    }
    Ok(parsed)
}

fn read_input(path: &str, stdin: Option<&str>) -> Result<String, String> {
    if path != "-" {
        return fs::read_to_string(path).map_err(|error| {
            error_document(
                "validation_failed",
                &format!("cannot read caller-supplied input `{path}`: {error}"),
                None,
                None,
            )
        });
    }
    if let Some(value) = stdin {
        return Ok(value.to_owned());
    }
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| internal_io_document("read stdin", &error))?;
    Ok(input)
}

// ---------------------------------------------------------------------------
// Setting registry and scope grammar
// ---------------------------------------------------------------------------

/// One contributed setting's operability contract, mirroring the fields the
/// contribution document discloses for it. The verbs consult this registry so
/// the disclosed contract and the transport cannot drift apart.
#[derive(Debug)]
struct SettingEntry {
    setting_ref: &'static str,
    writable: bool,
    operations: VerbOps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VerbOps {
    validate: bool,
    plan: bool,
    apply: bool,
    reset: bool,
}

const CENTRAL_PROJECT_ENTRY: SettingEntry = SettingEntry {
    setting_ref: CENTRAL_PROJECT_SETTING_REF,
    writable: true,
    operations: VerbOps {
        validate: true,
        plan: true,
        apply: true,
        reset: true,
    },
};

pub const TELEMETRY_SEARCH_LIMIT_SETTING: &str = "software-factory:telemetry:search-limit-default";
pub const TELEMETRY_SEARCH_TIMEOUT_SETTING: &str =
    "software-factory:telemetry:search-timeout-seconds";
pub const TELEMETRY_WATCH_INTERVAL_SETTING: &str =
    "software-factory:telemetry:watch-interval-seconds";

/// Effective telemetry settings live in a sidecar beside the developmental
/// state; the configuration plane writes them, the telemetry CLI reads them.
pub fn telemetry_settings_path(state_path: &std::path::Path) -> std::path::PathBuf {
    let mut name = state_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(".telemetry-settings.json");
    state_path.with_file_name(name)
}

/// Sidecar writes are atomic. Every reader of this sidecar recovers with
/// `unwrap_or_default()`, so a torn write would not error anywhere — the
/// applied settings would quietly fall back to built-in defaults. The body
/// therefore lands in a `.<name>.tmp` sibling first and a same-directory
/// rename (atomic within one filesystem) puts it in place, the write
/// discipline the configuration journal in this file already keeps.
fn write_telemetry_settings_atomic(
    sidecar: &std::path::Path,
    effective: &serde_json::Map<String, Value>,
) -> std::io::Result<()> {
    let file_name = sidecar
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = sidecar.with_file_name(format!(".{file_name}.tmp"));
    std::fs::write(&tmp, serde_json::to_string_pretty(effective)?)?;
    std::fs::rename(&tmp, sidecar)
}

/// The effective value of one telemetry setting, if the plane applied one.
pub fn read_telemetry_effective(state_path: &std::path::Path, setting_ref: &str) -> Option<String> {
    let text = std::fs::read_to_string(telemetry_settings_path(state_path)).ok()?;
    let document: serde_json::Value = serde_json::from_str(&text).ok()?;
    document[setting_ref]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            document[setting_ref].as_f64().map(|n| {
                if n.fract() == 0.0 {
                    format!("{}", n as i64)
                } else {
                    format!("{n}")
                }
            })
        })
}

/// The whole effective telemetry sidecar, for the CLI defaults.
pub fn read_telemetry_effective_all(
    state_path: &std::path::Path,
) -> std::collections::BTreeMap<String, f64> {
    std::fs::read_to_string(telemetry_settings_path(state_path))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn telemetry_setting_bounds(setting_ref: &str) -> Option<(f64, f64)> {
    match setting_ref {
        TELEMETRY_SEARCH_LIMIT_SETTING => Some((1.0, 100.0)),
        TELEMETRY_SEARCH_TIMEOUT_SETTING => Some((5.0, 600.0)),
        TELEMETRY_WATCH_INTERVAL_SETTING => Some((0.5, 60.0)),
        _ => None,
    }
}

fn is_telemetry_setting(setting_ref: &str) -> bool {
    setting_ref.starts_with("software-factory:telemetry:")
}

const TELEMETRY_SEARCH_LIMIT_ENTRY: SettingEntry = SettingEntry {
    setting_ref: TELEMETRY_SEARCH_LIMIT_SETTING,
    writable: true,
    operations: VerbOps {
        validate: true,
        plan: true,
        apply: true,
        reset: true,
    },
};
const TELEMETRY_SEARCH_TIMEOUT_ENTRY: SettingEntry = SettingEntry {
    setting_ref: TELEMETRY_SEARCH_TIMEOUT_SETTING,
    writable: true,
    operations: VerbOps {
        validate: true,
        plan: true,
        apply: true,
        reset: true,
    },
};
const TELEMETRY_WATCH_INTERVAL_ENTRY: SettingEntry = SettingEntry {
    setting_ref: TELEMETRY_WATCH_INTERVAL_SETTING,
    writable: true,
    operations: VerbOps {
        validate: true,
        plan: true,
        apply: true,
        reset: true,
    },
};

const SETTING_REGISTRY: &[SettingEntry] = &[
    CENTRAL_PROJECT_ENTRY,
    TELEMETRY_SEARCH_LIMIT_ENTRY,
    TELEMETRY_SEARCH_TIMEOUT_ENTRY,
    TELEMETRY_WATCH_INTERVAL_ENTRY,
];

/// The four transport verbs (C0 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Validate,
    Plan,
    Apply,
    Reset,
}

impl Verb {
    fn disclosed_flag(self, operations: VerbOps) -> bool {
        match self {
            Verb::Validate => operations.validate,
            Verb::Plan => operations.plan,
            Verb::Apply => operations.apply,
            Verb::Reset => operations.reset,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Verb::Validate => "validate",
            Verb::Plan => "plan",
            Verb::Apply => "apply",
            Verb::Reset => "reset",
        }
    }
}

/// The structured refusal law for addressing a setting (C0 §§2.2, 5): an
/// unknown setting is `unsupported_setting`; a known setting that is not
/// writable, or whose disclosed `operations` do not include this verb, refuses
/// mutation with the same structured error — never a silent fallback and never
/// an absorbed foreign setting.
fn refusal_reason(entry_writable: bool, operations: VerbOps, verb: Verb) -> Option<String> {
    if !entry_writable && verb != Verb::Validate {
        return Some(format!(
            "setting is disclosed as read-only (`writable: false`); `{}` mutation is refused",
            verb.name()
        ));
    }
    if !verb.disclosed_flag(operations) {
        return Some(format!(
            "contribution does not disclose `{}` for this setting",
            verb.name()
        ));
    }
    None
}

fn registry_entry(setting_ref: &str) -> Result<&'static SettingEntry, String> {
    SETTING_REGISTRY
        .iter()
        .find(|entry| entry.setting_ref == setting_ref)
        .ok_or_else(|| {
            error_document(
                "unsupported_setting",
                &format!(
                    "unknown setting `{setting_ref}`; this owner contributes `{CENTRAL_PROJECT_SETTING_REF}` and the `telemetry.*` settings, and never another product's settings"
                ),
                Some(setting_ref),
                None,
            )
        })
}

fn ensure_operable(setting_ref: &str, verb: Verb) -> Result<&'static SettingEntry, String> {
    let entry = registry_entry(setting_ref)?;
    if let Some(reason) = refusal_reason(entry.writable, entry.operations, verb) {
        return Err(error_document(
            "unsupported_setting",
            &format!(
                "setting `{setting_ref}` refuses `{}`: {reason}",
                verb.name()
            ),
            Some(setting_ref),
            None,
        ));
    }
    Ok(entry)
}

/// `"<scope_kind>:<scope_ref>"` — the compact CLI form (C0 §5); the ref is
/// omitted only for singular kinds.
fn parse_compact_scope(compact: &str) -> Result<ConfigScope, String> {
    let (scope_kind, scope_ref) = match compact.split_once(':') {
        Some((kind, reference)) => (kind, Some(reference.to_owned())),
        None => (compact, None),
    };
    if !SCOPE_KINDS.contains(&scope_kind) {
        return Err(error_document(
            "unknown_scope_kind",
            &format!("unknown scope kind `{scope_kind}`"),
            None,
            Some(scope_kind),
        ));
    }
    Ok(ConfigScope {
        scope_kind: scope_kind.to_owned(),
        scope_ref,
    })
}

/// The contributed setting allows exactly the `project` scope, and a project
/// address here is the developmental state document path (Factory's native
/// project-state address) — a non-singular kind, so a ref is required.
fn resolve_setting_scope(scope: &ConfigScope) -> Result<PathBuf, String> {
    if scope.scope_kind != "project" {
        return Err(error_document(
            "unsupported_scope",
            &format!(
                "setting `{CENTRAL_PROJECT_SETTING_REF}` is addressable only at the `project` scope, not `{}`",
                scope.scope_kind
            ),
            Some(CENTRAL_PROJECT_SETTING_REF),
            Some(&scope.scope_kind),
        ));
    }
    let reference = scope
        .scope_ref
        .as_deref()
        .filter(|reference| !reference.is_empty())
        .ok_or_else(|| {
            error_document(
                "unsupported_scope",
                &format!("the `project` scope requires a scope_ref; {SCOPE_NAMESPACE_NOTE}"),
                Some(CENTRAL_PROJECT_SETTING_REF),
                Some("project"),
            )
        })?;
    Ok(PathBuf::from(reference))
}

fn open_state(scope: &ConfigScope) -> Result<FactoryDevelopmentalFileProvider, String> {
    let path = resolve_setting_scope(scope)?;
    FactoryDevelopmentalFileProvider::open(path).map_err(|error| {
        error_document(
            "unsupported_scope",
            &format!("no Factory developmental state document for this project scope: {error}"),
            Some(CENTRAL_PROJECT_SETTING_REF),
            Some(&scope.scope_kind),
        )
    })
}

// ---------------------------------------------------------------------------
// Value contract: one binding row
// ---------------------------------------------------------------------------

/// The value of the central-project setting: a table (array of row objects)
/// with columns `central_project_ref` and `source_path`, exactly one row —
/// Factory binds a project to exactly one Central project.
fn central_project_value(value: &Value) -> Result<(String, String), String> {
    let violation = |code: &str, message: String| {
        error_document(
            "invalid_value",
            &format!("{code}: {message}"),
            Some(CENTRAL_PROJECT_SETTING_REF),
            None,
        )
    };
    let rows = value.as_array().ok_or_else(|| {
        violation(
            "value-shape",
            "the central-project value is a table: an array of row objects with columns `central_project_ref` and `source_path`".to_owned(),
        )
    })?;
    if rows.len() != 1 {
        return Err(violation(
            "row-count",
            format!(
                "Factory binds exactly one Central project per project state; the value carries {} rows",
                rows.len()
            ),
        ));
    }
    let row = rows[0]
        .as_object()
        .ok_or_else(|| violation("row-shape", "each table row is an object".to_owned()))?;
    let column = |name: &str| -> Result<String, String> {
        row.get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|column| !column.is_empty())
            .ok_or_else(|| {
                violation(
                    "missing-column",
                    format!("row column `{name}` is required and must be a non-empty string"),
                )
            })
    };
    let central_project_ref = column("central_project_ref")?;
    let source_path = column("source_path")?;
    Ok((central_project_ref, source_path))
}

/// The owner's native validation for a requested binding value against the
/// addressed state: `verify_central_project_link`, the exact check the native
/// admission performs, plus the one-binding-per-project conflict check the
/// admission performs (an existing link that differs in ref, source path or
/// source revision — the native comparison is whole-link — is a violation, so
/// `validate` truthfully previews what `apply` will do).
fn evaluate_value(
    setting_ref: &str,
    state: &FactoryDevelopmentalFileProvider,
    value: &Value,
) -> Result<(bool, Vec<ConfigViolation>), String> {
    if is_telemetry_setting(setting_ref) {
        return evaluate_telemetry_value(setting_ref, value);
    }
    let (central_project_ref, source_path) = central_project_value(value)?;
    let mut violations = Vec::new();

    let request = FactoryCentralProjectLinkRequest {
        contract: crate::developmental_read::FACTORY_CENTRAL_PROJECT_LINK_REQUEST.into(),
        factory_project_ref: state.project_ref().clone(),
        central_project_ref: central_project_ref.clone(),
        source_path: source_path.clone(),
    };
    let verified = match verify_central_project_link(&request) {
        Ok(verified) => verified,
        Err(error) => {
            violations.push(ConfigViolation {
                code: "central-project-source".into(),
                message: link_error_message(&error),
                path: None,
            });
            return Ok((false, violations));
        }
    };

    if let Some(existing) = state.central_project_link() {
        if *existing != verified {
            violations.push(ConfigViolation {
                code: "already-bound".into(),
                message: format!(
                    "the project is already bound to Central project `{}` (source `{}`); unbind it (`config reset`) before binding a different Central project or a revised source",
                    existing.central_project_ref, existing.source_path
                ),
                path: None,
            });
        }
    }
    Ok((violations.is_empty(), violations))
}

/// Telemetry knobs are bounded numbers; their validity needs no state.
fn evaluate_telemetry_value(
    setting_ref: &str,
    value: &Value,
) -> Result<(bool, Vec<ConfigViolation>), String> {
    let Some((low, high)) = telemetry_setting_bounds(setting_ref) else {
        return Err(error_document(
            "unsupported_setting",
            &format!("unknown telemetry setting `{setting_ref}`"),
            Some(setting_ref),
            None,
        ));
    };
    let mut violations = Vec::new();
    let number = value.as_f64();
    match number {
        None => violations.push(ConfigViolation {
            code: "invalid_value".into(),
            message: format!("`{setting_ref}` takes a number in [{low}, {high}]"),
            path: None,
        }),
        Some(n) if !(low..=high).contains(&n) => violations.push(ConfigViolation {
            code: "invalid_value".into(),
            message: format!("`{setting_ref}` must be within [{low}, {high}], got {n}"),
            path: None,
        }),
        _ => {}
    }
    Ok((violations.is_empty(), violations))
}

fn link_error_message(error: &crate::developmental_read::FactoryDevelopmentalReadError) -> String {
    match error {
        crate::developmental_read::FactoryDevelopmentalReadError::InvalidCentralProjectLink(
            detail,
        ) => detail.clone(),
        other => other.to_string(),
    }
}

fn violations_detail(violations: &[ConfigViolation]) -> String {
    violations
        .iter()
        .map(|violation| format!("{}: {}", violation.code, violation.message))
        .collect::<Vec<_>>()
        .join("; ")
}

// ---------------------------------------------------------------------------
// validate / plan / apply / reset
// ---------------------------------------------------------------------------

fn requested_value(
    parsed: &VerbArgs,
    setting_ref: &str,
    stdin: Option<&str>,
) -> Result<Value, String> {
    let raw = match (&parsed.value, &parsed.value_file) {
        (Some(value), _) => value.clone(),
        (None, Some(path)) => read_input(path, stdin)?,
        _ => {
            return Err(error_document(
                "validation_failed",
                "one of --value or --value-file is required",
                None,
                None,
            ))
        }
    };
    serde_json::from_str(raw.trim()).map_err(|error| {
        error_document(
            "invalid_value",
            &format!("--value is not valid JSON: {error}"),
            Some(setting_ref),
            None,
        )
    })
}

fn validate_command(parsed: &VerbArgs, json: bool, stdin: Option<&str>) -> Result<String, String> {
    let setting_ref = parsed
        .setting
        .as_deref()
        .ok_or_else(|| error_document("validation_failed", "missing --setting", None, None))?;
    ensure_operable(setting_ref, Verb::Validate)?;
    let scope = parse_compact_scope(
        parsed
            .scope
            .as_deref()
            .ok_or_else(|| error_document("validation_failed", "missing --scope", None, None))?,
    )?;
    let value = requested_value(parsed, setting_ref, stdin)?;

    let state = open_state(&scope)?;
    let (valid, violations) = evaluate_value(setting_ref, &state, &value)?;

    let validation = ConfigValidation {
        schema: CONFIG_VALIDATION_SCHEMA.into(),
        setting_ref: setting_ref.to_owned(),
        scope,
        valid,
        violations,
        expected_effect: Some(central_project_effect()),
    };
    if json {
        return serde_json::to_string_pretty(&validation).map_err(internal_error);
    }
    if valid {
        return Ok(format!(
            "{setting_ref} is valid — {CENTRAL_PROJECT_EFFECT_SUMMARY}"
        ));
    }
    Ok(violations_detail(&validation.violations))
}

fn plan_command(parsed: &VerbArgs, json: bool, stdin: Option<&str>) -> Result<String, String> {
    let setting_ref = parsed
        .setting
        .as_deref()
        .ok_or_else(|| error_document("validation_failed", "missing --setting", None, None))?;
    ensure_operable(setting_ref, Verb::Plan)?;
    let scope = parse_compact_scope(
        parsed
            .scope
            .as_deref()
            .ok_or_else(|| error_document("validation_failed", "missing --scope", None, None))?,
    )?;
    let value = requested_value(parsed, setting_ref, stdin)?;

    let state = open_state(&scope)?;
    let (valid, violations) = evaluate_value(setting_ref, &state, &value)?;
    if !valid {
        let code = if violations
            .iter()
            .any(|violation| violation.code == "already-bound")
        {
            "validation_failed"
        } else {
            "invalid_value"
        };
        return Err(error_document(
            code,
            &violations_detail(&violations),
            Some(setting_ref),
            Some(&scope.scope_kind),
        ));
    }
    if is_telemetry_setting(setting_ref) {
        let state_path = resolve_setting_scope(&scope)?;
        let current = read_telemetry_effective(&state_path, setting_ref);
        let plan = ConfigPlan {
            schema: CONFIG_PLAN_SCHEMA.into(),
            plan_id: format!("factory-plan-{}", ulid::Ulid::new()),
            plan_digest: String::new(),
            setting_ref: setting_ref.to_owned(),
            scope: scope.clone(),
            changes: vec![PlanChange {
                summary: format!(
                    "Set `{setting_ref}` to {value} for the telemetry CLI surfaces reading this state's sidecar.",
                ),
                native_ref: Some(format!(
                    "software-factory:telemetry-config:{}",
                    telemetry_settings_path(&state_path).display()
                )),
                before_ref: current.clone(),
                after_ref: Some(value.to_string()),
            }],
            expected_effect: ExpectedEffect {
                kind: "value-change".into(),
                summary: Some(format!(
                    "`{setting_ref}` takes effect on the next telemetry command; no restart, no reindex"
                )),
                ..central_project_effect()
            },
            expires_at_unix_ms: None,
            explain_ref: Some("factory telemetry status".into()),
        };
        let mut plan = plan;
        plan.plan_digest = plan_digest(&plan);
        let stored_scope = scope.clone();
        let stored_value = value.clone();
        let stored_setting = setting_ref.to_owned();
        journal_write(&state_path, |journal| {
            journal.plans.push(StoredPlan {
                plan_id: plan.plan_id.clone(),
                plan_digest: plan.plan_digest.clone(),
                setting_ref: stored_setting,
                scope: stored_scope,
                value: stored_value,
                minted_at_unix_ms: now_ms(),
            });
            Ok(())
        })?;
        if json {
            return serde_json::to_string_pretty(&plan).map_err(internal_error);
        }
        return Ok(format!(
            "plan {} (digest {}) — {}",
            plan.plan_id, plan.plan_digest, plan.changes[0].summary
        ));
    }
    let (central_project_ref, _source_path) = central_project_value(&value)?;
    let state_path = resolve_setting_scope(&scope)?;
    let before_ref = state
        .central_project_link()
        .map(|link| link.central_project_ref.clone());

    let mut plan = ConfigPlan {
        schema: CONFIG_PLAN_SCHEMA.into(),
        plan_id: format!("factory-plan-{}", ulid::Ulid::new()),
        plan_digest: String::new(),
        setting_ref: setting_ref.to_owned(),
        scope: scope.clone(),
        changes: vec![PlanChange {
            summary: format!(
                "Bind Factory project {} to Central project `{central_project_ref}` in the project's developmental state.",
                state.project_ref()
            ),
            native_ref: Some(format!(
                "{CENTRAL_PROJECT_NATIVE_REF}:{}",
                state_path.display()
            )),
            before_ref,
            after_ref: Some(central_project_ref),
        }],
        expected_effect: central_project_effect(),
        expires_at_unix_ms: None,
        explain_ref: Some("factory development central-project-link-read".into()),
    };
    plan.plan_digest = plan_digest(&plan);

    let stored_scope = scope.clone();
    let stored_value = value.clone();
    let stored_setting = setting_ref.to_owned();
    journal_write(&state_path, |journal| {
        journal.plans.push(StoredPlan {
            plan_id: plan.plan_id.clone(),
            plan_digest: plan.plan_digest.clone(),
            setting_ref: stored_setting,
            scope: stored_scope,
            value: stored_value,
            minted_at_unix_ms: now_ms(),
        });
        Ok(())
    })?;

    if json {
        return serde_json::to_string_pretty(&plan).map_err(internal_error);
    }
    Ok(format!(
        "plan {} (digest {}) — {}",
        plan.plan_id, plan.plan_digest, plan.changes[0].summary
    ))
}

fn apply_command(parsed: &VerbArgs, json: bool, stdin: Option<&str>) -> Result<String, String> {
    let plan_file = parsed
        .plan_file
        .as_deref()
        .ok_or_else(|| error_document("validation_failed", "missing --plan-file", None, None))?;
    let raw_plan = read_input(plan_file, stdin)?;
    let requested: Value = serde_json::from_str(raw_plan.trim()).map_err(|error| {
        error_document(
            "unsupported_schema",
            &format!("--plan-file is not valid JSON: {error}"),
            None,
            None,
        )
    })?;
    if requested["schema"] != json!(CONFIG_PLAN_SCHEMA) {
        return Err(error_document(
            "unsupported_schema",
            &format!(
                "plan schema `{}` is not `{CONFIG_PLAN_SCHEMA}`",
                requested["schema"].as_str().unwrap_or("<absent>")
            ),
            None,
            None,
        ));
    }
    let plan: ConfigPlan = serde_json::from_value(requested).map_err(|error| {
        error_document(
            "unsupported_schema",
            &format!("plan document does not satisfy `{CONFIG_PLAN_SCHEMA}`: {error}"),
            None,
            None,
        )
    })?;
    ensure_operable(&plan.setting_ref, Verb::Apply)?;
    let state_path = resolve_setting_scope(&plan.scope)?;
    let actual_digest = plan_digest(&plan);
    if actual_digest != plan.plan_digest {
        return Err(error_document(
            "validation_failed",
            &format!(
                "plan digest does not match the plan body: the plan carries {}, the body computes {}",
                plan.plan_digest, actual_digest
            ),
            Some(&plan.setting_ref),
            Some(&plan.scope.scope_kind),
        ));
    }
    let changeset_id = match parsed.changeset.as_deref() {
        Some(changeset) => valid_changeset(changeset)?,
        None => format!("cs-factory-{}", ulid::Ulid::new()),
    };

    let mut state = open_state(&plan.scope)?;

    // Idempotent replay (C0 §9): the exact key — (owner_ref constant here,
    // changeset_id, setting_ref, scope, plan_digest) — already answered by an
    // executed receipt replays as `no_op` naming that receipt. A failure never
    // satisfies the key, so a retry genuinely retries.
    let journal = journal_read(&state_path)?;
    if let Some(original) = journal.executed_receipt(
        &changeset_id,
        &plan.setting_ref,
        &plan.scope,
        Some(&plan.plan_digest),
    ) {
        let replay = ConfigReceipt {
            schema: CONFIG_RECEIPT_SCHEMA.into(),
            receipt_id: format!("factory-receipt-{}", ulid::Ulid::new()),
            owner_ref: CONFIG_OWNER_REF.into(),
            changeset_id,
            plan_digest: Some(plan.plan_digest.clone()),
            setting_ref: plan.setting_ref.clone(),
            scope: plan.scope.clone(),
            operation: "apply".into(),
            outcome: "no_op".into(),
            applied_at_unix_ms: now_ms(),
            native_ref: original.native_ref.clone(),
            expected_effect: Some(central_project_effect()),
            original_receipt_id: Some(original.receipt_id.clone()),
            error: None,
        };
        return emit_receipt(&state_path, replay, json);
    }

    // The plan carries the caller-visible change; the value itself is
    // recovered from the owner's plan store, minted when `config plan` ran.
    let stored = journal
        .plans
        .iter()
        .find(|stored| stored.plan_id == plan.plan_id && stored.plan_digest == plan.plan_digest)
        .ok_or_else(|| {
            error_document(
                "validation_failed",
                &format!(
                    "plan `{}` is not known to this owner's plan store; plan at the same address first",
                    plan.plan_id
                ),
                Some(&plan.setting_ref),
                Some(&plan.scope.scope_kind),
            )
        })?
        .clone();

    // Re-validate against the current state: the state may have changed since
    // the plan was minted. The owner's validation stays authoritative.
    let (valid, violations) = evaluate_value(&plan.setting_ref, &state, &stored.value)?;
    if !valid {
        return Err(error_document(
            "validation_failed",
            &format!(
                "the planned change no longer validates: {}",
                violations_detail(&violations)
            ),
            Some(&plan.setting_ref),
            Some(&plan.scope.scope_kind),
        ));
    }
    if is_telemetry_setting(&plan.setting_ref) {
        let (valid, _) = evaluate_telemetry_value(&plan.setting_ref, &stored.value)?;
        if !valid {
            return Err(error_document(
                "validation_failed",
                "the planned telemetry value no longer validates",
                Some(&plan.setting_ref),
                Some(&plan.scope.scope_kind),
            ));
        }
        let sidecar = telemetry_settings_path(&state_path);
        let mut effective: serde_json::Map<String, Value> = std::fs::read_to_string(&sidecar)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        effective.insert(plan.setting_ref.clone(), stored.value.clone());
        write_telemetry_settings_atomic(&sidecar, &effective).map_err(|error| {
            error_document(
                "internal",
                &format!("could not write the telemetry settings sidecar: {error}"),
                Some(&plan.setting_ref),
                None,
            )
        })?;
        let receipt = ConfigReceipt {
            schema: CONFIG_RECEIPT_SCHEMA.into(),
            receipt_id: format!("factory-receipt-{}", ulid::Ulid::new()),
            owner_ref: CONFIG_OWNER_REF.into(),
            changeset_id,
            plan_digest: Some(plan.plan_digest.clone()),
            setting_ref: plan.setting_ref.clone(),
            scope: plan.scope.clone(),
            operation: "apply".into(),
            outcome: "applied".into(),
            applied_at_unix_ms: now_ms(),
            native_ref: Some(format!(
                "software-factory:telemetry-config:{}",
                sidecar.display()
            )),
            expected_effect: Some(central_project_effect()),
            original_receipt_id: None,
            error: None,
        };
        return emit_receipt(&state_path, receipt, json);
    }
    let (central_project_ref, source_path) = central_project_value(&stored.value)?;

    let request = FactoryCentralProjectLinkRequest {
        contract: crate::developmental_read::FACTORY_CENTRAL_PROJECT_LINK_REQUEST.into(),
        factory_project_ref: state.project_ref().clone(),
        central_project_ref: central_project_ref.clone(),
        source_path: source_path.clone(),
    };
    let native = state.admit_central_project_link(request).map_err(|error| {
        error_document(
            "validation_failed",
            &format!("native admission refused the binding: {error}"),
            Some(&plan.setting_ref),
            Some(&plan.scope.scope_kind),
        )
    })?;
    let (outcome, original_receipt_id) = match native.result.as_str() {
        "applied" => ("applied", None),
        // The binding already exists and is identical — applied natively
        // outside this plane, or under another changeset. Nothing was
        // re-executed; name the plane receipt that originally applied this
        // exact change when the journal holds one.
        "already-applied" => (
            "no_op",
            journal.original_apply_receipt(&plan.setting_ref, &plan.scope, &plan.plan_digest),
        ),
        other => {
            return Err(error_document(
                "internal",
                &format!("native admission returned an unknown result `{other}`"),
                Some(&plan.setting_ref),
                None,
            ))
        }
    };

    let receipt = ConfigReceipt {
        schema: CONFIG_RECEIPT_SCHEMA.into(),
        receipt_id: format!("factory-receipt-{}", ulid::Ulid::new()),
        owner_ref: CONFIG_OWNER_REF.into(),
        changeset_id,
        plan_digest: Some(plan.plan_digest.clone()),
        setting_ref: plan.setting_ref.clone(),
        scope: plan.scope.clone(),
        operation: "apply".into(),
        outcome: outcome.into(),
        applied_at_unix_ms: now_ms(),
        native_ref: Some(format!(
            "factory:developmental-state:{}:central-project-links:{central_project_ref}",
            state_path.display()
        )),
        expected_effect: Some(central_project_effect()),
        original_receipt_id,
        error: None,
    };
    emit_receipt(&state_path, receipt, json)
}

fn reset_command(parsed: &VerbArgs, json: bool) -> Result<String, String> {
    let setting_ref = parsed
        .setting
        .as_deref()
        .ok_or_else(|| error_document("validation_failed", "missing --setting", None, None))?;
    ensure_operable(setting_ref, Verb::Reset)?;
    let scope = parse_compact_scope(
        parsed
            .scope
            .as_deref()
            .ok_or_else(|| error_document("validation_failed", "missing --scope", None, None))?,
    )?;
    let state_path = resolve_setting_scope(&scope)?;
    let changeset_id = match parsed.changeset.as_deref() {
        Some(changeset) => valid_changeset(changeset)?,
        None => format!("cs-factory-{}", ulid::Ulid::new()),
    };

    // Reset replay: the same executed reset (setting, scope, changeset, no
    // plan) answers no_op naming the original receipt.
    let journal = journal_read(&state_path)?;
    if let Some(original) = journal.executed_receipt(&changeset_id, setting_ref, &scope, None) {
        let replay = ConfigReceipt {
            schema: CONFIG_RECEIPT_SCHEMA.into(),
            receipt_id: format!("factory-receipt-{}", ulid::Ulid::new()),
            owner_ref: CONFIG_OWNER_REF.into(),
            changeset_id,
            plan_digest: None,
            setting_ref: setting_ref.to_owned(),
            scope,
            operation: "reset".into(),
            outcome: "no_op".into(),
            applied_at_unix_ms: now_ms(),
            native_ref: original.native_ref.clone(),
            expected_effect: Some(central_project_effect()),
            original_receipt_id: Some(original.receipt_id.clone()),
            error: None,
        };
        return emit_receipt(&state_path, replay, json);
    }

    if is_telemetry_setting(setting_ref) {
        let sidecar = telemetry_settings_path(&state_path);
        let mut effective: serde_json::Map<String, Value> = std::fs::read_to_string(&sidecar)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let removed = effective.remove(setting_ref).is_some();
        if removed {
            write_telemetry_settings_atomic(&sidecar, &effective).map_err(|error| {
                error_document(
                    "internal",
                    &format!("could not write the telemetry settings sidecar: {error}"),
                    Some(setting_ref),
                    None,
                )
            })?;
        }
        let receipt = ConfigReceipt {
            schema: CONFIG_RECEIPT_SCHEMA.into(),
            receipt_id: format!("factory-receipt-{}", ulid::Ulid::new()),
            owner_ref: CONFIG_OWNER_REF.into(),
            changeset_id,
            plan_digest: None,
            setting_ref: setting_ref.to_owned(),
            scope,
            operation: "reset".into(),
            outcome: if removed {
                "applied".into()
            } else {
                "no_op".into()
            },
            applied_at_unix_ms: now_ms(),
            native_ref: Some(format!(
                "software-factory:telemetry-config:{}",
                sidecar.display()
            )),
            expected_effect: Some(central_project_effect()),
            original_receipt_id: None,
            error: None,
        };
        return emit_receipt(&state_path, receipt, json);
    }
    let mut state = open_state(&scope)?;
    let removed = state.remove_central_project_link().map_err(|error| {
        error_document(
            "validation_failed",
            &format!("native unbind refused: {error}"),
            Some(setting_ref),
            Some(&scope.scope_kind),
        )
    })?;
    let native_ref = removed.map(|link| {
        format!(
            "factory:developmental-state:{}:central-project-links:{}",
            state_path.display(),
            link.central_project_ref
        )
    });
    let receipt = ConfigReceipt {
        schema: CONFIG_RECEIPT_SCHEMA.into(),
        receipt_id: format!("factory-receipt-{}", ulid::Ulid::new()),
        owner_ref: CONFIG_OWNER_REF.into(),
        changeset_id,
        plan_digest: None,
        setting_ref: setting_ref.to_owned(),
        scope,
        operation: "reset".into(),
        // A reset with nothing bound changes nothing: no_op, with no earlier
        // receipt to name.
        outcome: if native_ref.is_some() {
            "applied"
        } else {
            "no_op"
        }
        .into(),
        applied_at_unix_ms: now_ms(),
        native_ref,
        expected_effect: Some(central_project_effect()),
        original_receipt_id: None,
        error: None,
    };
    emit_receipt(&state_path, receipt, json)
}

fn emit_receipt(state_path: &Path, receipt: ConfigReceipt, json: bool) -> Result<String, String> {
    let recorded = receipt.clone();
    journal_write(state_path, |journal| {
        journal.receipts.push(recorded);
        Ok(())
    })?;
    if json {
        return serde_json::to_string_pretty(&receipt).map_err(internal_error);
    }
    Ok(format!(
        "receipt {} — {} {} at {} ({})",
        receipt.receipt_id,
        receipt.operation,
        receipt.setting_ref,
        format_scope_compact(&receipt.scope),
        receipt.outcome
    ))
}

fn valid_changeset(changeset: &str) -> Result<String, String> {
    let shaped = changeset.starts_with("cs-")
        && changeset.len() > 3
        && changeset[3..].chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        });
    if shaped {
        return Ok(changeset.to_owned());
    }
    Err(error_document(
        "validation_failed",
        &format!(
            "changeset id `{changeset}` must be `cs-` followed by letters, digits, `_` or `-`"
        ),
        None,
        None,
    ))
}

fn central_project_effect() -> ExpectedEffect {
    ExpectedEffect {
        kind: "value-change".into(),
        summary: Some(CENTRAL_PROJECT_EFFECT_SUMMARY.into()),
        r#ref: None,
    }
}

// ---------------------------------------------------------------------------
// Journal persistence
// ---------------------------------------------------------------------------

fn journal_path(state_path: &Path) -> PathBuf {
    let mut name = state_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(".config-journal.json");
    state_path.with_file_name(name)
}

fn journal_read(state_path: &Path) -> Result<ConfigJournal, String> {
    let path = journal_path(state_path);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(ConfigJournal::new()),
        Err(error) => return Err(internal_io_document(&path.display().to_string(), &error)),
    };
    if bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Ok(ConfigJournal::new());
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        error_document(
            "internal",
            &format!(
                "configuration journal at {} is unreadable: {error}",
                path.display()
            ),
            None,
            None,
        )
    })
}

fn journal_write(
    state_path: &Path,
    mutate: impl FnOnce(&mut ConfigJournal) -> Result<(), String>,
) -> Result<(), String> {
    let path = journal_path(state_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| internal_io_document("create journal directory", &error))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| internal_io_document(&path.display().to_string(), &error))?;
    file.lock_exclusive()
        .map_err(|error| internal_io_document("lock configuration journal", &error))?;
    let result = (|| {
        let mut journal = journal_read(state_path)?;
        mutate(&mut journal)?;
        let bytes = serde_json::to_vec_pretty(&journal).map_err(internal_error)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| internal_io_document("seek configuration journal", &error))?;
        file.write_all(&bytes)
            .map_err(|error| internal_io_document("write configuration journal", &error))?;
        file.set_len(bytes.len() as u64)
            .map_err(|error| internal_io_document("truncate configuration journal", &error))?;
        Ok(())
    })();
    let _ = file.unlock();
    result
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// `plan_digest` — sha256 hex over the canonical plan body (C0 §6): the plan
/// document with `plan_id` zeroed to `""`, `plan_digest` zeroed to `""` (a
/// digest cannot cover itself), `expires_at_unix_ms` zeroed to `0`, and every
/// other `*_unix_ms` field zeroed. Convention choice, stated: this owner
/// ZEROES those members in the hashed body; sibling owners remove the
/// `plan_digest` member outright. Either satisfies the frozen law as long as
/// mint and verify agree — and both go through this one function here, so the
/// choice is internally consistent by construction. The digest is therefore
/// stable across re-plans of the same change (only `plan_id` and clocks
/// differ), which is what makes it the idempotency anchor.
fn plan_digest(plan: &ConfigPlan) -> String {
    let mut body = serde_json::to_value(plan).expect("plan serialises");
    if let Some(object) = body.as_object_mut() {
        object.insert("plan_id".into(), json!(""));
        object.insert("plan_digest".into(), json!(""));
        object.insert("expires_at_unix_ms".into(), json!(0));
    }
    zero_unix_ms(&mut body);
    sha256_hex(&serde_json::to_string(&body).expect("canonical plan serialises"))
}

/// Zero every `*_unix_ms` field, recursively (the 07 §4.5 convention).
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
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn format_scope_compact(scope: &ConfigScope) -> String {
    match &scope.scope_ref {
        Some(reference) => format!("{}:{reference}", scope.scope_kind),
        None => scope.scope_kind.clone(),
    }
}

fn error_document(
    code: &str,
    message: &str,
    setting_ref: Option<&str>,
    scope_kind: Option<&str>,
) -> String {
    let mut document = Map::new();
    document.insert("schema".into(), json!(CONFIG_ERROR_SCHEMA));
    document.insert("error_code".into(), json!(code));
    document.insert("message".into(), json!(message));
    if let Some(setting_ref) = setting_ref {
        document.insert("setting_ref".into(), json!(setting_ref));
    }
    if let Some(scope_kind) = scope_kind {
        document.insert("scope_kind".into(), json!(scope_kind));
    }
    serde_json::to_string_pretty(&Value::Object(document)).expect("error document serialises")
}

fn internal_error(error: serde_json::Error) -> String {
    error_document("internal", &error.to_string(), None, None)
}

fn internal_io_document(context: &str, error: &io::Error) -> String {
    error_document("internal", &format!("{context}: {error}"), None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const READ_ONLY_ENTRY: SettingEntry = SettingEntry {
        setting_ref: "software-factory:binding:disclosure-only",
        writable: false,
        operations: VerbOps {
            validate: true,
            plan: false,
            apply: false,
            reset: false,
        },
    };

    fn plan_fixture() -> ConfigPlan {
        ConfigPlan {
            schema: CONFIG_PLAN_SCHEMA.into(),
            plan_id: "factory-plan-test".into(),
            plan_digest: String::new(),
            setting_ref: CENTRAL_PROJECT_SETTING_REF.into(),
            scope: ConfigScope {
                scope_kind: "project".into(),
                scope_ref: Some("/tmp/state.json".into()),
            },
            changes: vec![PlanChange {
                summary: "bind".into(),
                native_ref: None,
                before_ref: None,
                after_ref: None,
            }],
            expected_effect: central_project_effect(),
            expires_at_unix_ms: Some(1234567890),
            explain_ref: None,
        }
    }

    #[test]
    fn plan_digest_covers_the_change_not_the_clock_or_the_mint() {
        let mut first = plan_fixture();
        first.plan_digest = plan_digest(&first);
        let mut second = first.clone();
        second.plan_id = "factory-plan-other-mint".into();
        second.expires_at_unix_ms = Some(9999999999);
        second.plan_digest = plan_digest(&second);
        // Different plan_id and expiry, same change: the digest is the
        // idempotency anchor and must be identical.
        assert_eq!(first.plan_digest, second.plan_digest);
        assert_eq!(first.plan_digest.len(), 64);

        // A different requested change hashes differently.
        let mut changed = first.clone();
        changed.scope.scope_ref = Some("/tmp/other.json".into());
        changed.plan_digest = plan_digest(&changed);
        assert_ne!(first.plan_digest, changed.plan_digest);
    }

    #[test]
    fn idempotency_key_is_exact_and_failures_do_not_satisfy_it() {
        let scope = ConfigScope {
            scope_kind: "project".into(),
            scope_ref: Some("/tmp/state.json".into()),
        };
        let receipt = |changeset: &str, digest: Option<&str>, outcome: &str| ConfigReceipt {
            schema: CONFIG_RECEIPT_SCHEMA.into(),
            receipt_id: format!("factory-receipt-{changeset}-{outcome}"),
            owner_ref: CONFIG_OWNER_REF.into(),
            changeset_id: changeset.into(),
            plan_digest: digest.map(str::to_owned),
            setting_ref: CENTRAL_PROJECT_SETTING_REF.into(),
            scope: scope.clone(),
            operation: "apply".into(),
            outcome: outcome.into(),
            applied_at_unix_ms: 0,
            native_ref: None,
            expected_effect: None,
            original_receipt_id: None,
            error: None,
        };
        let mut journal = ConfigJournal::new();
        journal
            .receipts
            .push(receipt("cs-one", Some("d1"), "applied"));

        // Exact key replays; a different changeset is a different key.
        assert!(journal
            .executed_receipt("cs-one", CENTRAL_PROJECT_SETTING_REF, &scope, Some("d1"))
            .is_some());
        assert!(journal
            .executed_receipt("cs-two", CENTRAL_PROJECT_SETTING_REF, &scope, Some("d1"))
            .is_none());
        assert!(journal
            .executed_receipt("cs-one", CENTRAL_PROJECT_SETTING_REF, &scope, Some("d2"))
            .is_none());

        // A failed execution is not an executed key: retry re-attempts.
        journal
            .receipts
            .push(receipt("cs-one", Some("d2"), "failed"));
        assert!(journal
            .executed_receipt("cs-one", CENTRAL_PROJECT_SETTING_REF, &scope, Some("d2"))
            .is_none());

        // The original applier of a digest is findable for honest naming.
        assert_eq!(
            journal.original_apply_receipt(CENTRAL_PROJECT_SETTING_REF, &scope, "d1"),
            Some("factory-receipt-cs-one-applied".into())
        );
        assert_eq!(
            journal.original_apply_receipt(CENTRAL_PROJECT_SETTING_REF, &scope, "d2"),
            None
        );
    }

    #[test]
    fn unknown_settings_and_non_writable_settings_refuse_with_structured_errors() {
        // A foreign product's setting is never absorbed (the #299 C3D law):
        let error = ensure_operable("ai-kit:resolution:model.default", Verb::Plan).unwrap_err();
        assert!(error.contains("unsupported_setting"), "{error}");
        assert!(error.contains("ai-kit:resolution:model.default"));

        // A known but non-writable setting refuses every mutation verb while
        // still answering validate, exactly as its disclosure promises.
        assert!(refusal_reason(
            READ_ONLY_ENTRY.writable,
            READ_ONLY_ENTRY.operations,
            Verb::Validate
        )
        .is_none());
        for verb in [Verb::Plan, Verb::Apply, Verb::Reset] {
            let reason = refusal_reason(READ_ONLY_ENTRY.writable, READ_ONLY_ENTRY.operations, verb)
                .expect("non-writable settings refuse mutation");
            assert!(reason.contains("read-only"), "{reason}");
        }

        // A writable setting whose contribution omits a verb refuses that verb.
        let plan_only = VerbOps {
            validate: true,
            plan: true,
            apply: false,
            reset: false,
        };
        assert!(refusal_reason(true, plan_only, Verb::Apply).is_some());
        // The live registry entry is writable with every verb disclosed.
        assert!(refusal_reason(
            CENTRAL_PROJECT_ENTRY.writable,
            CENTRAL_PROJECT_ENTRY.operations,
            Verb::Reset
        )
        .is_none());
    }

    #[test]
    fn contribution_document_keeps_the_plane_separation_and_owner_identity() {
        let document = build_contribution();
        assert_eq!(document["schema"], CONFIGURATION_CONTRIBUTION_SCHEMA);
        assert_eq!(
            document["contract_revision"],
            CONFIGURATION_CONTRACT_REVISION
        );
        assert_eq!(document["owner"]["owner_ref"], CONFIG_OWNER_REF);
        assert_eq!(document["owner"]["owner_kind"], "product");
        assert_eq!(
            document["owner"]["contribution_command"],
            json!(["factory", "config-contribution", "--json"])
        );

        // The contribution never carries native read-state axes (C0 §1): a
        // document that mixes the planes is invalid.
        let rendered = serde_json::to_string(&document).unwrap();
        for axis in ["\"declared\"", "\"effective\"", "\"active\"", "\"desired\""] {
            assert!(
                !rendered.contains(axis),
                "contribution must not carry the {axis} axis"
            );
        }

        // The refs parse by the frozen grammar and map structurally onto the
        // v2 disclosure sections/keys (owner_ref == product_id, section_ref ==
        // enclosing section id).
        assert_eq!(CONFIG_OWNER_REF, "software-factory");
        let parts: Vec<&str> = CENTRAL_PROJECT_SETTING_REF.split(':').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], CONFIG_OWNER_REF);
        let sections = document["sections"].as_array().unwrap();
        for section in sections {
            for setting in section["settings"].as_array().unwrap() {
                assert_eq!(setting["section_ref"], section["id"]);
                let setting_parts: Vec<&str> = setting["setting_ref"]
                    .as_str()
                    .unwrap()
                    .split(':')
                    .collect();
                assert_eq!(setting_parts[1], section["id"].as_str().unwrap());
            }
        }
    }

    #[test]
    fn the_lane_does_not_absorb_aikit_or_actuation_semantics() {
        let rendered = serde_json::to_string(&build_contribution()).unwrap();
        assert!(!rendered.contains("ai-kit"));
        assert!(!rendered.contains("aikit"));
        assert!(!rendered.to_lowercase().contains("actuation"));
        assert!(!rendered.to_lowercase().contains("model.default"));
        assert!(!rendered.to_lowercase().contains("agency"));
    }

    #[test]
    fn central_project_value_contract_is_exactly_one_binding_row() {
        let row = json!([{"central_project_ref": "project:o-i", "source_path": "/tmp/p.json"}]);
        assert_eq!(
            central_project_value(&row).unwrap(),
            ("project:o-i".into(), "/tmp/p.json".into())
        );
        for invalid in [
            json!("project:o-i"),
            json!([]),
            json!([
                {"central_project_ref": "a", "source_path": "/a"},
                {"central_project_ref": "b", "source_path": "/b"}
            ]),
            json!([{"central_project_ref": "project:o-i"}]),
            json!([{"central_project_ref": "", "source_path": "/p"}]),
        ] {
            let error = central_project_value(&invalid).unwrap_err();
            assert!(error.contains("invalid_value"), "{error}");
        }
    }

    #[test]
    fn telemetry_sidecar_writes_are_atomic_and_leave_no_tmp_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let state_path = dir.path().join("state.json");
        let sidecar = telemetry_settings_path(&state_path);

        let mut effective = serde_json::Map::new();
        effective.insert(TELEMETRY_WATCH_INTERVAL_SETTING.into(), json!(30.0f64));
        write_telemetry_settings_atomic(&sidecar, &effective).expect("first write");

        // The settings are live under their own name and no `.<name>.tmp`
        // sibling survives the rename — a leftover tmp would be the torn
        // write this discipline exists to prevent.
        let written = std::fs::read_to_string(&sidecar).expect("sidecar exists");
        let parsed: serde_json::Map<String, Value> =
            serde_json::from_str(&written).expect("sidecar parses");
        assert_eq!(parsed[TELEMETRY_WATCH_INTERVAL_SETTING], json!(30.0));
        let tmp = sidecar.with_file_name(format!(
            ".{}.tmp",
            sidecar.file_name().unwrap().to_string_lossy()
        ));
        assert!(!tmp.exists(), "no tmp sibling may survive the write");

        // A second write replaces the document whole (rename-over), never
        // merges or appends.
        effective.remove(TELEMETRY_WATCH_INTERVAL_SETTING);
        write_telemetry_settings_atomic(&sidecar, &effective).expect("second write");
        let written = std::fs::read_to_string(&sidecar).expect("sidecar exists");
        let parsed: serde_json::Map<String, Value> =
            serde_json::from_str(&written).expect("sidecar parses");
        assert!(parsed.is_empty(), "reset leaves an empty sidecar");
        assert!(!tmp.exists(), "no tmp sibling after the second write");
    }
}
