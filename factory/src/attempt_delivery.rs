//! Delivery is a projection of the existing Run/attempt history. There is no
//! separate workflow database or publisher here. An explicitly admitted single
//! publisher performs Git effects; this owner retains intent and reads back the
//! exact remote effect before permitting another publication.
//!
//! Algorithm source: kunchenguid/no-mistakes at
//! 32e4de0b3e07f794fe11707d01e4c67e236213f0, ci.go/ci_fix.go/push.go:
//! enumerate checks fail-closed, retain repair rounds, re-review corrections,
//! compare remote head and reconcile an uncertain effect before another push.
//! This is an original accommodation to Factory's native attempt operations;
//! no upstream code is copied.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_runtime::*;
use crate::cli::CliError;
use crate::core::run::RunRef;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::{Duration, Instant};

pub const DELIVERY_ACTION: &str = "factory.attempt-delivery-action/v1";
pub const DELIVERY_READING: &str = "factory.attempt-delivery-reading/v1";
pub const DELIVERY_OBSERVATION: &str = "factory.attempt-delivery-observation/v1";
const MAX_CHECKS: usize = 1000;
const MAX_LOG_BYTES: usize = 32 * 1024;
const SOURCE_CUT_MARKER: &str = "OI_DEVELOPMENT_FIELD_CUT_JSON=";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCommit {
    pub repository: String,
    pub revision: String,
    pub tree: String,
}

/// A cross-product workflow runs at its own source revision. Its executed owner
/// cuts must be proved, rather than relabelling that workflow's head as the
/// Factory candidate or treating an editable artifact reference as a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossOwnerVerification {
    pub repository: String,
    pub workflow_revision: String,
    pub run_id: Option<u64>,
    pub run_attempt: Option<u64>,
    pub required_jobs: BTreeSet<String>,
    pub aikit: SourceCommit,
    pub factory: SourceCommit,
    /// Material execution is enrolled explicitly; older two-owner operations
    /// retain their original source fingerprint and verification obligation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workcell: Option<SourceCommit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryPlan {
    pub intent_ref: String,
    pub base_revision: String,
    pub candidate_revision: String,
    pub candidate_fingerprint: String,
    pub subject_ref: String,
    pub subject_revision: String,
    pub verification_plan_ref: String,
    pub verification_plan_revision: String,
    pub source_basis_refs: BTreeSet<String>,
    pub repository: String,
    pub branch: String,
    pub pull_request: Option<u64>,
    pub publisher_ref: String,
    pub expected_remote_head: Option<String>,
    pub required_checks: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_owner_verification: Option<CrossOwnerVerification>,
    pub maximum_repairs: u32,
    pub maximum_environment_retries: u32,
    pub timeout_ms: u64,
    /// The first route deliberately requires independent correction review.
    pub revalidate_repairs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostedCheck {
    pub name: String,
    pub head_revision: String,
    pub run_id: String,
    pub run_attempt: u64,
    pub status: String,
    pub conclusion: Option<String>,
    pub evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostedObservation {
    pub candidate_revision: String,
    #[serde(default)]
    pub candidate_fingerprint: Option<String>,
    pub remote_head: String,
    pub enumerated: bool,
    pub checks: Vec<HostedCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_owner: Option<CrossOwnerObservation>,
    pub evidence_refs: BTreeSet<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossOwnerObservation {
    pub basis: CrossOwnerVerification,
    pub checks: Vec<HostedCheck>,
    pub source_cut: Value,
    pub evidence_refs: BTreeSet<String>,
    pub enumerated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryRequest {
    pub contract: String,
    pub request_ref: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub attempt_ref: String,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub operation: DeliveryOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum DeliveryOperation {
    Select {
        plan: DeliveryPlan,
    },
    ObserveGithub {
        gh_binary: String,
    },
    /// Controlled orchestration input. This cannot claim actual GitHub proof:
    /// live acceptance uses observe-github and the owner-read transport.
    ObserveControlled {
        observation: HostedObservation,
    },
    RequestRepair {
        observation_ref: String,
        failure_class: String,
        failure_logs: String,
        artifact_refs: BTreeSet<String>,
        prior_decision_refs: BTreeSet<String>,
    },
    ReservePublication {
        reviewer_attempt_ref: String,
        gh_binary: String,
    },
    ReconcilePublication {
        gh_binary: String,
        publication_ref: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReading {
    pub contract: &'static str,
    pub run_ref: RunRef,
    pub attempt_ref: String,
    pub revision: u64,
    pub candidate_revision: Option<String>,
    pub candidate_fingerprint: Option<String>,
    pub required_checks: BTreeSet<String>,
    pub cross_owner_verification: Option<CrossOwnerVerification>,
    pub checks: Vec<HostedCheck>,
    pub cross_owner: Option<CrossOwnerObservation>,
    pub review_evidence_refs: BTreeSet<String>,
    pub publication_evidence_refs: BTreeSet<String>,
    pub failures: Vec<String>,
    pub failure_evidence_refs: BTreeSet<String>,
    pub repair_evidence_refs: BTreeSet<String>,
    pub repairs: usize,
    pub environment_retries: usize,
    pub ready: bool,
    pub next_action: String,
}

fn invalid(message: impl Into<String>) -> FactoryAttemptError {
    FactoryAttemptError::InvalidOperation(message.into())
}
fn text(value: &str, field: &str) -> Result<(), FactoryAttemptError> {
    if value.trim().is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} needs bounded explicit text")));
    }
    Ok(())
}
fn sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn plan(record: &FactoryAttemptRecord) -> Result<Option<DeliveryPlan>, FactoryAttemptError> {
    record
        .observations
        .iter()
        .rev()
        .find(|r| r.contract == DELIVERY_OBSERVATION && r.payload["kind"] == "selection")
        .map(|r| serde_json::from_value(r.payload["plan"].clone()).map_err(Into::into))
        .transpose()
}
fn last_ci(
    record: &FactoryAttemptRecord,
) -> Result<Option<(&OwnerOperationReceipt, HostedObservation)>, FactoryAttemptError> {
    record
        .observations
        .iter()
        .rev()
        .find(|r| r.contract == DELIVERY_OBSERVATION && r.payload["kind"] == "hosted-ci")
        .map(|r| Ok((r, serde_json::from_value(r.payload["observation"].clone())?)))
        .transpose()
}
fn current<'a>(
    reading: &'a FactoryAttemptReading,
    attempt: &str,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    let record = reading
        .attempts
        .iter()
        .find(|r| r.attempt_ref == attempt)
        .ok_or_else(|| invalid("delivery attempt absent"))?;
    if !reading.source_current
        || reading
            .legs
            .get(&record.workflow_unit_ref)
            .is_none_or(|leg| {
                record
                    .execution_ref
                    .as_deref()
                    .unwrap_or(&record.reserved_execution_ref)
                    != leg.execution_ref
            })
    {
        return Err(invalid(
            "delivery needs exact current attempt and authored source",
        ));
    }
    Ok(record)
}
fn repository(value: &str) -> bool {
    value.split('/').count() == 2
        && value
            .split('/')
            .all(|v| !v.is_empty() && v != "." && v != "..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
}
fn validate_cross_owner(plan: &DeliveryPlan) -> Result<(), FactoryAttemptError> {
    let Some(cross) = &plan.cross_owner_verification else {
        return Ok(());
    };
    if !repository(&cross.repository)
        || !sha(&cross.workflow_revision)
        || cross.required_jobs.is_empty()
        || cross.required_jobs.len() > 20
        || cross.run_id.is_some() != cross.run_attempt.is_some()
        || cross.run_id == Some(0)
        || cross.run_attempt == Some(0)
        || cross.aikit.repository != "EpiLogos/ai-kit"
        || !sha(&cross.aikit.revision)
        || !sha(&cross.aikit.tree)
        || cross.factory.repository != plan.repository
        || cross.factory.revision != plan.candidate_revision
        || candidate_tree(plan).ok() != Some(cross.factory.tree.as_str())
        || cross.workcell.as_ref().is_some_and(|source| {
            source.repository != "EpiLogos/Workcell"
                || source.revision.len() != 40
                || !sha(&source.revision)
                || source.tree.len() != 40
                || !sha(&source.tree)
        })
    {
        return Err(invalid("cross-owner verification needs exact workflow/run/attempt and immutable owner commits/trees"));
    }
    for name in &cross.required_jobs {
        text(name, "cross-owner required job")?;
    }
    Ok(())
}
/// Immutable source material reviewed before hosted dispatch. Run IDs are read
/// afterwards and deliberately do not enter this source fingerprint.
pub fn cross_owner_material_ref(plan: &DeliveryPlan) -> Option<String> {
    let cross = plan.cross_owner_verification.as_ref()?;
    let mut source = json!({"repository":cross.repository,"workflowRevision":cross.workflow_revision,
        "requiredJobs":cross.required_jobs,"aikit":cross.aikit,"factory":cross.factory});
    if let Some(workcell) = &cross.workcell {
        source["workcell"] = json!(workcell);
    }
    Some(format!(
        "cross-owner-source-fingerprint:{}",
        blake3::hash(&serde_json::to_vec(&source).ok()?).to_hex()
    ))
}
fn material_evidence_current(evidence: &BTreeSet<String>, plan: &DeliveryPlan) -> bool {
    evidence.contains(&format!(
        "candidate-fingerprint:{}",
        plan.candidate_fingerprint
    )) && cross_owner_material_ref(plan).is_none_or(|reference| evidence.contains(&reference))
}
fn validate_cross_transition(
    before: &DeliveryPlan,
    selected: &DeliveryPlan,
) -> Result<(), FactoryAttemptError> {
    let Some(prior) = &before.cross_owner_verification else {
        return Ok(());
    };
    let Some(next) = &selected.cross_owner_verification else {
        return Err(invalid(
            "cross-owner verification obligation cannot be removed",
        ));
    };
    if prior.repository != next.repository || prior.required_jobs != next.required_jobs {
        return Err(invalid(
            "cross-owner repository and required jobs are fixed",
        ));
    }
    if prior.workcell.is_some() && next.workcell.is_none() {
        return Err(invalid(
            "selected Workcell verification obligation cannot be removed",
        ));
    }
    let changed = cross_owner_material_ref(before) != cross_owner_material_ref(selected);
    if changed && prior.run_id.is_some() && next.run_id == prior.run_id {
        return Err(invalid(
            "changed candidate requires a new cross-owner workflow run proof",
        ));
    }
    // Rebinding unchanged source material is admitted only by the locked native
    // Retry/repair guard below. Structural validation alone cannot authorise it.
    Ok(())
}
fn validate_plan(plan: &DeliveryPlan) -> Result<(), FactoryAttemptError> {
    for (name, value) in [
        ("intent", &plan.intent_ref),
        ("fingerprint", &plan.candidate_fingerprint),
        ("subject", &plan.subject_ref),
        ("subject revision", &plan.subject_revision),
        ("verification plan", &plan.verification_plan_ref),
        (
            "verification plan revision",
            &plan.verification_plan_revision,
        ),
        ("publisher", &plan.publisher_ref),
        ("branch", &plan.branch),
    ] {
        text(value, name)?;
    }
    if !sha(&plan.base_revision)
        || !sha(&plan.candidate_revision)
        || plan.expected_remote_head.as_ref().is_some_and(|v| !sha(v))
        || plan.repository.split('/').count() != 2
        || !plan
            .repository
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
        || plan.branch.starts_with('-')
        || plan.branch.contains("..")
        || plan.branch.contains('@')
        || !plan
            .branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
        || plan.branch == "main"
        || plan.branch == "master"
        || plan.pull_request == Some(0)
        || plan.required_checks.is_empty()
        || plan.required_checks.len() > MAX_CHECKS
        || plan.source_basis_refs.is_empty()
        || plan.source_basis_refs.len() > 128
        || plan.maximum_repairs > 10
        || plan.maximum_environment_retries > 3
        || !(1..=3_600_000).contains(&plan.timeout_ms)
        || !plan.revalidate_repairs
    {
        return Err(invalid("delivery needs exact material, bounded plan/checks, task branch, one publisher and ci.revalidate_repairs=true"));
    }
    validate_cross_owner(plan)?;
    for reference in &plan.source_basis_refs {
        text(reference, "source basis")?;
    }
    for check in &plan.required_checks {
        text(check, "required check")?;
    }
    Ok(())
}

/// Enrolled verification is exact; legacy attempts retain their original law.
pub(crate) fn verification_current(record: &FactoryAttemptRecord) -> bool {
    match plan(record) {
        Ok(None) => true,
        Ok(Some(plan)) => verification_material_current(record, &plan),
        Err(_) => false,
    }
}
fn verification_material_current(record: &FactoryAttemptRecord, plan: &DeliveryPlan) -> bool {
    record.verifications.last().is_some_and(|v| {
        v.source_revision == plan.candidate_revision
            && material_evidence_current(&v.evidence_refs, plan)
            && v.evidence_refs.contains(&format!(
                "verification-plan:{}@{}",
                plan.verification_plan_ref, plan.verification_plan_revision
            ))
    })
}

pub(crate) fn return_material_current(
    record: &FactoryAttemptRecord,
    artifact: &crate::orchestration::ReturnedArtifact,
) -> bool {
    match plan(record) {
        Ok(None) => true,
        Ok(Some(selected)) => {
            artifact.subject_ref == selected.subject_ref
                && artifact.subject_revision == selected.subject_revision
                && material_evidence_current(&artifact.evidence_refs, &selected)
        }
        Err(_) => false,
    }
}

pub(crate) fn inherited_selection(
    prior: Option<&FactoryAttemptRecord>,
) -> Vec<OwnerOperationReceipt> {
    prior
        .and_then(|record| {
            record
                .observations
                .iter()
                .rev()
                .find(|r| r.contract == DELIVERY_OBSERVATION && r.payload["kind"] == "selection")
        })
        .cloned()
        .into_iter()
        .collect()
}

fn selected_failed_checks<'a>(
    selected: &DeliveryPlan,
    ci: &'a HostedObservation,
) -> Vec<&'a HostedCheck> {
    let mut checks = ci
        .checks
        .iter()
        .filter(|c| {
            selected.required_checks.contains(&c.name)
                && c.head_revision == selected.candidate_revision
        })
        .collect::<Vec<_>>();
    if let Some(basis) = &selected.cross_owner_verification {
        if let Some(proof) = &ci.cross_owner {
            if settled_cross_failure(basis, proof) {
                checks.extend(proof.checks.iter().filter(|c| {
                    basis.required_jobs.contains(&c.name)
                        && c.head_revision == basis.workflow_revision
                        && Some(c.run_id.as_str()) == basis.run_id.map(|v| v.to_string()).as_deref()
                        && Some(c.run_attempt) == basis.run_attempt
                }));
            }
        }
    }
    checks
        .into_iter()
        .filter(|c| {
            c.status == "completed"
                && matches!(c.conclusion.as_deref(), Some("failure" | "timed_out"))
        })
        .collect()
}
pub(crate) fn failed_ci(record: &FactoryAttemptRecord) -> bool {
    match (plan(record), last_ci(record)) {
        (Ok(Some(selected)), Ok(Some((_, ci)))) => {
            ci.enumerated
                && ci.error.is_none()
                && ci.candidate_revision == selected.candidate_revision
                && ci.remote_head == selected.candidate_revision
                && !selected_failed_checks(&selected, &ci).is_empty()
        }
        _ => false,
    }
}

pub(crate) fn validate_publisher(
    reading: &FactoryAttemptReading,
    request: &FactoryAttemptActionRequest,
) -> Result<(), FactoryAttemptError> {
    if let FactoryAttemptOperation::RecordObservation {
        attempt_ref,
        receipt,
    } = &request.operation
    {
        if receipt.contract == DELIVERY_OBSERVATION
            && receipt.payload["kind"] == "publication-intent"
        {
            let record = current(reading, attempt_ref)?;
            let selected = plan(record)?.ok_or_else(|| invalid("publication plan absent"))?;
            if request.caller.caller_ref != selected.publisher_ref {
                return Err(invalid(
                    "publication Action caller is not admitted publisher",
                ));
            }
        }
    }
    Ok(())
}

/// GitHub runs no `pull_request` workflow on a PR that conflicts with its base,
/// so a conflicted PR's required checks never appear and delivery would wait on
/// "required check missing" for ever (no-mistakes treats a conflict as an issue
/// of its own). Only a definite answer counts: `mergeable` is null while GitHub
/// is still computing it.
fn pull_request_conflict(pr: &Value) -> Option<String> {
    let state = pr["mergeable_state"].as_str();
    (pr["mergeable"].as_bool() == Some(false) || state == Some("dirty")).then(|| {
        format!(
            "PR conflicts with its base ({}): GitHub runs no workflows on a conflicted PR; \
             merge the base into the task branch and publish again",
            state.unwrap_or("not mergeable")
        )
    })
}

fn check_failures(plan: &DeliveryPlan, observation: &HostedObservation) -> Vec<String> {
    let mut failures = Vec::new();
    // The reader's own refusal names why the enumeration stopped (a PR in
    // conflict with its base, a moved remote), so it leads and becomes the
    // next action, ahead of the generic incomplete-enumeration failure.
    if let Some(error) = &observation.error {
        failures.push(error.clone());
    }
    if !observation.enumerated {
        failures.push("hosted check enumeration incomplete".into());
    }
    if observation.candidate_revision != plan.candidate_revision
        || observation.remote_head != plan.candidate_revision
    {
        failures.push("hosted/remote head differs from selected candidate".into());
    }
    // Checks that were never enumerated are unknown, not missing: the
    // refusal above already says why the reader stopped.
    for name in plan
        .required_checks
        .iter()
        .filter(|_| observation.enumerated)
    {
        let checks = observation
            .checks
            .iter()
            .filter(|c| c.name == *name)
            .collect::<Vec<_>>();
        if checks.is_empty() {
            failures.push(format!("required check missing: {name}"));
            continue;
        }
        // The newest admitted run/attempt is selected per workflow identity by
        // the GitHub reader; ambiguity supplied by a foreign producer fails.
        if checks.len() != 1 {
            failures.push(format!("required check ambiguous: {name}"));
            continue;
        }
        let check = checks[0];
        if check.head_revision != plan.candidate_revision
            || check.run_id.is_empty()
            || check.run_attempt == 0
        {
            failures.push(format!("required check stale/unattributed: {name}"));
        } else if check.status != "completed" || check.conclusion.as_deref() != Some("success") {
            failures.push(format!(
                "required check unresolved: {name} ({}/{})",
                check.status,
                check.conclusion.as_deref().unwrap_or("pending")
            ));
        }
    }
    if let Some(cross) = &plan.cross_owner_verification {
        match &observation.cross_owner {
            None => failures.push("cross-owner verification not observed".into()),
            Some(proof) => failures.extend(cross_owner_failures(cross, proof)),
        }
    }
    failures
}

/// Delivery observations are validated even when submitted via the underlying
/// native RecordObservation Action. The canonical transaction calls this hook.
pub(crate) fn validate_observation(
    record: &FactoryAttemptRecord,
    receipt: &OwnerOperationReceipt,
) -> Result<(), FactoryAttemptError> {
    if receipt.contract != DELIVERY_OBSERVATION {
        return Ok(());
    }
    if receipt.owner_ref != "factory/delivery" {
        return Err(invalid("delivery owner attribution changed"));
    }
    match receipt.payload["kind"].as_str() {
        Some("selection") => {
            let selected: DeliveryPlan = serde_json::from_value(receipt.payload["plan"].clone())?;
            validate_plan(&selected)?;
            if let Some(before) = plan(record)? {
                validate_cross_transition(&before, &selected)?;
                if unresolved_publication(record)
                    && (before.candidate_revision != selected.candidate_revision
                        || before.candidate_fingerprint != selected.candidate_fingerprint)
                {
                    return Err(invalid(
                        "reconcile exact retained publication before selecting another candidate",
                    ));
                }
                if before.intent_ref != selected.intent_ref
                    || before.base_revision != selected.base_revision
                    || before
                        .pull_request
                        .is_some_and(|n| Some(n) != selected.pull_request)
                    || before.subject_ref != selected.subject_ref
                    || before.repository != selected.repository
                    || before.branch != selected.branch
                    || before.publisher_ref != selected.publisher_ref
                    || before.required_checks != selected.required_checks
                    || before.verification_plan_ref != selected.verification_plan_ref
                    || before.verification_plan_revision != selected.verification_plan_revision
                    || before.maximum_repairs != selected.maximum_repairs
                    || before.maximum_environment_retries != selected.maximum_environment_retries
                    || before.timeout_ms != selected.timeout_ms
                    || before.source_basis_refs != selected.source_basis_refs
                {
                    return Err(invalid("repair cannot widen or replace original delivery intent/authority/bounds/verification basis"));
                }
            }
        }
        Some("hosted-ci") => {
            let observed: HostedObservation =
                serde_json::from_value(receipt.payload["observation"].clone())?;
            if observed.checks.len() > MAX_CHECKS
                || observed.evidence_refs.is_empty()
                || observed.cross_owner.as_ref().is_some_and(|p| {
                    p.checks.len() > MAX_CHECKS
                        || p.evidence_refs.is_empty()
                        || p.source_cut.to_string().len() > MAX_LOG_BYTES
                })
            {
                return Err(invalid(
                    "hosted CI needs bounded checks and attributable read evidence",
                ));
            }
            for check in &observed.checks {
                text(&check.name, "check name")?;
                text(&check.evidence_ref, "check evidence")?;
            }
            if plan(record)?.is_none() {
                return Err(invalid("select delivery plan before observing CI"));
            }
        }
        Some("repair") => {
            let selected = plan(record)?.ok_or_else(|| invalid("repair needs selected plan"))?;
            let (ci_receipt, ci) =
                last_ci(record)?.ok_or_else(|| invalid("repair needs retained hosted failure"))?;
            if receipt.payload["observationRef"] != ci_receipt.receipt_ref
                || ci.candidate_revision != selected.candidate_revision
                || ci.remote_head != selected.candidate_revision
                || !ci.enumerated
                || ci.error.is_some()
                || selected_failed_checks(&selected, &ci).is_empty()
            {
                return Err(invalid("repair must select this exact settled failed check/run/attempt; stale, unreadable and pending observations need reconciliation"));
            }
            let class = receipt.payload["failureClass"].as_str().unwrap_or("");
            if class != "code" && class != "environment" {
                return Err(invalid(
                    "repair class must be code or bounded environment retry",
                ));
            }
            let prior = record
                .observations
                .iter()
                .filter(|r| r.contract == DELIVERY_OBSERVATION && r.payload["kind"] == "repair");
            if prior
                .clone()
                .any(|r| r.payload["observationRef"] == ci_receipt.receipt_ref)
            {
                return Err(invalid("this failure already has a retained repair; recover it rather than execute again"));
            }
            let identities = failed_identities(&selected, &ci);
            if ci_receipt.payload["standing"] == "actual-github-read" {
                let fresh: HostedObservation =
                    serde_json::from_value(receipt.payload["freshRead"].clone())?;
                if !fresh.enumerated
                    || fresh.candidate_fingerprint.as_deref()
                        != Some(&selected.candidate_fingerprint)
                    || fresh.error.is_some()
                    || fresh.candidate_revision != selected.candidate_revision
                    || fresh.remote_head != selected.candidate_revision
                    || failed_identities(&selected, &fresh) != identities
                {
                    return Err(invalid(
                        "repair requires fresh exact hosted run/attempt failure",
                    ));
                }
            }
            if receipt.payload["failureIdentities"] != identities {
                return Err(invalid(
                    "repair identities must match exact retained failed check/run/attempt",
                ));
            }
            if record
                .observations
                .iter()
                .filter(|r| r.contract == DELIVERY_OBSERVATION && r.payload["kind"] == "repair")
                .any(|r| r.payload["failureIdentities"] == identities)
            {
                return Err(invalid("same hosted run/attempt failure already has a retained repair; a new polling receipt cannot repeat execution"));
            }
            let count = prior.filter(|r| r.payload["failureClass"] == class).count();
            let bound = if class == "code" {
                selected.maximum_repairs
            } else {
                selected.maximum_environment_retries
            };
            if count >= bound as usize {
                return Err(invalid("finite delivery repair/retry allowance exhausted"));
            }
            if receipt.payload["failureLogs"]
                .as_str()
                .is_none_or(|v| v.is_empty() || v.len() > MAX_LOG_BYTES)
                || receipt.evidence_refs.is_empty()
            {
                return Err(invalid(
                    "repair needs bounded failure logs and exact artifact references",
                ));
            }
        }
        Some("publication-intent") => {
            if plan(record)?.is_none() {
                return Err(invalid("publication needs plan"));
            }
            if unresolved_publication(record) {
                return Err(invalid(
                    "uncertain publication must be reconciled before another remote effect",
                ));
            }
        }
        Some("publication-readback") => {
            let intent = record
                .observations
                .iter()
                .find(|r| {
                    r.contract == DELIVERY_OBSERVATION
                        && r.payload["kind"] == "publication-intent"
                        && receipt.payload["publicationRef"] == r.receipt_ref
                })
                .ok_or_else(|| invalid("publication readback needs exact retained intent"))?;
            if receipt.operation_ref != intent.operation_ref {
                return Err(invalid(
                    "publication reconciliation changed operation identity",
                ));
            }
            if receipt.phase == OwnerOperationPhase::Observed
                && (receipt.payload["remoteHead"] != intent.payload["candidateRevision"]
                    || receipt.payload["candidateFingerprint"]
                        != intent.payload["candidateFingerprint"]
                    || receipt.evidence_refs.is_empty()
                    || receipt.partial_effect_refs != receipt.evidence_refs)
            {
                return Err(invalid(
                    "only exact published candidate readback settles publication",
                ));
            }
        }
        _ => return Err(invalid("unknown delivery observation kind")),
    }
    Ok(())
}

fn failed_identities(selected: &DeliveryPlan, ci: &HostedObservation) -> Value {
    let mut ids = selected_failed_checks(selected, ci)
        .into_iter()
        .map(|c| {
            (
                c.name.clone(),
                c.head_revision.clone(),
                c.run_id.clone(),
                c.run_attempt,
            )
        })
        .collect::<Vec<_>>();
    ids.sort();
    json!(ids)
}

pub(crate) fn unresolved_task_publication(state: &StoredAttemptState, task_ref: &str) -> bool {
    state
        .attempts
        .values()
        .filter(|a| a.task_ref == task_ref)
        .any(unresolved_publication)
}

/// Locked native admission prevents the lower-level Action from bypassing
/// selected-material review or replenishing delivery budgets across retries.
fn attempt_start_revision(state: &StoredAttemptState, attempt: &str) -> Option<u64> {
    state
        .action_receipts
        .values()
        .filter(|action| match &action.request.operation {
            FactoryAttemptOperation::StartSerial { attempt_ref, .. }
            | FactoryAttemptOperation::Retry { attempt_ref, .. } => attempt_ref == attempt,
            FactoryAttemptOperation::StartFork { attempts, .. } => {
                attempts.iter().any(|a| a.attempt_ref == attempt)
            }
            _ => false,
        })
        .map(|action| action.receipt.next_revision)
        .min()
}
fn validate_cross_renewal(
    state: &StoredAttemptState,
    record: &FactoryAttemptRecord,
    before: &DeliveryPlan,
    selected: &DeliveryPlan,
) -> Result<(), FactoryAttemptError> {
    let current = plan(record)?;
    if current.as_ref() == Some(selected)
        || current.as_ref().is_some_and(|current| {
            cross_owner_material_ref(current) == cross_owner_material_ref(selected)
                && current
                    .cross_owner_verification
                    .as_ref()
                    .is_some_and(|basis| basis.run_id.is_none())
        })
    {
        return Ok(());
    }
    let Some(prior) = &before.cross_owner_verification else {
        return Ok(());
    };
    let source_changed = cross_owner_material_ref(before) != cross_owner_material_ref(selected);
    let binding_changed = selected
        .cross_owner_verification
        .as_ref()
        .is_some_and(|next| {
            prior.run_id.is_some()
                && (prior.run_id != next.run_id || prior.run_attempt != next.run_attempt)
        });
    if !source_changed && !binding_changed {
        return Ok(());
    }
    let retry = state.action_receipts.values().find(|action| matches!(
        &action.request.operation, FactoryAttemptOperation::Retry { attempt_ref, .. }
            if attempt_ref == &record.attempt_ref))
        .ok_or_else(|| invalid("cross-owner source correction needs actual native Retry, not an ordinary selection edit"))?;
    let resolution = match &retry.request.operation {
        FactoryAttemptOperation::Retry {
            reresolution: Some(resolution),
            ..
        } => resolution,
        _ => {
            return Err(invalid(
                "cross-owner correction requires selected repair re-resolution at native Retry",
            ))
        }
    };
    // Persisted native Actions identify the immediate predecessor. A historic
    // failure from an earlier retry cannot be recycled to authorise new effects.
    let predecessor = state
        .attempts
        .values()
        .filter(|a| {
            a.task_ref == record.task_ref
                && a.workflow_unit_ref == record.workflow_unit_ref
                && a.attempt_ref != record.attempt_ref
        })
        .filter_map(|a| attempt_start_revision(state, &a.attempt_ref).map(|revision| (revision, a)))
        .filter(|(revision, _)| *revision < retry.receipt.next_revision)
        .max_by_key(|(revision, _)| *revision)
        .map(|(_, a)| a)
        .ok_or_else(|| invalid("cross-owner Retry predecessor is not retained"))?;
    if record.observations.iter().any(|receipt| {
        receipt.contract == DELIVERY_OBSERVATION
            && receipt.payload["kind"] == "selection"
            && !predecessor
                .observations
                .iter()
                .any(|inherited| inherited == receipt)
    }) {
        return Err(invalid("cross-owner run/source renewal is one-way within a native Retry; another renewal needs its own retained failure and Retry"));
    }
    let previous =
        plan(predecessor)?.ok_or_else(|| invalid("cross-owner Retry predecessor plan absent"))?;
    let (ci_receipt, ci) = last_ci(predecessor)?
        .ok_or_else(|| invalid("cross-owner Retry requires retained failed hosted proof"))?;
    let basis = previous
        .cross_owner_verification
        .as_ref()
        .ok_or_else(|| invalid("cross-owner prior source basis absent"))?;
    let proof = ci
        .cross_owner
        .as_ref()
        .ok_or_else(|| invalid("cross-owner prior source proof absent"))?;
    if !failed_ci(predecessor)
        || !(settled_cross_failure(basis, proof) || cross_owner_failures(basis, proof).is_empty())
    {
        return Err(invalid("cross-owner correction requires exact settled failed source proof; pending, stale and unreadable proof cannot authorise renewal"));
    }
    // Classify against the immediate predecessor, even when the immutable
    // bounds are checked against older attempts too. An environment retry may
    // renew a run/attempt only for the identical complete source fingerprint.
    let changed_material =
        cross_owner_material_ref(&previous) != cross_owner_material_ref(selected);
    if !changed_material
        && selected
            .cross_owner_verification
            .as_ref()
            .is_none_or(|next| {
                next.run_id.is_none()
                    || (next.run_id == basis.run_id && next.run_attempt == basis.run_attempt)
            })
    {
        return Err(invalid("environment Retry requires a fresh exact workflow run/attempt for identical selected source material"));
    }
    let failure_class = if changed_material {
        "code"
    } else {
        "environment"
    };
    let repair = predecessor
        .observations
        .iter()
        .rev()
        .find(|receipt| {
            receipt.contract == DELIVERY_OBSERVATION
                && receipt.payload["kind"] == "repair"
                && receipt.payload["observationRef"] == ci_receipt.receipt_ref
                && receipt.payload["failureClass"] == failure_class
        })
        .ok_or_else(|| {
            invalid("cross-owner renewal requires an admitted bounded repair of its exact class: code for changed material, environment for identical material")
        })?;
    let mut required = predecessor
        .observations
        .iter()
        .flat_map(|r| r.partial_effect_refs.iter().cloned())
        .collect::<BTreeSet<_>>();
    required.extend(predecessor.failure_evidence_refs.iter().cloned());
    required.extend([
        repair.receipt_ref.clone(),
        ci_receipt.receipt_ref.clone(),
        cross_owner_material_ref(selected).unwrap(),
    ]);
    if !resolution.evidence_refs.is_superset(&required) {
        return Err(invalid("cross-owner Retry re-resolution must retain exact failed proof, repair, settled effects and successor source fingerprint"));
    }
    Ok(())
}

pub(crate) fn validate_native_observation(
    state: &StoredAttemptState,
    engine: &crate::orchestration::ExecutableOrchestration,
    attempt_ref: &str,
    receipt: &OwnerOperationReceipt,
) -> Result<(), FactoryAttemptError> {
    if receipt.contract != DELIVERY_OBSERVATION {
        return Ok(());
    }
    let record = state
        .attempts
        .get(attempt_ref)
        .ok_or_else(|| invalid("delivery attempt absent"))?;
    validate_observation(record, receipt)?;
    if receipt.payload["kind"] == "selection" {
        let selected: DeliveryPlan = serde_json::from_value(receipt.payload["plan"].clone())?;
        if let Some(before) = plan(record)? {
            validate_cross_renewal(state, record, &before, &selected)?;
        }
        for retained in state
            .attempts
            .values()
            .filter(|a| a.task_ref == record.task_ref && unresolved_publication(a))
        {
            let before =
                plan(retained)?.ok_or_else(|| invalid("retained publication plan absent"))?;
            if retained.attempt_ref != record.attempt_ref
                || before.candidate_revision != selected.candidate_revision
                || before.candidate_fingerprint != selected.candidate_fingerprint
            {
                return Err(invalid("reconcile exact retained same-task publication before selecting another candidate or attempt"));
            }
        }
        if engine.current_subject_revision(&selected.subject_ref)
            != Some(selected.subject_revision.as_str())
        {
            return Err(invalid(
                "delivery selection is stale against actual native material subject",
            ));
        }
        for prior in state
            .attempts
            .values()
            .filter(|r| r.task_ref == record.task_ref && r.attempt_ref != record.attempt_ref)
        {
            if let Some(before) = plan(prior)? {
                validate_cross_transition(&before, &selected)?;
                validate_cross_renewal(state, record, &before, &selected)?;
                let mut proposed = selected.clone();
                if before.pull_request.is_none() {
                    proposed.pull_request = None;
                }
                proposed.candidate_revision = before.candidate_revision.clone();
                proposed.candidate_fingerprint = before.candidate_fingerprint.clone();
                proposed.subject_revision = before.subject_revision.clone();
                proposed.expected_remote_head = before.expected_remote_head.clone();
                proposed.cross_owner_verification = before.cross_owner_verification.clone();
                if proposed != before {
                    return Err(invalid(
                        "retry cannot replace prior native delivery plan or replenish bounds",
                    ));
                }
            }
        }
    }
    if receipt.payload["kind"] == "repair" {
        let selected = plan(record)?.ok_or_else(|| invalid("repair plan absent"))?;
        let class = receipt.payload["failureClass"].as_str().unwrap_or("");
        if state
            .attempts
            .values()
            .filter(|r| r.task_ref == record.task_ref)
            .flat_map(|r| &r.observations)
            .any(|r| {
                r.contract == DELIVERY_OBSERVATION
                    && r.payload["kind"] == "repair"
                    && r.payload["failureIdentities"] == receipt.payload["failureIdentities"]
            })
        {
            return Err(invalid(
                "same hosted run/attempt already has repair in retained native history",
            ));
        }
        let before = state
            .attempts
            .values()
            .filter(|r| r.task_ref == record.task_ref)
            .flat_map(|r| &r.observations)
            .filter(|r| {
                r.contract == DELIVERY_OBSERVATION
                    && r.payload["kind"] == "repair"
                    && r.payload["failureClass"] == class
            })
            .count();
        let bound = if class == "code" {
            selected.maximum_repairs
        } else {
            selected.maximum_environment_retries
        };
        if before >= bound as usize {
            return Err(invalid(
                "total delivery repair allowance exhausted across retained attempts",
            ));
        }
    }
    if receipt.payload["kind"] == "publication-intent" {
        if unresolved_task_publication(state, &record.task_ref) {
            return Err(invalid(
                "uncertain same-task publication must be reconciled before another remote effect",
            ));
        }
        let selected = plan(record)?.ok_or_else(|| invalid("publication plan absent"))?;
        candidate_tree(&selected).map_err(invalid)?;
        let reviewer_ref = receipt.payload["reviewerAttemptRef"]
            .as_str()
            .ok_or_else(|| invalid("publication reviewer absent"))?;
        let reviewer = crate::attempt_review::settled(state, engine, reviewer_ref)?;
        crate::attempt_review::settled(state, engine, attempt_ref)?;
        if !verification_material_current(reviewer, &selected)
            || receipt.payload["publisherRef"] != selected.publisher_ref
            || receipt.payload["repository"] != selected.repository
            || receipt.payload["branch"] != selected.branch
            || receipt.payload["candidateRevision"] != selected.candidate_revision
            || receipt.payload["candidateFingerprint"] != selected.candidate_fingerprint
            || receipt.payload["expectedRemoteHead"]
                != serde_json::to_value(&selected.expected_remote_head)?
            || reviewer.disposition.participant.agent_ref
                == record.disposition.participant.agent_ref
            || reviewer.disposition.participant.agency_ref
                == record.disposition.participant.agency_ref
            || reviewer.disposition.body.agent_session_ref
                == record.disposition.body.agent_session_ref
            || !engine
                .independent_reviewers()
                .get(reviewer.execution_ref.as_deref().unwrap_or(""))
                .is_some_and(|r| r.review_of.contains(&record.workflow_unit_ref))
            || !reviewer.disposition.selected_inputs.iter().any(|i| {
                i.predecessor == record.workflow_unit_ref
                    && i.execution_ref == record.execution_ref.as_deref().unwrap_or("")
                    && i.artifacts.iter().any(|a| {
                        a.subject_ref == selected.subject_ref
                            && a.subject_revision == selected.subject_revision
                            && material_evidence_current(&a.evidence_refs, &selected)
                    })
            })
        {
            return Err(invalid("publication intent lacks actual exact-material independent review or changes admitted target"));
        }
    }
    Ok(())
}

/// Retained-history projection. Current human and agent decisions use
/// read_current_readiness, which rereads immutable material and hosted results.
pub fn readiness(
    reading: &FactoryAttemptReading,
    attempt_ref: &str,
) -> Result<DeliveryReading, FactoryAttemptError> {
    let record = current(reading, attempt_ref)?;
    let selected = plan(record)?;
    let mut result = DeliveryReading {
        contract: DELIVERY_READING,
        run_ref: reading.run_ref.clone(),
        attempt_ref: attempt_ref.into(),
        revision: reading.revision,
        candidate_revision: selected.as_ref().map(|p| p.candidate_revision.clone()),
        candidate_fingerprint: selected.as_ref().map(|p| p.candidate_fingerprint.clone()),
        required_checks: selected
            .as_ref()
            .map(|p| p.required_checks.clone())
            .unwrap_or_default(),
        cross_owner_verification: selected
            .as_ref()
            .and_then(|p| p.cross_owner_verification.clone()),
        checks: vec![],
        cross_owner: None,
        review_evidence_refs: BTreeSet::new(),
        publication_evidence_refs: BTreeSet::new(),
        failures: vec![],
        failure_evidence_refs: reading
            .attempts
            .iter()
            .filter(|a| a.task_ref == record.task_ref)
            .flat_map(|a| a.failure_evidence_refs.iter().cloned())
            .collect(),
        repair_evidence_refs: reading
            .attempts
            .iter()
            .filter(|a| a.task_ref == record.task_ref)
            .flat_map(|a| &a.observations)
            .filter(|r| r.contract == DELIVERY_OBSERVATION && r.payload["kind"] == "repair")
            .map(|r| r.receipt_ref.clone())
            .collect(),
        repairs: reading
            .attempts
            .iter()
            .filter(|a| a.task_ref == record.task_ref)
            .flat_map(|a| &a.observations)
            .filter(|r| {
                r.contract == DELIVERY_OBSERVATION
                    && r.payload["kind"] == "repair"
                    && r.payload["failureClass"] == "code"
            })
            .count(),
        environment_retries: reading
            .attempts
            .iter()
            .filter(|a| a.task_ref == record.task_ref)
            .flat_map(|a| &a.observations)
            .filter(|r| {
                r.contract == DELIVERY_OBSERVATION
                    && r.payload["kind"] == "repair"
                    && r.payload["failureClass"] == "environment"
            })
            .count(),
        ready: false,
        next_action: "select exact delivery plan".into(),
    };
    let Some(selected) = selected else {
        return Ok(result);
    };
    if let Err(error) = candidate_tree(&selected) {
        result.failures.push(error);
    }
    if !crate::attempt_runtime::has_passing_verification(record) {
        result
            .failures
            .push("candidate verification missing or stale".into());
    }
    if !reading
        .current_returned_units
        .contains(&record.workflow_unit_ref)
        || !reading
            .legs
            .get(&record.workflow_unit_ref)
            .is_some_and(|leg| {
                leg.execution_ref == record.execution_ref.as_deref().unwrap_or("")
                    && leg
                        .artifacts
                        .iter()
                        .any(|a| return_material_current(record, a))
            })
    {
        result
            .failures
            .push("native candidate Return is not current".into());
    }
    if let Some((receipt, ci)) = last_ci(record)? {
        result.failures.extend(check_failures(&selected, &ci));
        if receipt.payload["standing"] == "actual-github-read"
            && ci.candidate_fingerprint.as_deref() != Some(&selected.candidate_fingerprint)
        {
            result
                .failures
                .push("hosted immutable candidate fingerprint missing or stale".into());
        }
        result.cross_owner = ci.cross_owner.clone();
        result.checks = ci.checks;
        if receipt.payload["standing"] != "actual-github-read" {
            result.failures.push("hosted evidence is controlled-backend coverage; actual GitHub read remains required".into());
        }
    } else {
        result.failures.push("hosted CI not observed".into());
    }
    for receipt in &record.observations {
        if receipt.contract != DELIVERY_OBSERVATION {
            continue;
        }
        if receipt.payload["kind"] == "publication-readback"
            && receipt.phase == OwnerOperationPhase::Observed
            && receipt.payload["remoteHead"] == selected.candidate_revision
            && receipt.payload["candidateFingerprint"] == selected.candidate_fingerprint
        {
            result
                .publication_evidence_refs
                .extend(receipt.evidence_refs.clone());
        }
        if receipt.payload["kind"] == "publication-intent"
            && receipt.payload["candidateRevision"] == selected.candidate_revision
            && receipt.payload["candidateFingerprint"] == selected.candidate_fingerprint
        {
            let reviewer = receipt.payload["reviewerAttemptRef"].as_str().unwrap_or("");
            if let Some(review) = reading.attempts.iter().find(|r| r.attempt_ref == reviewer) {
                if reading
                    .independent_reviewers
                    .get(review.execution_ref.as_deref().unwrap_or(""))
                    .is_some_and(|r| r.review_of.contains(&record.workflow_unit_ref))
                    && reading
                        .current_returned_units
                        .contains(&review.workflow_unit_ref)
                    && crate::attempt_runtime::has_passing_verification(review)
                    && verification_material_current(review, &selected)
                    && review.disposition.selected_inputs.iter().any(|i| {
                        i.predecessor == record.workflow_unit_ref
                            && i.execution_ref == record.execution_ref.as_deref().unwrap_or("")
                            && i.artifacts.iter().any(|a| {
                                a.subject_ref == selected.subject_ref
                                    && a.subject_revision == selected.subject_revision
                                    && material_evidence_current(&a.evidence_refs, &selected)
                            })
                    })
                {
                    result.review_evidence_refs.extend(
                        review
                            .readable_return
                            .iter()
                            .flat_map(|r| r.evidence_refs.clone()),
                    );
                }
            }
        }
    }
    if result.review_evidence_refs.is_empty() {
        result
            .failures
            .push("distinct selected-material review missing or stale".into());
    }
    if result.publication_evidence_refs.is_empty() {
        result
            .failures
            .push("exact publication readback missing".into());
    }
    result.ready = result.failures.is_empty();
    result.next_action = if result.ready {
        "return exact verified candidate".into()
    } else {
        result
            .failures
            .first()
            .cloned()
            .unwrap_or_else(|| "reconcile delivery".into())
    };
    Ok(result)
}

/// A readiness decision rereads the addressed forge. Historical green evidence
/// remains accessible in the attempt, but cannot establish today's remote head.
fn readiness_live(
    store: &FileAttemptStore,
    reading: &FactoryAttemptReading,
    attempt_ref: &str,
) -> Result<DeliveryReading, FactoryAttemptError> {
    let record = current(reading, attempt_ref)?;
    let selected = plan(record)?;
    let binary = record
        .observations
        .iter()
        .rev()
        .filter(|r| r.contract == DELIVERY_OBSERVATION)
        .find_map(|r| r.payload["request"]["gh_binary"].as_str());
    let Some((selected, binary)) = selected.as_ref().zip(binary) else {
        return readiness(reading, attempt_ref);
    };
    let ci = observe_github(binary, selected);
    let mut fresh = reading.clone();
    let record = fresh
        .attempts
        .iter_mut()
        .find(|r| r.attempt_ref == attempt_ref)
        .unwrap();
    record.observations.push(OwnerOperationReceipt {
        owner_ref: "factory/delivery".into(),
        contract: DELIVERY_OBSERVATION.into(),
        operation_ref: "delivery:current-readiness".into(),
        receipt_ref: "delivery:current-readiness".into(),
        source_revision: selected.candidate_revision.clone(),
        phase: OwnerOperationPhase::Observed,
        evidence_refs: ci.evidence_refs.clone(),
        partial_effect_refs: BTreeSet::new(),
        payload: json!({"kind":"hosted-ci","observation":ci,"standing":"actual-github-read"}),
    });
    let mut result = readiness(&fresh, attempt_ref)?;
    if store.reading()?.revision != reading.revision {
        result.ready = false;
        result
            .failures
            .push("native material changed during readiness read; reopen current basis".into());
        result.next_action = "reopen current native material".into();
    }
    Ok(result)
}

/// Current production readiness for SDK consumers uses the same bounded owner
/// reads as the human/JSON front door, never a historical green snapshot.
pub fn read_current_readiness(
    path: &Path,
    run_ref: RunRef,
    attempt_ref: &str,
) -> Result<DeliveryReading, FactoryAttemptError> {
    let store = FileAttemptStore::open_run(path, run_ref)?;
    readiness_live(&store, &store.reading()?, attempt_ref)
}

fn gh(binary: &str, endpoint: String, deadline: Instant) -> Result<Value, String> {
    if !Path::new(binary).is_absolute() {
        return Err("GitHub reader requires explicit absolute gh executable".into());
    }
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or("delivery hosted-read total wall budget exhausted")?;
    crate::sensing_sources::owner_json_with_timeout(
        binary,
        &["api".into(), endpoint],
        None,
        remaining.min(Duration::from_secs(60)),
    )
}
fn candidate_tree(plan: &DeliveryPlan) -> Result<&str, String> {
    plan.candidate_fingerprint
        .strip_prefix("git-tree:")
        .filter(|v| sha(v))
        .ok_or_else(|| {
            "production publication/readiness requires observable exact git-tree fingerprint".into()
        })
}
fn observe_commit_tree(
    binary: &str,
    plan: &DeliveryPlan,
    deadline: Instant,
) -> Result<String, String> {
    let tree = candidate_tree(plan)?;
    let commit = gh(
        binary,
        format!(
            "repos/{}/commits/{}",
            plan.repository, plan.candidate_revision
        ),
        deadline,
    )?;
    if commit["sha"].as_str() != Some(&plan.candidate_revision)
        || commit["commit"]["tree"]["sha"].as_str() != Some(tree)
    {
        return Err(
            "immutable hosted commit/tree differs from selected candidate fingerprint".into(),
        );
    }
    Ok(format!("git-tree:{tree}"))
}
fn unresolved_publication(record: &FactoryAttemptRecord) -> bool {
    record.observations.iter().any(|intent| {
        intent.contract == DELIVERY_OBSERVATION
            && intent.payload["kind"] == "publication-intent"
            && !record.observations.iter().any(|r| {
                r.contract == DELIVERY_OBSERVATION
                    && r.payload["kind"] == "publication-readback"
                    && r.payload["publicationRef"] == intent.receipt_ref
                    && r.phase == OwnerOperationPhase::Observed
            })
    })
}

fn remote_head_optional(
    binary: &str,
    selected: &DeliveryPlan,
    deadline: Instant,
) -> Result<Option<String>, String> {
    let value = gh(
        binary,
        format!(
            "repos/{}/git/matching-refs/heads/{}",
            selected.repository, selected.branch
        ),
        deadline,
    )?;
    let refs = value
        .as_array()
        .ok_or("remote reference enumeration unreadable")?;
    if refs.len() > MAX_CHECKS {
        return Err("remote reference enumeration exceeds bound".into());
    }
    let exact = refs
        .iter()
        .filter(|r| r["ref"] == format!("refs/heads/{}", selected.branch))
        .collect::<Vec<_>>();
    match exact.as_slice() {
        [] => Ok(None),
        [reference] => reference["object"]["sha"]
            .as_str()
            .filter(|v| sha(v))
            .map(|v| Some(v.to_owned()))
            .ok_or_else(|| "remote head unreadable".into()),
        _ => Err("remote reference ambiguous".into()),
    }
}
fn remote_head(binary: &str, selected: &DeliveryPlan, deadline: Instant) -> Result<String, String> {
    remote_head_optional(binary, selected, deadline)?
        .ok_or_else(|| "selected remote branch is absent".into())
}

fn marker_source_cut(logs: &str) -> Result<Value, String> {
    let mut markers = Vec::new();
    for line in logs.lines() {
        let Some((prefix, body)) = line.split_once(SOURCE_CUT_MARKER) else {
            continue;
        };
        // GitHub prints the submitted script before executing it. Only accept
        // stdout, optionally preceded by its single timestamp, never a script
        // literal containing the marker.
        let prefix = prefix.trim();
        if !prefix.is_empty()
            && !(prefix.len() >= 20
                && prefix.ends_with('Z')
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b"-:TZ.".contains(&b)))
        {
            continue;
        }
        if body.len() > MAX_LOG_BYTES {
            return Err("cross-owner source-cut marker exceeds bound".into());
        }
        markers.push(
            serde_json::from_str(body)
                .map_err(|e| format!("cross-owner source-cut JSON unreadable: {e}"))?,
        );
    }
    if markers.len() != 1 {
        return Err("cross-owner source-cut needs one executed marker; missing or ambiguous logs are not verification".into());
    }
    Ok(markers.remove(0))
}
fn exact_number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}
fn selected_sources(basis: &CrossOwnerVerification) -> impl Iterator<Item = (&str, &SourceCommit)> {
    [("ai-kit", &basis.aikit), ("factory", &basis.factory)]
        .into_iter()
        .chain(basis.workcell.as_ref().map(|source| ("workcell", source)))
}
fn workcell_stamp_matches(source: &SourceCommit, actual: &Value) -> bool {
    actual["binary_stamp_revision"] == source.revision
        && actual["source_revision"] == source.revision
        && actual["binary_version"].as_str().is_some_and(|version| {
            version.len() <= 256
                && !version.chars().any(char::is_control)
                && version.strip_prefix("workcell ").is_some_and(|rest| {
                    let Some((package, stamp)) = rest.split_once(" (") else {
                        return false;
                    };
                    !package.is_empty()
                        && !package.chars().any(char::is_whitespace)
                        && stamp == format!("{})", source.revision)
                })
        })
}
fn source_identity_matches(basis: &CrossOwnerVerification, cut: &Value) -> bool {
    cut["schema"] == "oi.development-field-cut-execution/v1"
        && cut["oi_revision"] == basis.workflow_revision
        && exact_number(&cut["run_id"]) == basis.run_id
        && exact_number(&cut["run_attempt"]) == basis.run_attempt
        && cut["provider_inference_observed"] == false
        && cut["physical_acceptance"] == false
        && cut["logs"].as_array().is_some_and(|logs| {
            !logs.is_empty()
                && logs.len() <= MAX_CHECKS
                && logs
                    .iter()
                    .all(|v| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 256))
        })
        && selected_sources(basis).all(|(owner, source)| {
            let actual = &cut["owners"][owner];
            actual["requested_revision"] == source.revision
                && actual["observed_revision"] == source.revision
                && actual["tree"] == source.tree
                && digest(&actual["lock_sha256"])
                && (actual["binary_sha256"].is_null() || digest(&actual["binary_sha256"]))
                && (owner != "workcell"
                    || ["binary_stamp_revision", "source_revision", "binary_version"]
                        .iter()
                        .all(|field| actual[*field].is_null())
                    || workcell_stamp_matches(source, actual))
        })
}
fn digest(value: &Value) -> bool {
    value.as_str().is_some_and(|v| v.len() == 64 && sha(v))
}
fn source_cut_matches(basis: &CrossOwnerVerification, cut: &Value) -> bool {
    source_identity_matches(basis, cut)
        && selected_sources(basis).all(|(owner, source)| {
            let actual = &cut["owners"][owner];
            digest(&actual["binary_sha256"])
                && (owner != "workcell" || workcell_stamp_matches(source, actual))
        })
}
/// Negative evidence is settled and source-bound even when compilation failed
/// before either binary existed. It can authorise a bounded repair, never ready.
fn settled_cross_failure(basis: &CrossOwnerVerification, proof: &CrossOwnerObservation) -> bool {
    &proof.basis == basis
        && proof.enumerated
        && proof.error.is_none()
        && source_identity_matches(basis, &proof.source_cut)
        && basis.required_jobs.iter().all(|name| {
            let matches = proof
                .checks
                .iter()
                .filter(|c| &c.name == name)
                .collect::<Vec<_>>();
            matches.len() == 1
                && matches[0].status == "completed"
                && matches[0].head_revision == basis.workflow_revision
                && Some(matches[0].run_id.as_str())
                    == basis.run_id.map(|v| v.to_string()).as_deref()
                && Some(matches[0].run_attempt) == basis.run_attempt
                && matches!(
                    matches[0].conclusion.as_deref(),
                    Some("success" | "failure" | "timed_out")
                )
        })
        && proof.checks.iter().any(|c| {
            basis.required_jobs.contains(&c.name)
                && matches!(c.conclusion.as_deref(), Some("failure" | "timed_out"))
        })
}
fn cross_owner_failures(
    basis: &CrossOwnerVerification,
    proof: &CrossOwnerObservation,
) -> Vec<String> {
    let mut errors = Vec::new();
    if &proof.basis != basis {
        errors.push("cross-owner proof is for another selected source basis".into());
    }
    if !proof.enumerated {
        errors.push("cross-owner job enumeration incomplete".into());
    }
    if let Some(error) = &proof.error {
        errors.push(error.clone());
    }
    if !source_cut_matches(basis, &proof.source_cut) {
        errors.push("cross-owner executed source/build cut missing or stale".into());
    }
    for name in &basis.required_jobs {
        let jobs = proof
            .checks
            .iter()
            .filter(|c| c.name == *name)
            .collect::<Vec<_>>();
        if jobs.len() != 1 {
            errors.push(format!(
                "cross-owner required job missing or ambiguous: {name}"
            ));
            continue;
        }
        let job = jobs[0];
        if job.head_revision != basis.workflow_revision
            || Some(job.run_id.as_str()) != basis.run_id.map(|v| v.to_string()).as_deref()
            || Some(job.run_attempt) != basis.run_attempt
        {
            errors.push(format!(
                "cross-owner required job has stale run/attempt/head: {name}"
            ));
        } else if job.status != "completed" || job.conclusion.as_deref() != Some("success") {
            errors.push(format!("cross-owner required job unresolved: {name}"));
        }
    }
    errors
}

fn observe_cross_owner(
    binary: &str,
    basis: &CrossOwnerVerification,
    deadline: Instant,
) -> CrossOwnerObservation {
    let mut proof = CrossOwnerObservation {
        basis: basis.clone(),
        checks: vec![],
        source_cut: Value::Null,
        evidence_refs: BTreeSet::from([format!(
            "github:{}:cross-owner:{}",
            basis.repository, basis.workflow_revision
        )]),
        enumerated: false,
        error: None,
    };
    let result = (|| -> Result<(), String> {
        let id = basis
            .run_id
            .ok_or("bind exact cross-owner workflow run after dispatch")?;
        let attempt = basis
            .run_attempt
            .ok_or("cross-owner workflow attempt absent")?;
        let exact_run = |run: &Value| -> bool {
            run["id"].as_u64() == Some(id)
                && run["head_sha"] == basis.workflow_revision
                && run["run_attempt"].as_u64() == Some(attempt)
                && run["path"] == ".github/workflows/cross-product.yml"
        };
        let endpoint = format!("repos/{}/actions/runs/{id}", basis.repository);
        let run = gh(binary, endpoint.clone(), deadline)?;
        if !exact_run(&run) {
            return Err("cross-owner workflow run/head/attempt moved".into());
        }
        // These are owner commit objects, not workflow checkout labels.
        for (_, source) in selected_sources(basis) {
            let commit = gh(
                binary,
                format!("repos/{}/commits/{}", source.repository, source.revision),
                deadline,
            )?;
            if commit["sha"] != source.revision || commit["commit"]["tree"]["sha"] != source.tree {
                return Err(
                    "cross-owner immutable owner commit/tree differs from selection".into(),
                );
            }
            proof.evidence_refs.insert(format!(
                "github:{}:commit:{}:git-tree:{}",
                source.repository, source.revision, source.tree
            ));
        }
        let mut jobs = Vec::new();
        let mut total = None;
        let mut ids = BTreeSet::new();
        for page in 1..=10 {
            let value = gh(
                binary,
                format!("{endpoint}/attempts/{attempt}/jobs?per_page=100&page={page}"),
                deadline,
            )?;
            let expected = value["total_count"]
                .as_u64()
                .ok_or("cross-owner job count absent")? as usize;
            if expected > MAX_CHECKS || total.is_some_and(|n| n != expected) {
                return Err("cross-owner jobs exceed bound or changed across pages".into());
            }
            total = Some(expected);
            let page_jobs = value["jobs"]
                .as_array()
                .ok_or("cross-owner job enumeration unreadable")?;
            for job in page_jobs {
                let job_id = job["id"]
                    .as_u64()
                    .filter(|v| *v > 0)
                    .ok_or("cross-owner job identity missing")?;
                if !ids.insert(job_id) {
                    return Err("cross-owner duplicated job identity".into());
                }
                jobs.push(job.clone());
            }
            if jobs.len() == expected {
                break;
            }
            if page_jobs.is_empty() || jobs.len() > expected {
                return Err("cross-owner job enumeration incomplete".into());
            }
        }
        if total != Some(jobs.len()) {
            return Err("cross-owner job pagination exhausted".into());
        }
        for name in &basis.required_jobs {
            let matching = jobs
                .iter()
                .filter(|j| j["name"].as_str() == Some(name))
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(format!(
                    "cross-owner required job missing or ambiguous: {name}"
                ));
            }
            let job = matching[0];
            if job["run_id"].as_u64() != Some(id) || job["head_sha"] != basis.workflow_revision {
                return Err("cross-owner job run/head differs from selected attempt".into());
            }
            let job_id = job["id"].as_u64().unwrap();
            let log_endpoint = format!("repos/{}/actions/jobs/{job_id}/logs", basis.repository);
            proof.checks.push(HostedCheck {
                name: name.clone(),
                head_revision: basis.workflow_revision.clone(),
                run_id: id.to_string(),
                run_attempt: attempt,
                status: job["status"]
                    .as_str()
                    .ok_or("cross-owner job status unreadable")?
                    .into(),
                conclusion: job["conclusion"].as_str().map(str::to_owned),
                evidence_ref: format!("https://api.github.com/{log_endpoint}"),
            });
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("delivery hosted-read total wall budget exhausted")?;
            let logs = crate::sensing_sources::owner_text_with_timeout(
                binary,
                &["api".into(), log_endpoint.clone()],
                remaining.min(Duration::from_secs(60)),
            )?;
            let cut = marker_source_cut(&logs)?;
            let failed = job["status"] == "completed"
                && matches!(job["conclusion"].as_str(), Some("failure" | "timed_out"));
            if !(source_cut_matches(basis, &cut) || failed && source_identity_matches(basis, &cut))
            {
                return Err(
                    "cross-owner executed source/build cut differs from selected owner revisions"
                        .into(),
                );
            }
            if !proof.source_cut.is_null() && proof.source_cut != cut {
                return Err("cross-owner jobs disagree on executed source/build cut".into());
            }
            proof.source_cut = cut;
            proof
                .evidence_refs
                .insert(format!("https://api.github.com/{log_endpoint}"));
        }
        let fresh = gh(binary, endpoint, deadline)?;
        if !exact_run(&fresh)
            || fresh["status"] != run["status"]
            || fresh["conclusion"] != run["conclusion"]
        {
            return Err("cross-owner workflow changed during source-cut reading".into());
        }
        if fresh["status"] != "completed" {
            return Err("cross-owner workflow remains pending".into());
        }
        if fresh["conclusion"] != "success"
            && proof
                .checks
                .iter()
                .all(|c| c.conclusion.as_deref() == Some("success"))
        {
            return Err("cross-owner workflow has unresolved unenumerated required failure".into());
        }
        proof.enumerated = true;
        Ok(())
    })();
    if let Err(error) = result {
        proof.error = Some(error);
    }
    proof
}

/// Actual GitHub read, complete pagination and exact workflow-run/attempt
/// correlation. Read failures remain retained observations with no green state.
fn observe_github(binary: &str, selected: &DeliveryPlan) -> HostedObservation {
    let deadline = Instant::now() + Duration::from_millis(selected.timeout_ms);
    let mut observation = HostedObservation {
        candidate_revision: selected.candidate_revision.clone(),
        candidate_fingerprint: None,
        remote_head: String::new(),
        enumerated: false,
        checks: vec![],
        cross_owner: None,
        evidence_refs: BTreeSet::from([format!(
            "github:{}:pull:{}",
            selected.repository,
            selected
                .pull_request
                .map_or_else(|| "unselected".into(), |n| n.to_string())
        )]),
        error: None,
    };
    let result = (|| -> Result<(), String> {
        let pull_request = selected
            .pull_request
            .ok_or("publish task branch and select exact PR before hosted verification")?;
        let fingerprint = observe_commit_tree(binary, selected, deadline)?;
        observation.evidence_refs.insert(format!(
            "github:{}:commit:{}:{}",
            selected.repository, selected.candidate_revision, fingerprint
        ));
        observation.candidate_fingerprint = Some(fingerprint);
        let pr = gh(
            binary,
            format!("repos/{}/pulls/{}", selected.repository, pull_request),
            deadline,
        )?;
        if pr["head"]["ref"].as_str() != Some(&selected.branch)
            || pr["head"]["sha"].as_str() != Some(&selected.candidate_revision)
        {
            return Err("PR target/head differs from selected task branch/candidate".into());
        }
        if let Some(conflict) = pull_request_conflict(&pr) {
            return Err(conflict);
        }
        observation.remote_head = remote_head(binary, selected, deadline)?;
        if observation.remote_head != selected.candidate_revision {
            return Err("remote moved after candidate selection".into());
        }
        let mut count = 0usize;
        let mut total = None;
        let mut workflow_attempts = BTreeMap::new();
        for page in 1..=10 {
            let value = gh(
                binary,
                format!(
                    "repos/{}/commits/{}/check-runs?filter=latest&per_page=100&page={page}",
                    selected.repository, selected.candidate_revision
                ),
                deadline,
            )?;
            let expected = value["total_count"]
                .as_u64()
                .ok_or("GitHub omitted check enumeration count")?
                as usize;
            if total.is_some_and(|n| n != expected) {
                return Err("check enumeration changed across pages".into());
            }
            total = Some(expected);
            if expected > MAX_CHECKS {
                return Err("check enumeration exceeds bounded capacity".into());
            }
            let checks = value["check_runs"]
                .as_array()
                .ok_or("check list unreadable")?;
            count += checks.len();
            for check in checks {
                let name = check["name"]
                    .as_str()
                    .ok_or("check name unreadable")?
                    .to_owned();
                let mut run_id = check["id"]
                    .as_u64()
                    .filter(|v| *v > 0)
                    .ok_or("check identity missing")?
                    .to_string();
                let mut run_attempt = 1;
                if let Some(url) = check["details_url"].as_str() {
                    if let Some(id) = url
                        .split("/actions/runs/")
                        .nth(1)
                        .and_then(|v| v.split('/').next())
                    {
                        if !id.bytes().all(|b| b.is_ascii_digit()) {
                            return Err("workflow run id invalid".into());
                        }
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            workflow_attempts.entry(id.to_owned())
                        {
                            entry.insert(workflow_check_ids(binary, selected, id, deadline)?);
                        }
                        let (attempt, check_ids) = &workflow_attempts[id];
                        let check_id = check["id"].as_u64().ok_or("check id unreadable")?;
                        if !check_ids.contains(&check_id) {
                            return Err("check is from a superseded workflow attempt; reread current hosted result".into());
                        }
                        run_id = id.into();
                        run_attempt = *attempt;
                    }
                }
                observation.checks.push(HostedCheck {
                    name,
                    head_revision: check["head_sha"]
                        .as_str()
                        .ok_or("check head unreadable")?
                        .into(),
                    run_id,
                    run_attempt,
                    status: check["status"]
                        .as_str()
                        .ok_or("check status unreadable")?
                        .into(),
                    conclusion: check["conclusion"].as_str().map(str::to_owned),
                    evidence_ref: check["url"]
                        .as_str()
                        .ok_or("check evidence URL missing")?
                        .into(),
                });
            }
            if count == expected {
                break;
            }
            if checks.is_empty() || count > expected {
                return Err("check enumeration incomplete".into());
            }
        }
        if total != Some(count) {
            return Err("check pagination exhausted".into());
        }
        if let Some(cross) = &selected.cross_owner_verification {
            let proof = observe_cross_owner(binary, cross, deadline);
            observation
                .evidence_refs
                .extend(proof.evidence_refs.clone());
            observation.cross_owner = Some(proof);
        }
        // Revalidate cached run attempts as well as the branch head: a rerun may
        // supersede a green job while later check pages are being read.
        for (id, (attempt, _)) in &workflow_attempts {
            let run = gh(
                binary,
                format!("repos/{}/actions/runs/{id}", selected.repository),
                deadline,
            )?;
            if run["head_sha"].as_str() != Some(&selected.candidate_revision)
                || run["run_attempt"].as_u64() != Some(*attempt)
            {
                return Err("workflow attempt moved while reading hosted checks".into());
            }
        }
        // A second head read closes movement across the enumeration aperture.
        if remote_head(binary, selected, deadline)? != selected.candidate_revision {
            return Err("remote moved while reading hosted checks".into());
        }
        observation.enumerated = true;
        Ok(())
    })();
    if let Err(error) = result {
        observation.error = Some(error);
    }
    observation
}

/// A run's current attempt alone is insufficient: its old successful check-run
/// may still be visible during a rerun. Match the immutable check ID to the
/// complete job set for that exact attempt before accepting it.
fn workflow_check_ids(
    binary: &str,
    selected: &DeliveryPlan,
    id: &str,
    deadline: Instant,
) -> Result<(u64, BTreeSet<u64>), String> {
    let run = gh(
        binary,
        format!("repos/{}/actions/runs/{id}", selected.repository),
        deadline,
    )?;
    if run["head_sha"].as_str() != Some(&selected.candidate_revision) {
        return Err("workflow run head differs from exact check revision".into());
    }
    let attempt = run["run_attempt"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or("workflow attempt missing")?;
    let mut ids = BTreeSet::new();
    let mut seen = 0;
    let mut total = None;
    for page in 1..=10 {
        let value = gh(
            binary,
            format!(
                "repos/{}/actions/runs/{id}/attempts/{attempt}/jobs?per_page=100&page={page}",
                selected.repository
            ),
            deadline,
        )?;
        let expected = value["total_count"]
            .as_u64()
            .ok_or("workflow job enumeration count missing")? as usize;
        if expected > MAX_CHECKS || total.is_some_and(|n| n != expected) {
            return Err(
                "workflow job enumeration exceeds capacity or changed between pages".into(),
            );
        }
        total = Some(expected);
        let jobs = value["jobs"].as_array().ok_or("workflow jobs unreadable")?;
        seen += jobs.len();
        for job in jobs {
            let url = job["check_run_url"]
                .as_str()
                .ok_or("workflow job check-run identity missing")?;
            let check = url
                .rsplit('/')
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or("workflow job check-run identity invalid")?;
            if !ids.insert(check) {
                return Err("workflow job enumeration duplicated a check-run identity".into());
            }
        }
        if seen == expected {
            break;
        }
        if jobs.is_empty() || seen > expected {
            return Err("workflow job enumeration incomplete".into());
        }
    }
    if total != Some(seen) {
        return Err("workflow job pagination exhausted".into());
    }
    Ok((attempt, ids))
}

pub fn execute(
    path: &Path,
    request: DeliveryRequest,
) -> Result<DeliveryReading, FactoryAttemptError> {
    if request.contract != DELIVERY_ACTION {
        return Err(invalid("wrong delivery action contract"));
    }
    text(&request.request_ref, "request ref")?;
    let mut store = FileAttemptStore::open_run(path, request.run_ref.clone())?;
    let reading = store.reading()?;
    let record = current(&reading, &request.attempt_ref)?;
    let operation_ref = format!("delivery:{}", request.request_ref);
    if let Some(prior) = record
        .observations
        .iter()
        .find(|r| r.contract == DELIVERY_OBSERVATION && r.receipt_ref == operation_ref)
    {
        if prior.payload["request"] != serde_json::to_value(&request.operation)? {
            return Err(invalid(
                "delivery request identity reused with changed operation",
            ));
        }
        return readiness_live(&store, &reading, &request.attempt_ref);
    }
    if reading.revision != request.expected_revision {
        return Err(FactoryAttemptError::RevisionConflict {
            expected: request.expected_revision,
            actual: reading.revision,
        });
    }
    let selected = plan(record)?;
    let mut phase = OwnerOperationPhase::Observed;
    let mut evidence_refs = BTreeSet::new();
    let mut identity = operation_ref.clone();
    let mut payload = match &request.operation {
        DeliveryOperation::Select { plan } => json!({"kind":"selection","plan":plan}),
        DeliveryOperation::ObserveGithub { gh_binary } => {
            let selected = selected
                .as_ref()
                .ok_or_else(|| invalid("select plan before polling"))?;
            let observation = observe_github(gh_binary, selected);
            evidence_refs = observation.evidence_refs.clone();
            json!({"kind":"hosted-ci","observation":observation,"standing":"actual-github-read"})
        }
        DeliveryOperation::ObserveControlled { observation } => {
            evidence_refs = observation.evidence_refs.clone();
            json!({"kind":"hosted-ci","observation":observation,"standing":"controlled-backend"})
        }
        DeliveryOperation::RequestRepair {
            observation_ref,
            failure_class,
            failure_logs,
            artifact_refs,
            prior_decision_refs,
        } => {
            evidence_refs = artifact_refs.clone();
            evidence_refs.insert(observation_ref.clone());
            let selected = selected
                .as_ref()
                .ok_or_else(|| invalid("repair plan absent"))?;
            let (retained, ci) =
                last_ci(record)?.ok_or_else(|| invalid("repair observation absent"))?;
            let fresh = if retained.payload["standing"] == "actual-github-read" {
                let binary = retained.payload["request"]["gh_binary"]
                    .as_str()
                    .ok_or_else(|| invalid("actual hosted repair needs retained exact reader"))?;
                let fresh = observe_github(binary, selected);
                if !fresh.enumerated
                    || fresh.error.is_some()
                    || fresh.candidate_revision != selected.candidate_revision
                    || fresh.remote_head != selected.candidate_revision
                    || failed_identities(selected, &fresh) != failed_identities(selected, &ci)
                {
                    return Err(invalid("selected hosted failure changed or became unreadable; observe current exact CI before requesting repair"));
                }
                Some(fresh)
            } else {
                None
            };
            json!({"kind":"repair","observationRef":observation_ref,"failureClass":failure_class,"failureLogs":failure_logs,
                "artifactRefs":artifact_refs,"priorDecisionRefs":prior_decision_refs,"ciRevalidateRepairs":true,"failureIdentities":failed_identities(selected,&ci),"freshRead":fresh})
        }
        DeliveryOperation::ReservePublication {
            reviewer_attempt_ref,
            gh_binary,
        } => {
            let selected = selected
                .as_ref()
                .ok_or_else(|| invalid("publication plan absent"))?;
            if request.caller.caller_ref != selected.publisher_ref {
                return Err(invalid(
                    "only admitted single publisher may reserve remote effect",
                ));
            }
            let reviewer = current(&reading, reviewer_attempt_ref)?;
            if !crate::attempt_runtime::has_passing_verification(record)
                || !crate::attempt_runtime::has_passing_verification(reviewer)
                || !verification_material_current(reviewer, selected)
                || reviewer.disposition.participant.agent_ref
                    == record.disposition.participant.agent_ref
                || reviewer.disposition.participant.agency_ref
                    == record.disposition.participant.agency_ref
                || reviewer.disposition.body.agent_session_ref
                    == record.disposition.body.agent_session_ref
                || !reading
                    .independent_reviewers
                    .get(reviewer.execution_ref.as_deref().unwrap_or(""))
                    .is_some_and(|r| r.review_of.contains(&record.workflow_unit_ref))
            {
                return Err(invalid("publication requires verified candidate and actual independently admitted reviewer"));
            }
            if !reviewer.disposition.selected_inputs.iter().any(|i| {
                i.predecessor == record.workflow_unit_ref
                    && i.execution_ref == record.execution_ref.as_deref().unwrap_or("")
                    && i.artifacts.iter().any(|a| {
                        a.subject_ref == selected.subject_ref
                            && a.subject_revision == selected.subject_revision
                            && material_evidence_current(&a.evidence_refs, selected)
                    })
            }) {
                return Err(invalid(
                    "reviewer did not select this exact candidate material",
                ));
            }
            candidate_tree(selected).map_err(invalid)?;
            let deadline = Instant::now() + Duration::from_millis(selected.timeout_ms);
            let actual = remote_head_optional(gh_binary, selected, deadline).map_err(invalid)?;
            if actual != selected.expected_remote_head {
                return Err(invalid("remote head moved from admitted expected head before publication; reconcile without pushing"));
            }
            if actual.as_deref() == Some(&selected.candidate_revision) {
                observe_commit_tree(gh_binary, selected, deadline).map_err(invalid)?;
            }
            phase = OwnerOperationPhase::Dispatching;
            evidence_refs = reviewer
                .readable_return
                .iter()
                .flat_map(|r| r.evidence_refs.clone())
                .collect();
            json!({"kind":"publication-intent","publisherRef":selected.publisher_ref,"repository":selected.repository,
                "branch":selected.branch,"expectedRemoteHead":selected.expected_remote_head,"candidateRevision":selected.candidate_revision,
                "candidateFingerprint":selected.candidate_fingerprint,"reviewerAttemptRef":reviewer_attempt_ref})
        }
        DeliveryOperation::ReconcilePublication {
            gh_binary,
            publication_ref,
        } => {
            let selected = selected
                .as_ref()
                .ok_or_else(|| invalid("publication plan absent"))?;
            let intent = record
                .observations
                .iter()
                .find(|r| {
                    r.receipt_ref == *publication_ref && r.payload["kind"] == "publication-intent"
                })
                .ok_or_else(|| invalid("exact publication intent absent"))?;
            identity = intent.operation_ref.clone();
            let deadline = Instant::now() + Duration::from_millis(selected.timeout_ms);
            let mut original = selected.clone();
            original.candidate_revision = intent.payload["candidateRevision"]
                .as_str()
                .ok_or_else(|| invalid("publication intent candidate missing"))?
                .into();
            original.candidate_fingerprint = intent.payload["candidateFingerprint"]
                .as_str()
                .ok_or_else(|| invalid("publication intent fingerprint missing"))?
                .into();
            let observed = (|| -> Result<String, String> {
                let head = remote_head(gh_binary, &original, deadline)?;
                if head != original.candidate_revision {
                    return Err("remote differs from exact retained candidate".into());
                }
                observe_commit_tree(gh_binary, &original, deadline)?;
                Ok(head)
            })();
            match observed {
                Ok(head) if head == intent.payload["candidateRevision"].as_str().unwrap_or("") => {
                    evidence_refs.insert(format!(
                        "github:{}:ref:heads/{}@{}",
                        selected.repository, selected.branch, head
                    ));
                    json!({"kind":"publication-readback","publicationRef":publication_ref,"remoteHead":head,"candidateFingerprint":original.candidate_fingerprint})
                }
                result => {
                    phase = OwnerOperationPhase::Uncertain;
                    json!({"kind":"publication-readback","publicationRef":publication_ref,"remoteHead":result.as_ref().ok(),
                        "error":result.err().unwrap_or_else(||"remote differs from expected candidate; reconcile before another effect".into())})
                }
            }
        }
    };
    payload["request"] = serde_json::to_value(&request.operation)?;
    let partial_effect_refs =
        if payload["kind"] == "publication-readback" && phase == OwnerOperationPhase::Observed {
            evidence_refs.clone()
        } else {
            BTreeSet::new()
        };
    let receipt = OwnerOperationReceipt {
        owner_ref: "factory/delivery".into(),
        contract: DELIVERY_OBSERVATION.into(),
        operation_ref: identity,
        receipt_ref: operation_ref,
        source_revision: selected
            .map(|p| p.candidate_revision)
            .unwrap_or_else(|| reading.workflow_source_revision.clone()),
        phase,
        evidence_refs,
        partial_effect_refs,
        payload,
    };
    validate_observation(record, &receipt)?;
    store.apply(FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: request.projection_ref,
        caller: request.caller,
        run_ref: request.run_ref,
        expected_revision: request.expected_revision,
        authority: request.authority,
        operation: FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt,
        },
    })?;
    let retained = store.reading()?;
    if matches!(&request.operation, DeliveryOperation::ObserveGithub { .. }) {
        readiness(&retained, &request.attempt_ref)
    } else {
        readiness_live(&store, &retained, &request.attempt_ref)
    }
}

pub fn human(reading: &DeliveryReading) -> String {
    let mut result = format!("{}\nRun: {}\nAttempt: {}\nCandidate: {}\nReady: {}\nRequired checks: {}\nReview evidence: {}\nPublication evidence: {}\nRepairs: {}\nEnvironment retries: {}\nNext: {}\n{}",
        reading.contract,reading.run_ref,reading.attempt_ref,reading.candidate_revision.as_deref().unwrap_or("unselected"),reading.ready,
        reading.required_checks.iter().cloned().collect::<Vec<_>>().join(", "),reading.review_evidence_refs.len(),reading.publication_evidence_refs.len(),
        reading.repairs,reading.environment_retries,reading.next_action,reading.failures.join("\n"));
    if let Some(basis) = &reading.cross_owner_verification {
        result.push_str(&format!(
            "\nCross-owner workflow: {} @ {} (run {}, attempt {})\nRequired cross-owner jobs: {}",
            basis.repository,
            basis.workflow_revision,
            basis
                .run_id
                .map_or_else(|| "unbound".into(), |v| v.to_string()),
            basis
                .run_attempt
                .map_or_else(|| "unbound".into(), |v| v.to_string()),
            basis
                .required_jobs
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if let Some(workcell) = &basis.workcell {
            result.push_str(&format!(
                "\nSelected Workcell: {} @ {} (git-tree:{})",
                workcell.repository, workcell.revision, workcell.tree
            ));
        }
    }
    result
}

pub fn execute_cli(args: &[String], stdin_override: Option<&str>) -> Result<String, CliError> {
    let positional = args
        .iter()
        .filter(|a| a.as_str() != "--json")
        .collect::<Vec<_>>();
    let command = positional.first().map(|v| v.as_str());
    if matches!(command, None | Some("help" | "--help" | "-h")) {
        return Ok(format!("Factory exact-revision delivery\n  factory attempt delivery <state> <request-json|-> [--json]\n  factory attempt readiness <state> <run-ref> <attempt-ref> [--json]\nContract: {DELIVERY_ACTION}\nReadiness: {DELIVERY_READING}\nPublication uses one admitted publisher; reserve before effect, reconcile exact remote head afterwards."));
    }
    let path = Path::new(
        positional
            .get(1)
            .ok_or_else(|| CliError::new("missing state"))?
            .as_str(),
    );
    let reading = match command {
        Some("delivery") => {
            let input_path = positional.get(2).map(|v| v.as_str()).unwrap_or("-");
            let input = if input_path != "-" {
                fs::read_to_string(input_path).map_err(CliError::from_native)?
            } else if let Some(input) = stdin_override {
                input.into()
            } else {
                let mut input = String::new();
                io::stdin()
                    .read_to_string(&mut input)
                    .map_err(CliError::from_native)?;
                input
            };
            execute(
                path,
                serde_json::from_str(&input).map_err(CliError::from_native)?,
            )
            .map_err(CliError::from_native)?
        }
        Some("readiness") => {
            let run = positional
                .get(2)
                .ok_or_else(|| CliError::new("missing Run"))?
                .parse::<RunRef>()
                .map_err(CliError::from_native)?;
            let attempt = positional
                .get(3)
                .ok_or_else(|| CliError::new("missing attempt"))?;
            read_current_readiness(path, run, attempt).map_err(CliError::from_native)?
        }
        _ => return Err(CliError::new("unknown delivery command")),
    };
    if args.iter().any(|a| a == "--json") {
        serde_json::to_string_pretty(&reading).map_err(CliError::from_native)
    } else {
        Ok(human(&reading))
    }
}

#[cfg(test)]
mod conflict_tests {
    use super::pull_request_conflict;
    use serde_json::json;

    #[test]
    fn only_a_definite_conflict_counts() {
        assert!(
            pull_request_conflict(&json!({"mergeable": false, "mergeable_state": "dirty"}))
                .unwrap()
                .starts_with("PR conflicts with its base (dirty)")
        );
        assert!(pull_request_conflict(&json!({"mergeable_state": "dirty"})).is_some());
        assert!(
            pull_request_conflict(&json!({"mergeable": null, "mergeable_state": "unknown"}))
                .is_none()
        );
        assert!(
            pull_request_conflict(&json!({"mergeable": true, "mergeable_state": "blocked"}))
                .is_none()
        );
        assert!(pull_request_conflict(&json!({})).is_none());
    }
}
