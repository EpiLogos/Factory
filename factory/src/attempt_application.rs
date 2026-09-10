//! Public attempt Action admission. All mutations still use the one existing
//! FileAttemptStore and ExecutableOrchestration; this module has no state file,
//! scheduler, execution loop or substitute worker. The store's locked revision
//! check makes these current-reading preconditions race-safe: a concurrent
//! mutation after this read is refused, never applied against an older basis.

use crate::attempt_runtime::*;
use crate::core::run::{Run, WorkflowUnitRef};
use crate::orchestration::LegStatus;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Fresh public read on each call; retaining a provider object is not retaining
/// a lease on current Run, source, writer or retry state.
pub fn read_attempts(path: &Path) -> Result<FactoryAttemptReading, FactoryAttemptError> {
    let reading = FileAttemptStore::open(path)?.reading()?;
    validate_reading(&reading)?;
    Ok(reading)
}

pub fn apply_attempt_action(
    path: &Path,
    request: FactoryAttemptActionRequest,
) -> Result<FactoryAttemptActionReceipt, FactoryAttemptError> {
    let mut store = FileAttemptStore::open(path)?;
    let reading = store.reading()?;
    validate_reading(&reading)?;
    if request.expected_revision != reading.revision {
        return Err(FactoryAttemptError::RevisionConflict {
            expected: request.expected_revision,
            actual: reading.revision,
        });
    }
    // Read only this native envelope's canonical Run; it is never reconstructed
    // from the request's assertions or a consumer-owned projected RunMap.
    #[derive(Deserialize)]
    struct RunBasis {
        revision: u64,
        run: Run,
    }
    let basis: RunBasis = serde_json::from_slice(&fs::read(path)?)?;
    if basis.revision != reading.revision
        || basis.run.reference() != &reading.run_ref
        || basis.run.revision().get() != reading.run_revision
    {
        return Err(invalid(
            "native Run changed during Action admission; reread",
        ));
    }
    validate_operation(&reading, &basis.run, &request.operation)?;
    store.apply(request)
}

fn validate_reading(reading: &FactoryAttemptReading) -> Result<(), FactoryAttemptError> {
    let mut identities = BTreeSet::new();
    for record in &reading.attempts {
        let execution = execution_identity(record);
        if !identities.insert(execution) {
            return Err(invalid("two attempt records identify the same execution"));
        }
        let leg = reading
            .legs
            .get(&record.workflow_unit_ref)
            .ok_or_else(|| invalid("attempt record is not linked to a coordinator leg"))?;
        if !leg
            .attempts
            .iter()
            .any(|attempt| attempt.execution_ref == execution)
        {
            return Err(invalid(
                "attempt record is not linked to its execution history",
            ));
        }
    }
    for leg in reading.legs.values() {
        for attempt in &leg.attempts {
            if !identities.contains(attempt.execution_ref.as_str()) {
                return Err(invalid(
                    "coordinator history has an untracked native attempt",
                ));
            }
        }
    }
    Ok(())
}

fn validate_operation(
    reading: &FactoryAttemptReading,
    run: &Run,
    operation: &FactoryAttemptOperation,
) -> Result<(), FactoryAttemptError> {
    use FactoryAttemptOperation::*;
    match operation {
        StartSerial {
            disposition,
            retry_grant,
            tracking,
            parent_journey_ref,
            ..
        } => {
            validate_start(run, disposition, retry_grant.as_ref())?;
            nonempty(parent_journey_ref, "parent Journey")?;
            for fact in tracking {
                validate_fact(fact)?;
            }
        }
        StartFork {
            attempts,
            parent_journey_ref,
        } => {
            nonempty(parent_journey_ref, "parent Journey")?;
            let mut refs = BTreeSet::new();
            for start in attempts {
                if !refs.insert(&start.attempt_ref) {
                    return Err(invalid("fork repeats an attempt identity"));
                }
                validate_start(run, &start.disposition, start.retry_grant.as_ref())?;
                for fact in &start.tracking {
                    validate_fact(fact)?;
                }
            }
        }
        RequestCancellation { attempt_ref }
        | AcceptCancellation { attempt_ref }
        | RecordProcessTermination { attempt_ref }
        | MarkQuiescent { attempt_ref }
        | IncorporateLateResult { attempt_ref } => {
            current_attempt(reading, attempt_ref)?;
        }
        Fail {
            attempt_ref,
            evidence_refs,
            ..
        } => {
            let record = current_attempt(reading, attempt_ref)?;
            if has_uncertain_operation(record) {
                return Err(invalid(
                    "uncertain owner effects require reconciliation, not failure/writer release",
                ));
            }
            if !evidence_refs.is_superset(&partial_effects(record)) {
                return Err(invalid(
                    "failure evidence must retain every known partial effect",
                ));
            }
        }
        Retry {
            workflow_unit_ref,
            task_ref,
            parent_journey_ref,
            grant_ref,
            disposition,
            tracking,
            reresolution,
            ..
        } => {
            let prior = current_for_unit(reading, workflow_unit_ref)?;
            let leg = &reading.legs[workflow_unit_ref];
            if prior.task_ref != *task_ref
                || leg.delegation.parent_journey_ref != *parent_journey_ref
            {
                return Err(invalid("retry must retain its task and parent Journey"));
            }
            if prior.disposition.budget.retry_grant_ref.as_deref() != Some(grant_ref.as_str()) {
                return Err(invalid(
                    "retry cannot switch the previously consumed retry grant",
                ));
            }
            validate_start(run, disposition, None)?;
            if has_uncertain_operation(prior) {
                return Err(invalid("retry is blocked by an unreconciled owner outcome"));
            }
            if prior.disposition.participant.agent_ref != disposition.participant.agent_ref {
                return Err(invalid("retry cannot silently replace the enduring Agent"));
            }
            if prior.disposition.body.model_ref != disposition.body.model_ref
                || prior.disposition.body.provider_ref != disposition.body.provider_ref
            {
                return Err(invalid("explicit model/provider replacement requires its owner-authorised operation; retry is not that authority"));
            }
            if prior.disposition.budget.maximum_attempts != disposition.budget.maximum_attempts
                || prior.disposition.budget.retry_grant_ref != disposition.budget.retry_grant_ref
            {
                return Err(invalid("retry cannot replenish its total attempt budget"));
            }
            if let Some(limit) = prior.disposition.budget.wall_clock_timeout_ms {
                if disposition
                    .budget
                    .wall_clock_timeout_ms
                    .is_none_or(|next| next > limit)
                {
                    return Err(invalid("retry cannot remove or increase its time bound"));
                }
            }
            if let Some(limit) = prior.disposition.budget.cost_ceiling_usd {
                if disposition
                    .budget
                    .cost_ceiling_usd
                    .is_none_or(|next| next > limit)
                {
                    return Err(invalid("retry cannot remove or increase its cost ceiling"));
                }
            }
            match (&prior.disposition.placement, &disposition.placement) {
                (Some(before), Some(after)) => {
                    if !after.protected_paths.is_superset(&before.protected_paths)
                        || !after
                            .required_coverage
                            .is_superset(&before.required_coverage)
                    {
                        return Err(invalid(
                            "retry cannot drop protected sources or required coverage",
                        ));
                    }
                    if before.now_ref != after.now_ref
                        && reresolution
                            .as_ref()
                            .and_then(|value| value.replacement_now_ref.as_deref())
                            != Some(after.now_ref.as_str())
                    {
                        return Err(invalid(
                            "changed NOW requires explicit matching re-resolution",
                        ));
                    }
                }
                (Some(_), None) => return Err(invalid("retry cannot remove placement protection")),
                _ => {}
            }
            if prior.disposition.body.material_world_ref != disposition.body.material_world_ref
                && reresolution
                    .as_ref()
                    .and_then(|value| value.replacement_material_ref.as_deref())
                    != disposition.body.material_world_ref.as_deref()
            {
                return Err(invalid(
                    "changed material requires explicit matching re-resolution",
                ));
            }
            if prior.disposition.body.harness_ref != disposition.body.harness_ref
                && reresolution
                    .as_ref()
                    .and_then(|value| value.replacement_harness_ref.as_deref())
                    != Some(disposition.body.harness_ref.as_str())
            {
                return Err(invalid(
                    "changed harness requires explicit matching re-resolution",
                ));
            }
            if !partial_effects(prior).is_empty() || prior.disposition != *disposition {
                let resolution = reresolution.as_ref().ok_or_else(|| {
                    invalid("partial effects or changed conditions require explicit re-resolution")
                })?;
                if resolution.source_revision != disposition.participant.source_revision
                    || !resolution
                        .evidence_refs
                        .is_superset(&partial_effects(prior))
                {
                    return Err(invalid("re-resolution must retain current source basis and partial-effect evidence"));
                }
            }
            for fact in tracking {
                validate_fact(fact)?;
            }
        }
        BindDispatch {
            attempt_ref,
            execution_ref,
            receipt,
        } => {
            let record = current_attempt(reading, attempt_ref)?;
            if execution_ref == &record.disposition.body.agent_session_ref
                || execution_ref == &record.disposition.body.session_space_ref
                || execution_ref == &receipt.operation_ref
            {
                return Err(invalid(
                    "session, delivery and execution identities are not interchangeable",
                ));
            }
        }
        ReturnArtifact { attempt_ref, .. } => {
            let record = attempt(reading, attempt_ref)?;
            if has_uncertain_operation(record) {
                return Err(invalid(
                    "uncertain outcome cannot become a completed Return",
                ));
            }
            let verification = record
                .verifications
                .last()
                .ok_or_else(|| invalid("Return needs current verification"))?;
            if verification.outcome != VerificationOutcome::Passed
                || !verification
                    .obligations
                    .is_superset(&record.disposition.verification_obligations)
            {
                return Err(invalid(
                    "a later failed/unknown verification supersedes an earlier pass",
                ));
            }
            let dispatch = record
                .dispatch
                .as_ref()
                .ok_or_else(|| invalid("Return requires an actual owner dispatch"))?;
            let last = record
                .observations
                .iter()
                .rev()
                .find(|receipt| {
                    receipt.owner_ref == dispatch.owner_ref
                        && receipt.operation_ref == dispatch.operation_ref
                })
                .unwrap_or(dispatch);
            if last.phase != OwnerOperationPhase::Returned {
                return Err(invalid(
                    "submitted, failed or cancelled delivery is not completed provider work",
                ));
            }
        }
        RecordTracking { fact, .. } => validate_fact(fact)?,
        RecordObservation { .. }
        | RecordVerification { .. }
        | RecordReresolution { .. }
        | AdvanceSubject { .. }
        | AttachReceiving { .. }
        | AttachArchive { .. } => {}
    }
    Ok(())
}

fn validate_start(
    run: &Run,
    disposition: &SituatedExecutionDisposition,
    grant: Option<&crate::orchestration::RetryGrant>,
) -> Result<(), FactoryAttemptError> {
    if disposition.selection.demand.project_ref != run.project_ref().to_string() {
        return Err(invalid(
            "Execution Intelligence demand names another Project",
        ));
    }
    if disposition
        .budget
        .cost_ceiling_usd
        .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
        || disposition.budget.wall_clock_timeout_ms == Some(0)
    {
        return Err(invalid("invalid finite execution budget"));
    }
    if let Some(grant) = grant {
        if grant.grant_ref.trim().is_empty() || grant.attempts_allowed == 0 {
            return Err(invalid(
                "retry grant must have an identity and a positive total bound",
            ));
        }
    }
    Ok(())
}

fn execution_identity(record: &FactoryAttemptRecord) -> &str {
    record
        .execution_ref
        .as_deref()
        .unwrap_or(&record.reserved_execution_ref)
}

fn attempt<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == reference)
        .ok_or_else(|| FactoryAttemptError::UnknownAttempt(reference.into()))
}

fn current_attempt<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    let record = attempt(reading, reference)?;
    let current = reading
        .legs
        .get(&record.workflow_unit_ref)
        .ok_or_else(|| invalid("attempt has no native leg"))?;
    if current.execution_ref != execution_identity(record) {
        return Err(invalid(
            "historical attempt cannot mutate its replacement; history remains readable",
        ));
    }
    Ok(record)
}

fn current_for_unit<'a>(
    reading: &'a FactoryAttemptReading,
    unit: &WorkflowUnitRef,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    let leg = reading
        .legs
        .get(unit)
        .ok_or_else(|| invalid("no prior leg for retry"))?;
    reading
        .attempts
        .iter()
        .find(|record| {
            &record.workflow_unit_ref == unit && execution_identity(record) == leg.execution_ref
        })
        .ok_or_else(|| invalid("current coordinator execution has no attempt record"))
}

fn partial_effects(record: &FactoryAttemptRecord) -> BTreeSet<String> {
    record
        .dispatch
        .iter()
        .chain(record.observations.iter())
        .flat_map(|receipt| receipt.partial_effect_refs.iter().cloned())
        .collect()
}

fn has_uncertain_operation(record: &FactoryAttemptRecord) -> bool {
    let mut uncertain = BTreeSet::new();
    for receipt in record.dispatch.iter().chain(record.observations.iter()) {
        let identity = (&receipt.owner_ref, &receipt.operation_ref);
        match receipt.phase {
            OwnerOperationPhase::Uncertain | OwnerOperationPhase::Dispatching => {
                uncertain.insert(identity);
            }
            OwnerOperationPhase::ReconciledNoReplay => {
                uncertain.remove(&identity);
            }
            _ => {}
        }
    }
    !uncertain.is_empty()
}

fn validate_fact(fact: &AttemptTrackingFact) -> Result<(), FactoryAttemptError> {
    let owner = match fact.kind.as_str() {
        "now" | "day" | "flow" | "source-revision" | "source-change" | "receiving" => {
            Some("central")
        }
        "model-usage" | "activity" => Some("actuation"),
        "resource-usage" | "material" | "coverage" => Some("workcell"),
        "harness" | "session" | "context" | "praxis" => Some("aikit"),
        _ => None,
    };
    if owner.is_some_and(|expected| fact.owner_ref != expected) {
        return Err(invalid("tracking fact misattributes its native owner"));
    }
    Ok(())
}

fn nonempty(value: &str, field: &str) -> Result<(), FactoryAttemptError> {
    if value.trim().is_empty() {
        Err(invalid(&format!("{field} must be explicit")))
    } else {
        Ok(())
    }
}

fn invalid(message: &str) -> FactoryAttemptError {
    FactoryAttemptError::InvalidOperation(message.into())
}

/// Same public command grammar and native receipt type. Only admission and
/// current readback are added; all semantic mutation remains in the coordinator.
pub fn execute_attempt_cli(
    args: &[String],
    stdin_override: Option<&str>,
) -> Result<String, FactoryAttemptError> {
    let positional = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .collect::<Vec<_>>();
    if positional.first().map(|value| value.as_str()) != Some("action") {
        if positional.first().map(|value| value.as_str()) == Some("read") {
            let path = positional
                .get(1)
                .ok_or_else(|| invalid("missing state path"))?;
            let reading = read_attempts(Path::new(path))?;
            if args.iter().any(|arg| arg == "--json") {
                return Ok(serde_json::to_string_pretty(&reading)?);
            }
            let mut output = format!(
                "{}\nRun: {} @ {}\nState revision: {}",
                reading.contract, reading.run_ref, reading.run_revision, reading.revision
            );
            for record in &reading.attempts {
                let leg = &reading.legs[&record.workflow_unit_ref];
                let historic = leg.execution_ref != execution_identity(record);
                let status = if historic {
                    "historical".to_owned()
                } else {
                    format!("{:?}", leg.status)
                };
                output.push_str(&format!(
                    "\n\nTask: {}\nAttempt: {} ({})\nAgent: {}\nSource: {} @ {}\nExecution: {}",
                    record.task_ref,
                    record.attempt_ref,
                    status,
                    record.disposition.participant.agent_ref,
                    record.disposition.participant.source_ref,
                    record.disposition.participant.source_revision,
                    record.execution_ref.as_deref().unwrap_or("not dispatched")
                ));
                if let Some(result) = &record.readable_return {
                    output.push_str(&format!(
                        "\nReturn {}: {}",
                        result.return_ref, result.summary
                    ));
                }
                if has_uncertain_operation(record) {
                    output.push_str("\nOwner outcome uncertain; reconciliation required.");
                }
                if !historic && leg.status != LegStatus::Returned {
                    output.push_str("\nNot a completed Factory Return.");
                }
            }
            return Ok(output);
        }
        return crate::attempt_runtime::execute_attempt_cli(args, stdin_override);
    }
    let path = positional
        .get(1)
        .ok_or_else(|| invalid("missing state path"))?;
    let input_path = positional.get(2).map(|value| value.as_str()).unwrap_or("-");
    let input = if input_path != "-" {
        fs::read_to_string(input_path)?
    } else if let Some(input) = stdin_override {
        input.to_owned()
    } else {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        input
    };
    let request = serde_json::from_str(&input)?;
    let receipt = apply_attempt_action(Path::new(path), request)?;
    if args.iter().any(|arg| arg == "--json") {
        return Ok(serde_json::to_string_pretty(&receipt)?);
    }
    Ok(format!(
        "{}\nRun: {}\nOperation: {}\nRevision: {} -> {}\n{}",
        receipt.contract,
        receipt.run_ref,
        receipt.operation,
        receipt.previous_revision,
        receipt.next_revision,
        receipt.standing
    ))
}
