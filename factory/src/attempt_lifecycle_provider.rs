//! Lifecycle and unit decisions use the same canonical provider transaction as
//! attempts. Human-channel assertions retain their evidence; this boundary does
//! not authenticate a person from a role string or create a second inbox.
use crate::attempt_runtime::{
    apply_operation, apply_operation_with_source_currency, FactoryAttemptActionRequest,
    FactoryAttemptError, FactoryAttemptOperation, StoredAttemptState,
};
use crate::build::HumanRequestRecord;
use crate::core::run::{RunLifecycle, RunLifecycleCommand};
use crate::developmental_read::FactoryDevelopmentalState;
use crate::orchestration::LegStatus;
use crate::run_lifecycle::{
    validate_closure, validate_decision_basis, validate_decision_response, validate_finishing,
    UnitDecisionOutcome,
};
use std::collections::BTreeSet;

fn invalid(message: &str) -> FactoryAttemptError {
    FactoryAttemptError::InvalidOperation(message.into())
}

pub(crate) fn apply(
    native: &mut FactoryDevelopmentalState,
    view: &mut StoredAttemptState,
    request: &FactoryAttemptActionRequest,
    native_admission: Option<&crate::attempt_native_receiving::NativeReceivingAdmission>,
) -> Result<(String, Vec<String>, Option<crate::core::run::Run>), FactoryAttemptError> {
    let workflow = crate::workflow::compile_workflow(view.workflow_source.clone())?;
    let engine = view.snapshot.restore(workflow, view.run.clone())?;
    let supersession = crate::attempt_native_store::supersession_basis(native, view);
    let requests = native.build.human_requests_for_run(view.run.reference());
    let pending = requests
        .iter()
        .filter(|record| {
            record.unit_decision_basis.is_some()
                && record.unit_decision_response.is_none()
                && record.unit_decision_retirement.is_none()
        })
        .collect::<Vec<_>>();
    let blocked = pending
        .iter()
        .filter_map(|record| {
            record
                .unit_decision_basis
                .as_ref()
                .map(|basis| basis.workflow_unit_ref.clone())
        })
        .collect::<BTreeSet<_>>();
    let unresolved = pending
        .iter()
        .map(|record| record.human_request_ref.clone())
        .collect::<BTreeSet<_>>();
    match &request.operation {
        FactoryAttemptOperation::TransitionRun {
            command,
            authority,
            closure,
        } => {
            match command.lifecycle {
                RunLifecycle::Finishing => {
                    if !pending.is_empty() {
                        return Err(invalid("Run has unresolved unit decisions"));
                    }
                    validate_finishing(view, &engine)?;
                }
                RunLifecycle::Finished => {
                    native_admission.ok_or_else(|| invalid("Finished requires live native receiving admission; a public receiving label or observation is not proof"))?
                        .validate(request).map_err(|error| invalid(&error))?;
                    validate_closure(
                        view,
                        &engine,
                        closure.as_ref().ok_or_else(|| {
                            invalid(
                                "Finished requires exact native review and receiving closure basis",
                            )
                        })?,
                        &unresolved,
                    )?;
                }
                RunLifecycle::WaitingHuman => {
                    if pending.is_empty() || has_unblocked_frontier(&engine, &blocked) {
                        return Err(invalid("WaitingHuman requires the whole runnable frontier to await canonical decisions"));
                    }
                }
                RunLifecycle::Active
                    if !pending.is_empty() && !has_unblocked_frontier(&engine, &blocked) =>
                {
                    return Err(invalid(
                        "resolve the canonical unit decision before resuming this Run",
                    ));
                }
                RunLifecycle::Aborted | RunLifecycle::Archived
                    if !engine.snapshot().writer_reservations().is_empty()
                        || engine.legs().values().any(|leg| {
                            matches!(
                                leg.status,
                                LegStatus::Active
                                    | LegStatus::Detached
                                    | LegStatus::CancelRequested
                                    | LegStatus::CancellationAccepted
                                    | LegStatus::LateResult
                            )
                        }) =>
                {
                    return Err(invalid(
                            "Run cannot abort/archive while a native execution or writer is not quiescent",
                        ));
                }
                _ => {}
            }
            view.run
                .apply_lifecycle_command(authority, command.clone())
                .map_err(|error| invalid(&error.to_string()))?;
            Ok(("transition-run".into(), vec![], None))
        }
        FactoryAttemptOperation::RequestUnitDecision { request: decision } => {
            if !matches!(
                view.run.lifecycle(),
                RunLifecycle::Active | RunLifecycle::WaitingHuman
            ) {
                return Err(invalid("unit decision requires an active undertaking"));
            }
            validate_decision_basis(view, &engine, &decision.basis)?;
            let leg = engine
                .leg(&decision.basis.workflow_unit_ref)
                .expect("basis validated");
            if !matches!(leg.status, LegStatus::Active | LegStatus::Detached) {
                return Err(invalid("unit decision requires current unfinished work"));
            }
            if decision.human_request_ref.trim().is_empty()
                || decision.decision_ref.trim().is_empty()
                || decision.question.trim().is_empty()
                || decision.why_human.trim().is_empty()
                || decision.evidence_refs.is_empty()
                || pending.iter().any(|record| {
                    record.unit_decision_basis.as_ref().is_some_and(|basis| {
                        basis.workflow_unit_ref == decision.basis.workflow_unit_ref
                    })
                })
            {
                return Err(invalid(
                    "unit decision needs an attributed question, evidence and one unresolved owner",
                ));
            }
            native
                .build
                .insert_unit_decision(HumanRequestRecord {
                    run_ref: view.run.reference().clone(),
                    human_request_ref: decision.human_request_ref.clone(),
                    decision_ref: decision.decision_ref.clone(),
                    question: decision.question.clone(),
                    why_human: decision.why_human.clone(),
                    blocked_execution_refs: vec![decision.basis.execution_ref.clone()],
                    evidence_refs: decision.evidence_refs.iter().cloned().collect(),
                    unit_decision_basis: Some(decision.basis.clone()),
                    unit_decision_response: None,
                    unit_decision_retirement: None,
                })
                .map_err(|error| invalid(&error.to_string()))?;
            if view.run.lifecycle() == RunLifecycle::Active
                && !has_unblocked_frontier(
                    &engine,
                    &pending
                        .iter()
                        .filter_map(|record| {
                            record
                                .unit_decision_basis
                                .as_ref()
                                .map(|basis| basis.workflow_unit_ref.clone())
                        })
                        .chain(std::iter::once(decision.basis.workflow_unit_ref.clone()))
                        .collect(),
                )
            {
                let command = RunLifecycleCommand {
                    command_id: format!("decision-wait:{}", decision.human_request_ref),
                    expected_revision: view.run.revision(),
                    lifecycle: RunLifecycle::WaitingHuman,
                };
                view.run
                    .apply_lifecycle_command(&view.run.mutation_authority(), command)
                    .map_err(|error| invalid(&error.to_string()))?;
            }
            Ok((
                "request-unit-decision".into(),
                vec![decision.basis.attempt_ref.clone()],
                None,
            ))
        }
        FactoryAttemptOperation::RetireUnitDecision {
            human_request_ref,
            reason,
            replacement_basis_refs,
        } => {
            let decision = pending
                .iter()
                .find(|record| &record.human_request_ref == human_request_ref)
                .ok_or_else(|| invalid("unresolved native unit decision is absent"))?;
            let basis = decision
                .unit_decision_basis
                .as_ref()
                .expect("pending basis");
            if (validate_decision_basis(view, &engine, basis).is_ok() && supersession.is_none())
                || reason.trim().is_empty()
                || replacement_basis_refs.is_empty()
            {
                return Err(invalid("only an obsolete affected-work basis may be retired, with exact replacement evidence"));
            }
            if supersession
                .as_ref()
                .is_some_and(|refs| !replacement_basis_refs.is_superset(refs))
            {
                return Err(invalid("superseded decision retirement must retain the canonical continuation, successor Run and exact source basis"));
            }
            native
                .build
                .retire_unit_decision(
                    human_request_ref,
                    crate::run_lifecycle::UnitDecisionRetirement {
                        reason: reason.clone(),
                        replacement_basis_refs: replacement_basis_refs.clone(),
                        observed_provider_revision: view.revision,
                    },
                )
                .map_err(|error| invalid(&error.to_string()))?;
            if pending.len() == 1 && view.run.lifecycle() == RunLifecycle::WaitingHuman {
                let command = RunLifecycleCommand {
                    command_id: format!("decision-retire:{}", human_request_ref),
                    expected_revision: view.run.revision(),
                    lifecycle: RunLifecycle::Active,
                };
                view.run
                    .apply_lifecycle_command(&view.run.mutation_authority(), command)
                    .map_err(|error| invalid(&error.to_string()))?;
            }
            Ok((
                "retire-unit-decision".into(),
                vec![basis.attempt_ref.clone()],
                None,
            ))
        }
        FactoryAttemptOperation::ResolveUnitDecision {
            human_request_ref,
            response,
        } => {
            native_admission.ok_or_else(|| invalid("unit decision requires the actual native reviewed answer; DesktopHuman and channel strings are not proof"))?
                .validate(request).map_err(|error| invalid(&error))?;
            let decision = pending
                .iter()
                .find(|record| &record.human_request_ref == human_request_ref)
                .ok_or_else(|| invalid("unresolved native unit decision is absent"))?;
            let basis = decision
                .unit_decision_basis
                .as_ref()
                .expect("pending basis");
            validate_decision_basis(view, &engine, basis)?;
            validate_decision_response(basis, response)?;
            native
                .build
                .resolve_unit_decision(human_request_ref, response.clone())
                .map_err(|error| invalid(&error.to_string()))?;
            let lifecycle_basis = if response.outcome == UnitDecisionOutcome::Cancel {
                apply_operation(
                    view,
                    FactoryAttemptOperation::RequestCancellation {
                        attempt_ref: basis.attempt_ref.clone(),
                    },
                )?;
                Some(view.run.clone())
            } else {
                None
            };
            if pending.len() == 1 && view.run.lifecycle() == RunLifecycle::WaitingHuman {
                let command = RunLifecycleCommand {
                    command_id: format!("decision-resume:{}", response.response_ref),
                    expected_revision: view.run.revision(),
                    lifecycle: RunLifecycle::Active,
                };
                view.run
                    .apply_lifecycle_command(&view.run.mutation_authority(), command)
                    .map_err(|error| invalid(&error.to_string()))?;
            }
            Ok((
                "resolve-unit-decision".into(),
                vec![basis.attempt_ref.clone()],
                lifecycle_basis,
            ))
        }
        operation => {
            let addressed_attempt = match operation {
                FactoryAttemptOperation::BindDispatch { attempt_ref, .. }
                | FactoryAttemptOperation::ReturnArtifact { attempt_ref, .. }
                | FactoryAttemptOperation::IncorporateLateResult { attempt_ref }
                | FactoryAttemptOperation::RegisterIndependentReview { attempt_ref, .. }
                | FactoryAttemptOperation::Synthesize { attempt_ref, .. } => Some(attempt_ref),
                _ => None,
            };
            let historical_intake = supersession.is_some()
                && matches!(operation, FactoryAttemptOperation::ReturnArtifact { .. });
            if !historical_intake
                && addressed_attempt.is_some_and(|attempt| {
                    pending.iter().any(|record| {
                        record
                            .unit_decision_basis
                            .as_ref()
                            .is_some_and(|basis| &basis.attempt_ref == attempt)
                    })
                })
            {
                return Err(invalid(
                    "the affected unit awaits its canonical human decision",
                ));
            }
            if matches!(
                operation,
                FactoryAttemptOperation::StartSerial { .. }
                    | FactoryAttemptOperation::StartFork { .. }
                    | FactoryAttemptOperation::Retry { .. }
                    | FactoryAttemptOperation::AdvanceSubject { .. }
            ) && matches!(
                view.run.lifecycle(),
                RunLifecycle::Suspended
                    | RunLifecycle::Finishing
                    | RunLifecycle::Finished
                    | RunLifecycle::Aborted
                    | RunLifecycle::Archived
            ) {
                return Err(invalid(
                    "this Run lifecycle does not admit new work or a new candidate",
                ));
            }
            let result = apply_operation_with_source_currency(
                view,
                operation.clone(),
                supersession.is_none(),
            )?;
            let mut lifecycle_basis = None;
            if !pending.is_empty()
                && matches!(
                    view.run.lifecycle(),
                    RunLifecycle::Active | RunLifecycle::WaitingHuman
                )
            {
                let workflow = crate::workflow::compile_workflow(view.workflow_source.clone())?;
                let current = view.snapshot.restore(workflow, view.run.clone())?;
                let lifecycle = if has_unblocked_frontier(&current, &blocked) {
                    RunLifecycle::Active
                } else {
                    RunLifecycle::WaitingHuman
                };
                if view.run.lifecycle() != lifecycle {
                    lifecycle_basis = Some(view.run.clone());
                    view.run
                        .apply_lifecycle_command(
                            &view.run.mutation_authority(),
                            RunLifecycleCommand {
                                command_id: format!(
                                    "decision-frontier:{}:{}",
                                    request.projection_ref,
                                    view.run.revision().get()
                                ),
                                expected_revision: view.run.revision(),
                                lifecycle,
                            },
                        )
                        .map_err(|error| invalid(&error.to_string()))?;
                }
            }
            Ok((result.0, result.1, lifecycle_basis))
        }
    }
}

fn has_unblocked_frontier(
    engine: &crate::orchestration::ExecutableOrchestration,
    blocked: &BTreeSet<crate::core::run::WorkflowUnitRef>,
) -> bool {
    engine.workflow().units.values().any(|unit| {
        if blocked.contains(&unit.reference) {
            return false;
        }
        if let Some(leg) = engine.leg(&unit.reference) {
            return leg.status != LegStatus::Returned;
        }
        unit.dependencies
            .iter()
            .all(|dependency| engine.is_current_return(dependency))
            && engine
                .workflow()
                .barriers
                .iter()
                .filter(|barrier| barrier.releases.contains(&unit.reference))
                .all(|barrier| {
                    engine.barrier_reading(&barrier.key).is_ok_and(|reading| {
                        reading.state == crate::orchestration::BarrierState::Complete
                    })
                })
    })
}
