//! Bounded bidirectional source/attempt navigation over existing native reads.
use super::*;
use crate::attempt_runtime::{FactoryAttemptRecord, OwnerOperationReceipt};
use crate::core::run::RunRef;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) fn summary(source: &WorkflowSource) -> Value {
    let authored = source.source.authoring.as_ref().map(|a| {
        let modules = a
            .modules
            .iter()
            .map(|(key, m)| {
                (
                    key.clone(),
                    json!({"path":m.path,"digest":m.digest,"bytes":m.bytes}),
                )
            })
            .collect::<BTreeMap<_, _>>();
        json!({"contract":a.contract,"compiler":a.compiler,"compilerDigest":a.compiler_digest,
            "entry":a.entry,"revision":a.revision,"bundleDigest":a.bundle_digest,"modules":modules,
            "fieldLocationCount":a.locations.len(),"successorOf":a.successor_of,"domainAdapters":a.adapters})
    });
    json!({"ref":source.source.reference,"revision":source.source.revision,"semanticDigest":source.source.digest,
        "temporalRef":source.source.temporal_ref,"flowRef":source.source.flow_ref,"authoring":authored})
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    revision: u64,
    run_ref: RunRef,
    unit: Option<String>,
    attempt: Option<String>,
    offset: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocateCursor {
    revision: u64,
    reference: String,
    offset: usize,
}
fn failure(e: impl fmt::Display) -> Diagnostic {
    error("workflow.inspection", e.to_string())
}
fn bound(value: Value) -> Result<Value, Diagnostic> {
    if serde_json::to_vec(&value).map_err(failure)?.len() > MAX_BUNDLE_BYTES {
        return Err(error("workflow.inspection_limit","Reading exceeds 2 MiB; select one unit/attempt or a smaller page. Detailed payloads remain available through their native owner."));
    }
    Ok(value)
}
fn range(limit: usize) -> Result<(), Diagnostic> {
    if !(1..=100).contains(&limit) {
        Err(failure("limit must be 1..100"))
    } else {
        Ok(())
    }
}
fn source_location<'a>(source: &'a WorkflowSource, key: &str) -> Option<&'a SourceLocation> {
    let index = source.units.iter().position(|u| u.key == key)?;
    source
        .source
        .authoring
        .as_ref()?
        .locations
        .get(&format!("/units/{index}/key"))
}
fn receipt_summary(r: &OwnerOperationReceipt) -> Value {
    json!({"ownerRef":r.owner_ref,"contract":r.contract,"operationRef":r.operation_ref,
        "receiptRef":r.receipt_ref,"sourceRevision":r.source_revision,"phase":r.phase,
        "evidenceRefCount":r.evidence_refs.len(),"partialEffectRefCount":r.partial_effect_refs.len(),
        "payloadDisclosure":"withheld-use-native-owner-read"})
}

/// Each page carries bounded units, attempts, barriers and telemetry references.
/// A stale cursor refuses rather than silently changing historical selection.
pub fn inspect(
    path: &Path,
    run: &RunRef,
    unit: Option<String>,
    attempt: Option<String>,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Value, Diagnostic> {
    range(limit)?;
    let (state, reading) = crate::attempt_task::coherent_read(path, run).map_err(failure)?;
    let source = state
        .attempt_states
        .get(run)
        .ok_or_else(|| failure("Run has no native workflow attachment"))?
        .workflow_source();
    let workflow = compile_workflow(source.clone()).map_err(failure)?;
    let offset = if let Some(cursor) = cursor {
        let c: Cursor = serde_json::from_str(cursor).map_err(failure)?;
        if c.revision != reading.revision
            || c.run_ref != *run
            || c.unit != unit
            || c.attempt != attempt
        {
            return Err(error("workflow.stale_cursor","Snapshot or selection changed; reselect the same stable unit/attempt with a fresh cursor"));
        }
        c.offset
    } else {
        0
    };
    let selected_attempt = attempt
        .as_ref()
        .map(|r| {
            reading
                .attempts
                .iter()
                .find(|a| &a.attempt_ref == r)
                .ok_or_else(|| failure("Selected attempt does not belong to this Run"))
        })
        .transpose()?;
    let selected_unit = unit
        .as_ref()
        .map(|r| {
            workflow
                .units
                .values()
                .find(|u| u.key == *r || u.reference.to_string() == *r)
                .ok_or_else(|| failure("Selected unit does not belong to this source basis"))
        })
        .transpose()?;
    if let (Some(a), Some(u)) = (selected_attempt, selected_unit) {
        if a.workflow_unit_ref != u.reference {
            return Err(failure(
                "Selected source unit and attempt are not correlated",
            ));
        }
    }
    let unit_ref = selected_unit
        .map(|u| &u.reference)
        .or_else(|| selected_attempt.map(|a| &a.workflow_unit_ref));
    let attempts = reading
        .attempts
        .iter()
        .filter(|a| {
            unit_ref.is_none_or(|u| u == &a.workflow_unit_ref)
                && attempt.as_ref().is_none_or(|r| r == &a.attempt_ref)
        })
        .collect::<Vec<_>>();
    let all_units = workflow
        .units
        .values()
        .filter(|u| unit_ref.is_none_or(|r| r == &u.reference))
        .collect::<Vec<_>>();
    let correlations = state
        .execution_correlations
        .iter()
        .filter(|c| {
            c.run_ref == *run
                && unit_ref.is_none_or(|u| u == &c.workflow_unit_ref)
                && selected_attempt
                    .is_none_or(|a| a.execution_ref.as_ref() == Some(&c.execution_ref))
        })
        .collect::<Vec<_>>();
    let all_barriers = workflow
        .barriers
        .iter()
        .filter(|b| unit_ref.is_none_or(|u| b.waits_for.contains(u) || b.releases.contains(u)))
        .collect::<Vec<_>>();
    let total = attempts
        .len()
        .max(all_units.len())
        .max(correlations.len())
        .max(all_barriers.len());
    if offset > total {
        return Err(failure("Cursor offset exceeds the native reading"));
    }
    let rows=attempts.iter().skip(offset).take(limit).map(|a|{
        let leg=reading.legs.get(&a.workflow_unit_ref);
        let execution=a.execution_ref.as_ref().unwrap_or(&a.reserved_execution_ref);
        let current=leg.is_some_and(|l|&l.execution_ref==execution);
        let status=if current {leg.map(|l|l.status)} else {leg.and_then(|l|l.attempts.iter().find(|p|&p.execution_ref==execution)).map(|p|p.status)};
        let receipts=a.dispatch.iter().chain(a.observations.iter()).take(25).map(receipt_summary).collect::<Vec<_>>();
        json!({"attemptRef":a.attempt_ref,"taskRef":a.task_ref,"workflowUnitRef":a.workflow_unit_ref,
            "executionRef":a.execution_ref,"reservedExecutionRef":a.reserved_execution_ref,"currentAttempt":current,"status":status,
            "sourceRef":reading.workflow_source_ref,"sourceRevision":reading.workflow_source_revision,"sourceDigest":reading.workflow_source_digest,
            "participant":a.disposition.participant,"body":a.disposition.body,"bodyStanding":"resolved-arrangement-not-provider-observation",
            "selectedInputs":a.disposition.selected_inputs,"contextRefs":a.disposition.context_refs,"contextStanding":"configured-not-proof-of-loading",
            "praxisRefs":a.disposition.praxis_refs,"capabilityRefs":a.disposition.capability_refs,
            "ownerObservations":receipts,"totalOwnerObservations":a.observations.len()+usize::from(a.dispatch.is_some()),
            "verification":a.verifications.iter().take(25).collect::<Vec<_>>(),"totalVerifications":a.verifications.len(),
            "trackingFactCount":a.tracking.len(),"reresolutionCount":a.reresolutions.len(),
            "failureEvidenceRefs":a.failure_evidence_refs,"return":a.readable_return,
            "nativeTaskRead":{"command":"attempt.task","runRef":run,"taskRef":a.task_ref},
            "nativeReturnRead":{"command":"attempt.return","runRef":run,"attemptRef":a.attempt_ref}})
    }).collect::<Vec<_>>();
    let units = all_units
        .iter()
        .skip(offset)
        .take(limit)
        .map(|u| {
            let mut value = serde_json::to_value(
                state
                    .workflow_unit_reading(&u.reference, Some(run))
                    .map_err(failure)?,
            )
            .map_err(failure)?;
            value["sourceLocation"] = json!(source_location(source, &u.key));
            // The authored C-prime requirement travels with the compiled unit;
            // it is intent, not an observation of how the attempt was conducted.
            value["composition"] = json!(u.composition);
            Ok(value)
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    let legs=all_units.iter().skip(offset).take(limit).map(|u|{
        let leg=reading.legs.get(&u.reference);
        (u.reference.to_string(),json!({"status":leg.map(|l|l.status),"executionRef":leg.map(|l|&l.execution_ref),
            "failureReason":leg.and_then(|l|l.failure_reason.as_ref()),"requiredVerification":u.verification_obligations,
            "standing":if leg.is_some(){"native-leg"}else{"not-started"}}))
    }).collect::<BTreeMap<_,_>>();
    let barriers=all_barriers.iter().skip(offset).take(limit).map(|b|{
        let missing=b.waits_for.iter().filter(|u|!reading.legs.get(*u).is_some_and(|l|l.status==crate::orchestration::LegStatus::Returned)).collect::<Vec<_>>();
        json!({"key":b.key,"waitsFor":b.waits_for,"releases":b.releases,"notReturned":missing,"complete":missing.is_empty()})
    }).collect::<Vec<_>>();
    let telemetry=correlations.iter().skip(offset).take(limit).map(|c|json!({"telemetryRef":c.telemetry_ref,
        "workflowUnitRef":c.workflow_unit_ref,"executionRef":c.execution_ref,"command":"development.execution-telemetry"})).collect::<Vec<_>>();
    let next_offset = offset.saturating_add(limit);
    let next = (next_offset < total).then(|| Cursor {
        revision: reading.revision,
        run_ref: run.clone(),
        unit: unit.clone(),
        attempt: attempt.clone(),
        offset: next_offset,
    });
    let native_run = state.run_reading(run).map_err(failure)?;
    bound(
        json!({"contract":"factory.workflow-inspection/v1","revision":reading.revision,"runRef":run,
        "source":summary(source),"sourceCurrent":reading.source_current,
        "run":{"runRef":run,"revision":native_run.revision,"projectRef":native_run.project_ref,
            "owningJourneyRefs":native_run.owning_journey_refs,"lifecycle":native_run.lifecycle,
            "destination":native_run.destination,"thoughtAvailable":native_run.thought_available,"actions":native_run.actions,
            "nativeDetailCommand":"development.run","fullSssfCommand":"development.build"},
        "units":units,"legs":legs,"barriers":barriers,"attempts":rows,"telemetry":telemetry,
        "totalAttempts":attempts.len(),"totalUnits":all_units.len(),"totalBarriers":all_barriers.len(),
        "totalTelemetry":correlations.len(),"nextCursor":next,
        "disclosure":"source bodies and transport payloads withheld; bounded first-page observation summaries disclose totals and native detail reads; references never assert missing telemetry"}),
    )
}

fn attempt_relations(a: &FactoryAttemptRecord, reference: &str) -> Vec<&'static str> {
    let mut matches = Vec::new();
    if a.attempt_ref == reference {
        matches.push("attempt");
    }
    if a.task_ref == reference {
        matches.push("task");
    }
    if a.execution_ref.as_deref() == Some(reference) || a.reserved_execution_ref == reference {
        matches.push("execution");
    }
    let p = &a.disposition.participant;
    let b = &a.disposition.body;
    if [&p.agent_ref, &p.agency_ref, &p.world_binding_ref].contains(&&reference.to_owned()) {
        matches.push("resolved-participant");
    }
    if [
        &b.agent_session_ref,
        &b.session_space_ref,
        &b.harness_composition_ref,
    ]
    .contains(&&reference.to_owned())
    {
        matches.push("resolved-body");
    }
    for r in a.dispatch.iter().chain(&a.observations) {
        if r.receipt_ref == reference
            || r.operation_ref == reference
            || r.evidence_refs.contains(reference)
            || r.partial_effect_refs.contains(reference)
        {
            matches.push("native-owner-observation");
        }
    }
    if a.verifications
        .iter()
        .any(|v| v.verification_ref == reference || v.evidence_refs.contains(reference))
    {
        matches.push("verification");
    }
    if a.readable_return.as_ref().is_some_and(|r| {
        r.return_ref == reference
            || r.artifact_refs.contains(reference)
            || r.evidence_refs.contains(reference)
            || r.receiving_ref.as_deref() == Some(reference)
            || r.archive_refs.contains(reference)
    }) {
        matches.push("return");
    }
    matches.sort();
    matches.dedup();
    matches
}

/// Exact native references only. A shared reference may yield several attributed
/// attempts; ambiguity is retained instead of guessing from a display name.
pub fn locate(
    path: &Path,
    reference: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Value, Diagnostic> {
    range(limit)?;
    if reference.trim() != reference || reference.is_empty() || reference.len() > 2048 {
        return Err(failure("Expected one bounded exact native reference"));
    }
    let state =
        crate::project_development_store::read_developmental_state(path).map_err(failure)?;
    let revision = state.build.revision().get();
    let offset = if let Some(cursor) = cursor {
        let c: LocateCursor = serde_json::from_str(cursor).map_err(failure)?;
        if c.revision != revision || c.reference != reference {
            return Err(error(
                "workflow.stale_cursor",
                "Native reference or revision changed; reselect explicitly",
            ));
        }
        c.offset
    } else {
        0
    };
    let mut hits = Vec::new();
    let mut total_matches = 0;
    let mut push_hit = |value: Value| {
        if total_matches >= offset && hits.len() < limit {
            hits.push(value);
        }
        total_matches += 1;
    };
    for (run, native) in &state.attempt_states {
        let source = native.workflow_source();
        let workflow = compile_workflow(source.clone()).map_err(failure)?;
        for u in workflow.units.values() {
            let unit_match = u.reference.to_string() == reference;
            let source_match = source.source.reference.to_string() == reference;
            let mut found = false;
            for a in native
                .attempts()
                .values()
                .filter(|a| a.workflow_unit_ref == u.reference)
            {
                let mut relations = attempt_relations(a, reference);
                if unit_match {
                    relations.push("workflow-unit");
                }
                if source_match {
                    relations.push("workflow-source");
                }
                for c in state.execution_correlations.iter().filter(|c| {
                    c.run_ref == *run
                        && c.workflow_unit_ref == u.reference
                        && a.execution_ref.as_ref() == Some(&c.execution_ref)
                }) {
                    if c.correlation_ref.to_string() == reference
                        || c.telemetry_ref.to_string() == reference
                        || c.temporal
                            .activity_refs
                            .iter()
                            .any(|r| r.reference == reference)
                    {
                        relations.push("owner-telemetry");
                    }
                }
                if !relations.is_empty() {
                    found = true;
                    push_hit(
                        json!({"runRef":run,"workflowUnitRef":u.reference,"unitKey":u.key,"attemptRef":a.attempt_ref,
                        "executionRef":a.execution_ref,"relations":relations,"source":summary(source),"sourceLocation":source_location(source,&u.key),
                        "inspect":{"command":"workflow.inspect","runRef":run,"attemptRef":a.attempt_ref}}),
                    );
                }
            }
            if !found && (unit_match || source_match) {
                push_hit(
                    json!({"runRef":run,"workflowUnitRef":u.reference,"unitKey":u.key,"attemptRef":null,
                "relations":[if unit_match{"workflow-unit"}else{"workflow-source"}],"source":summary(source),"sourceLocation":source_location(source,&u.key),
                "inspect":{"command":"workflow.inspect","runRef":run,"unitRef":u.reference}}),
                );
            }
        }
    }
    if offset > total_matches {
        return Err(failure("Cursor exceeds reference matches"));
    }
    let next = offset.saturating_add(limit);
    let cursor = (next < total_matches).then(|| LocateCursor {
        revision,
        reference: reference.into(),
        offset: next,
    });
    bound(
        json!({"contract":"factory.workflow-location/v1","revision":revision,"reference":reference,
        "standing":if total_matches==0{"unrecorded"}else{"native-correlated"},"totalMatches":total_matches,
        "matches":hits,"nextCursor":cursor,
        "disclosure":"only recorded source/unit/attempt/owner/Return references; no payload scraping or inferred external ancestry"}),
    )
}
