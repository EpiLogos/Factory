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
//! Retrieval is a reported gap, not new plumbing: today no AIKit surface
//! returns the receipt to Factory. It is emitted only at prepare time
//! (aikit `jev_now.rs`, `reading["praxis"]`), the published prepared view
//! carries no praxis field, and the dispatched `aikit.encounter-task/v1`
//! reading does not include the preparation. The receipt therefore reaches
//! admission only when the process already holds it, carried additively on
//! [`crate::attempt_runtime::SituatedExecutionDisposition::praxis_receipt`].
//! When an AIKit read surface exists, the disposition constructor attaches it
//! through the existing `crate::native_aikit_route` channel; until then
//! admissions of praxis-bearing units refuse closed with the named absence.

use crate::workflow::CompiledWorkflowUnit;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}
