//! Factory-side fail-closed validation of AIKit's encounter-task praxis receipt.
//!
//! Factory workflow units carry required, identity-bearing `praxisRefs`. They
//! ride the AIKit NOW preparation (`PreparedFactoryUnit.praxis_refs`) and
//! AIKit resolves them there into the typed receipt
//! `aikit.encounter-task-praxis/v1`, emitted inside the
//! `aikit.now-preparation-result/v1` reading under the `praxis` key. The
//! receipt asserts resolution standing only; it never grants trust,
//! activation, capability or authority.
//!
//! This module is the Factory side of that contract, admitted at attempt
//! admission beside the existing praxis superset check: when a workflow unit
//! demands praxisRefs, the attempt's disposition must carry the receipt for
//! its NOW preparation, the claim must be `resolved`, and every required ref
//! must be resolved with standing `available`. A refused claim, a missing
//! required ref or a lower standing fails the admission closed, echoing the
//! receipt's refusal code, condition and recovery verbatim.
//!
//! The wire types deliberately mirror AIKit's own serde shape rather than
//! Factory's camelCase convention: AIKit's `EncounterTaskPraxisReceipt`
//! derives serde without a rename, so object keys are the verbatim Rust field
//! names (`resolution_hash`, `workflow_unit_ref`) and enums are kebab-case
//! (`resolved`, `ref-absent`, `available`). Unknown members are tolerated so
//! AIKit can extend the receipt without breaking a v1 reader; the schema name
//! pins the contract version, so a future schema is refused as not this
//! contract, not silently accepted.
//!
//! Retrieval is implemented against the read surface AIKit now serves —
//! `aikit now context inspect` returns the owner's published prepared view,
//! and its `data.prepared.praxis` member is the exact receipt that view was
//! published with (ai-kit commit 70fce4cb), never a re-derivation.
//! [`inspect_admission_praxis_receipt`] invokes that one read-only surface
//! through the existing `crate::native_aikit_route` channel and returns the
//! receipt verbatim; [`retrieve_admission_receipt`] gates it on the unit's
//! praxisRefs so a unit that demands none never touches the route. A
//! receipt-less view (or an unpublished one) returns `None`, and a failed or
//! refused inspection returns the named cause: both leave the disposition's
//! receipt absent, where the admission absence law below fails the attempt
//! closed.
//!
//! The admission wiring itself stops at a missing handle, reported rather
//! than invented: the enforcement site `prepare_start`
//! (`attempt_runtime.rs`) holds no NOW-context identity. The only candidate
//! is `disposition.placement.now_ref` (`the `SituatedExecutionDisposition.placement` declaration`), optional and
//! guaranteed only for effectful/materialised attempts
//! (`the admission placement law in `attempt_runtime``); the authoritative allocation happens
//! later, for an already-admitted attempt (`central.now.allocate`,
//! `the `central.now.allocate` call in `attempt_central::prepare``). The inspection's own handles — binary, cwd,
//! transport, the Redis NOW `--config-file` and the `--participant-ref` —
//! ride the dispatch-time `FactoryAttemptOwnerRequest.invocation`
//! (`the `FactoryAttemptOwnerRequest.invocation` declaration`) and exist nowhere in `AttemptStart` or
//! `StoredAttemptState`. Until a caller that holds those handles invokes the
//! retrieval and attaches the result to the disposition
//! ([`crate::attempt_runtime::SituatedExecutionDisposition::praxis_receipt`]),
//! admissions of praxis-bearing units continue to refuse closed with the
//! named absence.

use crate::native_aikit_route::AikitOwnerTransport;
use crate::workflow::CompiledWorkflowUnit;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// AIKit's typed encounter-task praxis receipt contract.
pub const AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA: &str = "aikit.encounter-task-praxis/v1";

/// The involvement-ladder standing of one catalogued praxisRef, mirroring
/// AIKit's `PraxisStanding` (`kebab-case` on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PraxisStanding {
    Available,
    Unavailable,
    Unproven,
}

/// The classification of one resolved praxisRef, mirroring AIKit's
/// `PraxisForm` (`kebab-case` on the wire). Factory carries it verbatim and
/// never derives a form of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PraxisForm {
    Skill,
    Method,
    Methodology,
}

/// One resolved praxisRef, mirroring AIKit's `ResolvedPraxisRefStanding`.
/// Field names are the verbatim wire names (no rename), matching the AIKit
/// source type exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPraxisRefStanding {
    pub reference: String,
    pub form: PraxisForm,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub standing: PraxisStanding,
}

/// Machine-readable refusal codes, mirroring AIKit's `PraxisRefusalCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PraxisRefusalCode {
    RefAbsent,
    RefWithheld,
    RefStandingUnproven,
}

/// One unresolvable praxisRef, named exactly, mirroring AIKit's
/// `PraxisRefRefusal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PraxisRefRefusal {
    pub reference: String,
    pub code: PraxisRefusalCode,
    pub condition: String,
    pub recovery: String,
}

/// The praxis resolution of one workflow unit's required refs, mirroring
/// AIKit's `UnitPraxisResolution`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitPraxisResolution {
    pub workflow_unit_ref: String,
    #[serde(default)]
    pub resolved: Vec<ResolvedPraxisRefStanding>,
    #[serde(default)]
    pub refusals: Vec<PraxisRefRefusal>,
}

/// The praxis claim of the reading, mirroring AIKit's `PraxisClaim`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PraxisClaim {
    Resolved,
    Refused,
}

/// The typed receipt, mirroring AIKit's `EncounterTaskPraxisReceipt` member
/// for member. Field names are the verbatim wire names (no rename).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounterTaskPraxisReceipt {
    pub schema: String,
    pub resolution_hash: String,
    pub claim: PraxisClaim,
    #[serde(default)]
    pub units: Vec<UnitPraxisResolution>,
    #[serde(default)]
    pub standing: String,
}

/// Validate one attempt's praxis receipt against the workflow unit being
/// admitted.
///
/// `receipt` is the raw receipt JSON as carried on the disposition;
/// `require_present` distinguishes the two enforcement moments this repo
/// keeps apart:
///
/// * `true` — fresh admission (`prepare_start`): a unit that demands
///   praxisRefs refuses closed when no receipt accompanies the attempt. This
///   is the repo's placement-basis law ("requires an explicit
///   placement/protection basis"), not the verification law: praxis identity
///   is checked when the arrangement is admitted, like participant and
///   placement requirements.
/// * `false` — reopen (`validate_state`): a carried receipt is validated (a
///   corrupt or non-covering receipt is corrupt state) but its absence is
///   historical and stays valid. New admission checks do not retroactively
///   erase historical executions whose older compiler lacked this rule.
///
/// A unit carrying no praxisRefs passes unchanged with or without a receipt:
/// the check stays additive.
pub fn validate_praxis_receipt(
    receipt: Option<&Value>,
    unit: &CompiledWorkflowUnit,
    require_present: bool,
) -> Result<(), String> {
    if unit.praxis_refs.is_empty() {
        return Ok(());
    }
    let Some(receipt) = receipt else {
        return if require_present {
            Err(format!(
                "workflow unit {} demands praxisRefs {:?} but the attempt carries no AIKit \
                 NOW-preparation praxis receipt; admission fails closed until the \
                 {AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA} receipt resolves them",
                unit.reference, unit.praxis_refs
            ))
        } else {
            Ok(())
        };
    };
    let parsed: EncounterTaskPraxisReceipt =
        serde_json::from_value(receipt.clone()).map_err(|error| {
            format!(
                "AIKit praxis receipt is not the typed {AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA} \
                 contract: {error}"
            )
        })?;
    if parsed.schema != AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA {
        return Err(format!(
            "AIKit praxis receipt names schema {} instead of \
             {AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA}",
            parsed.schema
        ));
    }
    // Any refusal fails the whole claim closed, exactly as the receipt's own
    // law states; a receipt that claims `resolved` while still carrying a
    // refusal entry is refused here rather than believed.
    for entry in &parsed.units {
        if let Some(refusal) = entry.refusals.first() {
            return Err(echoed_refusal(refusal));
        }
    }
    match parsed.claim {
        PraxisClaim::Resolved => {}
        PraxisClaim::Refused => {
            return Err(
                "AIKit praxis receipt refuses this attempt's praxis claim; the reading names \
                 no resolvable condition"
                    .to_string(),
            )
        }
    }
    let Some(entry) = parsed
        .units
        .iter()
        .find(|entry| entry.workflow_unit_ref == unit.reference.to_string())
    else {
        return Err(format!(
            "AIKit praxis receipt does not resolve workflow unit {} required praxisRefs {:?}",
            unit.reference, unit.praxis_refs
        ));
    };
    for reference in &unit.praxis_refs {
        let standing = entry
            .resolved
            .iter()
            .find(|standing| &standing.reference == reference)
            .ok_or_else(|| {
                format!(
                    "AIKit praxis receipt does not resolve required praxisRef {reference} \
                     for workflow unit {}",
                    unit.reference
                )
            })?;
        if standing.standing != PraxisStanding::Available {
            return Err(format!(
                "required praxisRef {reference} stands {} in the AIKit praxis receipt; \
                 only available standing admits workflow unit {}",
                standing_name(standing.standing),
                unit.reference
            ));
        }
    }
    Ok(())
}

fn echoed_refusal(refusal: &PraxisRefRefusal) -> String {
    format!(
        "AIKit praxis receipt refuses praxisRef {} ({}): {}; recovery: {}",
        refusal.reference,
        refusal_code_name(refusal.code),
        refusal.condition,
        refusal.recovery
    )
}

/// The kebab-case wire name of a refusal code, echoed verbatim into the
/// admission refusal.
fn refusal_code_name(code: PraxisRefusalCode) -> &'static str {
    match code {
        PraxisRefusalCode::RefAbsent => "ref-absent",
        PraxisRefusalCode::RefWithheld => "ref-withheld",
        PraxisRefusalCode::RefStandingUnproven => "ref-standing-unproven",
    }
}

/// The kebab-case wire name of a standing, echoed verbatim into the
/// admission refusal.
fn standing_name(standing: PraxisStanding) -> &'static str {
    match standing {
        PraxisStanding::Available => "available",
        PraxisStanding::Unavailable => "unavailable",
        PraxisStanding::Unproven => "unproven",
    }
}

/// The schema of AIKit's read-only NOW-context inspection, the envelope
/// `data` member `aikit now context inspect` answers with.
pub const AIKIT_NOW_CONTEXT_INSPECTION_SCHEMA: &str = "aikit.now-context-inspection/v1";

/// The exact handles the read-only `aikit now context inspect` surface
/// needs, carried by the caller that holds them. Nothing here is derived or
/// discovered: the binary/cwd/transport follow the same declared native
/// owner route as Task inspection, the config file is the owner's Redis NOW
/// config path, and the participant reference names the prepared context's
/// owner. Every member is opaque to this module.
pub struct NowContextInspect<'a> {
    pub binary: &'a Path,
    pub cwd: &'a Path,
    pub transport: Option<&'a AikitOwnerTransport>,
    pub config_file: &'a Path,
    pub participant_ref: &'a str,
    pub timeout: Duration,
}

/// Whether admission retrieval applies to one compiled unit: only a unit
/// whose workflow law demands praxisRefs has a receipt to retrieve. A unit
/// without praxisRefs must never touch the route at all.
pub fn retrieval_applies(unit: &CompiledWorkflowUnit) -> bool {
    !unit.praxis_refs.is_empty()
}

/// The admission retrieval entry point: the typed receipt for one unit's NOW
/// preparation, or `None` when the unit demands no praxisRefs (the route is
/// not contacted at all) or the inspected view carries no receipt. A
/// retrieval failure returns the named cause; both `None` results leave the
/// disposition's receipt absent, where the admission absence law fails the
/// attempt closed. The returned value is the surface's `data.prepared.praxis`
/// verbatim — never fabricated, transformed or re-derived.
pub fn retrieve_admission_receipt(
    unit: &CompiledWorkflowUnit,
    inspect: &NowContextInspect<'_>,
) -> Result<Option<Value>, String> {
    if !retrieval_applies(unit) {
        return Ok(None);
    }
    inspect_admission_praxis_receipt(inspect)
}

/// Inspect one prepared NOW context through the declared native AIKit route
/// and return its published praxis receipt verbatim (`Ok(None)` when the
/// view is unpublished or receipt-less). The invocation is exactly
/// `aikit -C <cwd> --json now context inspect --config-file <path>
/// --participant-ref <ref>`; a non-success exit, a non-JSON response, a
/// failure envelope or a wrong inspection schema is a named error carrying
/// the bounded native response.
pub fn inspect_admission_praxis_receipt(
    inspect: &NowContextInspect<'_>,
) -> Result<Option<Value>, String> {
    let config_file = inspect
        .config_file
        .to_str()
        .filter(|path| !path.is_empty() && !path.contains('\0'))
        .ok_or(
            "Native AIKit NOW-context inspection requires a UTF-8 Redis NOW config path without NUL",
        )?;
    if inspect.participant_ref.is_empty() || inspect.participant_ref.contains('\0') {
        return Err(
            "Native AIKit NOW-context inspection requires the prepared context's participant \
             reference"
                .into(),
        );
    }
    let arguments = [
        "--json".to_owned(),
        "now".to_owned(),
        "context".to_owned(),
        "inspect".to_owned(),
        "--config-file".to_owned(),
        config_file.to_owned(),
        "--participant-ref".to_owned(),
        inspect.participant_ref.to_owned(),
    ];
    let output = crate::native_aikit_route::output(
        inspect.binary,
        inspect.cwd,
        inspect.transport,
        &arguments,
        inspect.timeout,
    )
    .map_err(|error| {
        format!("Native AIKit NOW-context inspection failed before dispatch: {error}")
    })?;
    let bounded =
        |bytes: &[u8]| String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    if !output.status.success() {
        return Err(format!(
            "Native AIKit NOW-context inspection refused; exit {:?}; stderr {}; response {}",
            output.status.code(),
            bounded(&output.stderr),
            bounded(&output.stdout)
        ));
    }
    let envelope: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Native AIKit NOW-context inspection is not JSON: {error}"))?;
    if envelope["ok"] != true {
        return Err(format!(
            "Native AIKit NOW-context inspection did not succeed: {}",
            bounded(&output.stdout)
        ));
    }
    if envelope["data"]["schema"] != AIKIT_NOW_CONTEXT_INSPECTION_SCHEMA {
        return Err(format!(
            "Native AIKit NOW-context inspection names schema {} instead of \
             {AIKIT_NOW_CONTEXT_INSPECTION_SCHEMA}",
            envelope["data"]["schema"]
        ));
    }
    // The prepared view is optional on the surface, and the receipt member is
    // omitted from a receipt-less published view; both stay None so the
    // admission absence law owns exactly that case.
    let receipt = &envelope["data"]["prepared"]["praxis"];
    if receipt.is_null() {
        return Ok(None);
    }
    Ok(Some(receipt.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn compiled_unit(praxis_refs: &[&str]) -> CompiledWorkflowUnit {
        let source = json!({
            "ref": "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            "key": "inspect-source",
            "developmentalConcern": "concern",
            "requiredDifference": "difference",
            "returnContract": "return",
            "subjectRef": "project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
            "basisRevision": "r1",
            "agentRequirements": {"agentRefs": ["agent:test"], "agentSetRefs": [], "agencyRefs": []},
            "praxisRefs": praxis_refs,
            "capabilityRefs": ["capability:01ARZ3NDEKTSV4RRFFQ69G5FB0"],
            "dependencies": [],
            "independenceFrom": [],
            "permittedEffects": ["effect:test"],
            "verificationObligations": ["obligation:test"],
            "returnAddress": "return:01ARZ3NDEKTSV4RRFFQ69G5FB4",
            "stopConditions": "stop",
            "escalationConditions": "escalate"
        });
        serde_json::from_value(source).unwrap()
    }

    fn resolved_receipt(unit_ref: &str, refs: &[&str]) -> Value {
        json!({
            "schema": AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA,
            "resolution_hash": "resolution:01ARZ3NDEKTSV4RRFFQ69G5FB8",
            "claim": "resolved",
            "units": [{
                "workflow_unit_ref": unit_ref,
                "resolved": refs.iter().map(|reference| json!({
                    "reference": reference,
                    "form": "skill",
                    "revision": "r1",
                    "standing": "available"
                })).collect::<Vec<_>>(),
                "refusals": []
            }],
            "standing": "resolution standing only"
        })
    }

    #[test]
    fn a_resolved_available_receipt_admits_a_demanding_unit() {
        let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
        let receipt = resolved_receipt(
            "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            &["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"],
        );
        assert_eq!(validate_praxis_receipt(Some(&receipt), &unit, true), Ok(()));
    }

    #[test]
    fn a_receipt_resolving_more_than_demanded_still_admits_the_required_refs() {
        // The receipt reports the NOW context's own resolution; a context
        // that carried more refs than the compiled unit demands resolves
        // them too. Only the unit's required refs are admission law here.
        let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
        let receipt = resolved_receipt(
            "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            &[
                "skill:01ARZ3NDEKTSV4RRFFQ69G5FAY",
                "skill:01ARZ3NDEKTSV4RRFFQ69G5FB2",
            ],
        );
        assert_eq!(validate_praxis_receipt(Some(&receipt), &unit, true), Ok(()));
    }

    #[test]
    fn a_missing_receipt_refuses_admission_but_not_reopen() {
        let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
        let error = validate_praxis_receipt(None, &unit, true).unwrap_err();
        assert!(error.contains("carries no AIKit NOW-preparation praxis receipt"));
        assert!(error.contains(AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA));
        assert_eq!(validate_praxis_receipt(None, &unit, false), Ok(()));
    }

    #[test]
    fn a_refused_receipt_echoes_code_condition_and_recovery() {
        let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
        let receipt = json!({
            "schema": AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA,
            "resolution_hash": "resolution:01ARZ3NDEKTSV4RRFFQ69G5FB8",
            "claim": "refused",
            "units": [{
                "workflow_unit_ref": "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
                "resolved": [],
                "refusals": [{
                    "reference": "skill:01ARZ3NDEKTSV4RRFFQ69G5FAY",
                    "code": "ref-absent",
                    "condition": "praxisRef is absent from the resolved catalogue",
                    "recovery": "author or register the Skill the praxisRef names"
                }]
            }],
            "standing": "resolution standing only"
        });
        let error = validate_praxis_receipt(Some(&receipt), &unit, true).unwrap_err();
        assert!(error.contains("ref-absent"), "{error}");
        assert!(
            error.contains("absent from the resolved catalogue"),
            "{error}"
        );
        assert!(error.contains("recovery: author or register"), "{error}");
    }

    #[test]
    fn an_unproven_standing_and_a_missing_refusal_entry_both_refuse() {
        let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
        let unproven = json!({
            "schema": AIKIT_ENCOUNTER_TASK_PRAXIS_SCHEMA,
            "resolution_hash": "resolution:01ARZ3NDEKTSV4RRFFQ69G5FB8",
            "claim": "refused",
            "units": [{
                "workflow_unit_ref": "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
                "resolved": [],
                "refusals": [{
                    "reference": "skill:01ARZ3NDEKTSV4RRFFQ69G5FAY",
                    "code": "ref-standing-unproven",
                    "condition": "standing is unproven: neither trusted nor active",
                    "recovery": "review the revision: aikit trust record"
                }]
            }],
            "standing": "resolution standing only"
        });
        let error = validate_praxis_receipt(Some(&unproven), &unit, true).unwrap_err();
        assert!(error.contains("ref-standing-unproven"), "{error}");
        assert!(error.contains("aikit trust record"), "{error}");

        // A crafted receipt claiming resolved while a refusal entry remains
        // is refused, not believed.
        let mut lying = resolved_receipt(
            "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            &["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"],
        );
        lying["claim"] = json!("resolved");
        lying["units"][0]["refusals"] = json!([{
            "reference": "skill:other",
            "code": "ref-withheld",
            "condition": "withheld by resolution",
            "recovery": "review the revision"
        }]);
        let error = validate_praxis_receipt(Some(&lying), &unit, true).unwrap_err();
        assert!(error.contains("ref-withheld"), "{error}");
    }

    #[test]
    fn a_claimed_resolved_receipt_missing_a_required_ref_refuses() {
        let unit = compiled_unit(&[
            "skill:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            "skill:01ARZ3NDEKTSV4RRFFQ69G5FB2",
        ]);
        let receipt = resolved_receipt(
            "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            &["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"],
        );
        let error = validate_praxis_receipt(Some(&receipt), &unit, true).unwrap_err();
        assert!(
            error.contains("does not resolve required praxisRef"),
            "{error}"
        );
        assert!(
            error.contains("skill:01ARZ3NDEKTSV4RRFFQ69G5FB2"),
            "{error}"
        );
    }

    #[test]
    fn an_entry_for_another_unit_and_a_wrong_standing_both_refuse() {
        let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
        let other_unit = resolved_receipt(
            "unit:01ARZ3NDEKTSV4RRFFQ69G5FBA",
            &["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"],
        );
        let error = validate_praxis_receipt(Some(&other_unit), &unit, true).unwrap_err();
        assert!(error.contains("does not resolve workflow unit"), "{error}");

        let mut unproven = resolved_receipt(
            "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            &["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"],
        );
        unproven["units"][0]["resolved"][0]["standing"] = json!("unproven");
        let error = validate_praxis_receipt(Some(&unproven), &unit, true).unwrap_err();
        assert!(error.contains("stands unproven"), "{error}");
        assert!(error.contains("only available standing admits"), "{error}");
    }

    #[test]
    fn a_receipt_that_is_not_this_contract_refuses() {
        let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
        let wrong_schema = resolved_receipt(
            "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            &["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"],
        );
        let mut renamed = wrong_schema.clone();
        renamed["schema"] = json!("aikit.encounter-task-praxis/v2");
        assert!(validate_praxis_receipt(Some(&renamed), &unit, true)
            .unwrap_err()
            .contains("instead of"));
        assert!(
            validate_praxis_receipt(
                Some(&json!({"schema": "aikit.encounter-task/v1"})),
                &unit,
                true
            )
            .unwrap_err()
            .contains("not the typed"),
            "missing members are a parse refusal"
        );
        let _ = wrong_schema;
    }

    #[test]
    fn a_unit_without_praxis_refs_passes_unchanged_with_or_without_a_receipt() {
        let unit = compiled_unit(&[]);
        assert_eq!(validate_praxis_receipt(None, &unit, true), Ok(()));
        assert_eq!(validate_praxis_receipt(None, &unit, false), Ok(()));
        let receipt = resolved_receipt("workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY", &[]);
        assert_eq!(validate_praxis_receipt(Some(&receipt), &unit, true), Ok(()));
    }

    /// Controlled subprocess doubles exercise the real native AIKit route and
    /// the exact read-only inspection invocation. They are owner protocol
    /// doubles, not an installed AIKit proof. The double is a plain /bin/sh
    /// script: a cheap process that must never perturb the suite's other
    /// timing-sensitive native tests.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    mod inspect {
        use super::*;
        use std::fs;

        const UNIT_REF: &str = "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY";
        const PARTICIPANT: &str = "agent:praxis-worker";
        const CONFIG_FILE: &str = "now-config.json";

        // The double refuses any argv that is not exactly the canonical
        // `aikit -C <cwd> --json now context inspect --config-file <path>
        // --participant-ref <ref>` read-only surface, records the call, and
        // serves the prepared envelope (or the scripted refusal).
        const INSPECT: &str = r#"#!/bin/sh
if [ "$#" -ne 10 ] || [ "$1" != "-C" ] || case "$2" in /*) false ;; *) true ;; esac \
   || [ "$3" != "--json" ] || [ "$4" != "now" ] || [ "$5" != "context" ] \
   || [ "$6" != "inspect" ] || [ "$7" != "--config-file" ] || [ "$9" != "--participant-ref" ]; then
    printf '%s\n' '{"schema":1,"ok":false,"error":{"code":"test.wrong_argv","message":"not the canonical read-only inspection","details":{}}}'
    exit 2
fi
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
printf '%s %s\n' "$8" "${10}" >> "$dir/calls.jsonl"
mode=$(cat "$dir/mode")
case "$mode" in
    fail)
        printf '%s\n' '{"schema":1,"ok":false,"error":{"code":"now_context.unavailable","message":"service unavailable","details":{}}}'
        exit 4
        ;;
    garbage)
        printf '%s\n' 'TEST_ONLY not JSON'
        exit 0
        ;;
    *)
        cat "$dir/envelope.json"
        ;;
esac
"#;

        /// One scripted owner answer: the served envelope is built by the
        /// test, so the double stays a transport double and never mints a
        /// contract of its own.
        enum Answer {
            /// A success envelope carrying this exact `data` member.
            Serve(Value),
            /// A failure envelope with exit 4.
            Fail,
            /// Exit 0 with a non-JSON body.
            Garbage,
        }

        struct Double {
            dir: tempfile::TempDir,
            route: NowContextInspect<'static>,
        }

        impl Double {
            fn new(answer: Answer) -> Self {
                let dir = tempfile::tempdir().unwrap();
                let binary = dir.path().join("aikit-inspect.sh");
                // Exec'd, so a child writes it (see `crate::test_executable`);
                // the data files beside it are written in-process.
                crate::test_executable::write_executable(&binary, INSPECT);
                match answer {
                    Answer::Serve(data) => {
                        fs::write(dir.path().join("mode"), "serve").unwrap();
                        fs::write(
                            dir.path().join("envelope.json"),
                            json!({
                                "schema": 1,
                                "ok": true,
                                "context": {"context_id": null, "session_id": null, "project_root": null},
                                "data": data,
                                "warnings": []
                            })
                            .to_string(),
                        )
                        .unwrap();
                    }
                    Answer::Fail => fs::write(dir.path().join("mode"), "fail").unwrap(),
                    Answer::Garbage => fs::write(dir.path().join("mode"), "garbage").unwrap(),
                }
                // The route borrows for 'static: leak the exact handles for
                // the test's lifetime; the TempDir owns the directory and
                // cleans it up on drop.
                let cwd: &Path = Box::leak(dir.path().to_path_buf().into_boxed_path());
                let binary: &Path = Box::leak(binary.into_boxed_path());
                let config_file: &Path = Box::leak(dir.path().join(CONFIG_FILE).into_boxed_path());
                let participant_ref: &str = Box::leak(PARTICIPANT.to_string().into_boxed_str());
                Self {
                    dir,
                    route: NowContextInspect {
                        binary,
                        cwd,
                        transport: None,
                        config_file,
                        participant_ref,
                        timeout: Duration::from_secs(10),
                    },
                }
            }

            fn calls(&self) -> Option<Value> {
                let logged = fs::read_to_string(self.dir.path().join("calls.jsonl")).ok()?;
                let mut fields = logged.lines().next()?.split(' ');
                Some(json!({"config": fields.next()?, "participant": fields.next()?}))
            }
        }

        fn inspection(prepared: Value) -> Value {
            json!({
                "schema": AIKIT_NOW_CONTEXT_INSPECTION_SCHEMA,
                "participantRef": PARTICIPANT,
                "currentVersion": 7,
                "prepared": prepared,
            })
        }

        fn prepared_view(receipt: Option<Value>) -> Value {
            let mut view = json!({
                "schema": "aikit.now-prepared/v1",
                "participant_ref": PARTICIPANT,
                "now_ref": "now:01ARZ3NDEKTSV4RRFFQ69G5FB6",
                "prepared_at_unix_ms": 1_760_000_000_000u64,
            });
            if let Some(receipt) = receipt {
                view["praxis"] = receipt;
            }
            view
        }

        #[test]
        fn the_inspected_receipt_is_returned_verbatim_through_the_canonical_route() {
            let receipt = resolved_receipt(UNIT_REF, &["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
            let double = Double::new(Answer::Serve(inspection(prepared_view(Some(
                receipt.clone(),
            )))));
            let unit = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
            let retrieved = retrieve_admission_receipt(&unit, &double.route).unwrap();
            assert_eq!(
                retrieved.as_ref(),
                Some(&receipt),
                "receipt must ride verbatim"
            );
            let call = double
                .calls()
                .expect("the read-only route was contacted once");
            assert_eq!(call["participant"], json!(PARTICIPANT));
            assert!(
                call["config"].as_str().unwrap_or("").ends_with(CONFIG_FILE),
                "config file passed through: {call}"
            );
        }

        #[test]
        fn a_receiptless_or_unpublished_view_returns_none_for_a_demanding_unit() {
            let demanding = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
            let absent = Double::new(Answer::Serve(inspection(prepared_view(None))));
            assert_eq!(
                retrieve_admission_receipt(&demanding, &absent.route).unwrap(),
                None,
                "a receipt-less published view stays None"
            );
            let unpublished = Double::new(Answer::Serve(inspection(json!(null))));
            assert_eq!(
                retrieve_admission_receipt(&demanding, &unpublished.route).unwrap(),
                None,
                "an unpublished participant view stays None"
            );
        }

        #[test]
        fn a_refused_or_unreadable_inspection_names_its_cause() {
            let demanding = compiled_unit(&["skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"]);
            let refused = Double::new(Answer::Fail);
            let error = retrieve_admission_receipt(&demanding, &refused.route).unwrap_err();
            assert!(error.contains("NOW-context inspection"), "{error}");
            assert!(error.contains("exit"), "{error}");

            let garbage = Double::new(Answer::Garbage);
            let error = retrieve_admission_receipt(&demanding, &garbage.route).unwrap_err();
            assert!(error.contains("is not JSON"), "{error}");

            let wrong = Double::new(Answer::Serve(
                json!({"schema": "other.surface/v1", "prepared": prepared_view(None)}),
            ));
            let error = retrieve_admission_receipt(&demanding, &wrong.route).unwrap_err();
            assert!(error.contains("instead of"), "{error}");
            assert!(
                error.contains(AIKIT_NOW_CONTEXT_INSPECTION_SCHEMA),
                "{error}"
            );
        }

        #[test]
        fn a_unit_without_praxis_refs_never_touches_the_route() {
            let quiet = Double::new(Answer::Fail);
            let unit = compiled_unit(&[]);
            assert!(!retrieval_applies(&unit));
            assert_eq!(
                retrieve_admission_receipt(&unit, &quiet.route).unwrap(),
                None,
                "no retrieval outcome for a unit that demands no praxisRefs"
            );
            assert!(
                !quiet.dir.path().join("calls.jsonl").exists(),
                "the read-only route must not be contacted for a unit without praxisRefs"
            );
        }
    }
}
