//! Source-declared predecessor material on the existing native workflow and
//! attempt. A dependency alone does not select or deliver a predecessor result.
use crate::core::run::WorkflowUnitRef;
use crate::orchestration::{ExecutableOrchestration, LegStatus, ReturnedArtifact};
use crate::workflow::CompiledWorkflowUnit;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowInputSource {
    pub predecessor: String,
    pub receiving_context_ref: String,
}
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompiledWorkflowInput {
    pub predecessor: WorkflowUnitRef,
    pub receiving_context_ref: String,
}
/// An exact selection from an already returned native leg. The full original
/// artifact records make the receiving packet useful without losing attribution.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedWorkflowInput {
    pub predecessor: WorkflowUnitRef,
    pub execution_ref: String,
    pub receiving_context_ref: String,
    pub artifacts: Vec<ReturnedArtifact>,
}

pub fn validate_selected_inputs(
    engine: &ExecutableOrchestration,
    unit: &CompiledWorkflowUnit,
    selected: &[SelectedWorkflowInput],
    context_refs: &BTreeSet<String>,
) -> Result<(), String> {
    if selected.len() != unit.inputs.len() || selected.len() > 128 {
        return Err("every authored predecessor input needs one exact selection; extra or missing inputs are refused".into());
    }
    let mut seen = BTreeSet::new();
    for input in selected {
        if !seen.insert((&input.predecessor, &input.receiving_context_ref))
            || !unit.inputs.iter().any(|declared| {
                declared.predecessor == input.predecessor
                    && declared.receiving_context_ref == input.receiving_context_ref
            })
            || !context_refs.contains(&input.receiving_context_ref)
        {
            return Err("predecessor selection changes or duplicates the authored input route or lacks its explicit receiving Context".into());
        }
        let leg = engine
            .leg(&input.predecessor)
            .ok_or("required predecessor has not started")?;
        if leg.status != LegStatus::Returned || leg.execution_ref != input.execution_ref {
            return Err("required predecessor has not returned on this exact current execution; failed, historical and late results cannot become current inputs".into());
        }
        if input.artifacts.is_empty() || input.artifacts.len() > 128 {
            return Err("predecessor input needs 1..128 selected native artifacts".into());
        }
        let mut artifacts = BTreeSet::new();
        for artifact in &input.artifacts {
            if !artifacts.insert(&artifact.artifact_ref)
                || !leg.artifacts.contains(artifact)
                || artifact.producing_execution_ref != input.execution_ref
                || engine.current_subject_revision(&artifact.subject_ref)
                    != Some(artifact.subject_revision.as_str())
            {
                return Err("selected artifact is duplicated, foreign, edited, or stale against the current subject; reselect the actual native Return".into());
            }
        }
    }
    Ok(())
}

/// Source-scoped candidate reading. This is not automatic selection or admission;
/// the caller chooses artifact IDs and a later native start/send rechecks them.
pub fn candidates(
    engine: &ExecutableOrchestration,
    unit: &CompiledWorkflowUnit,
) -> serde_json::Value {
    use serde_json::json;
    let routes=unit.inputs.iter().map(|input| {
        let leg=engine.leg(&input.predecessor);
        let artifacts=leg.map(|l|l.artifacts.iter().map(|a|json!({
            "artifact":a,
            "current": l.status==LegStatus::Returned
                && a.producing_execution_ref==l.execution_ref
                && engine.current_subject_revision(&a.subject_ref)==Some(a.subject_revision.as_str())
        })).collect::<Vec<_>>()).unwrap_or_default();
        json!({"predecessor":input.predecessor,"receivingContextRef":input.receiving_context_ref,
            "executionRef":leg.map(|l|&l.execution_ref),"status":leg.map(|l|l.status),"artifacts":artifacts})
    }).collect::<Vec<_>>();
    json!({"unitRef":unit.reference,"unitKey":unit.key,"routes":routes,"selection":"not-performed"})
}
