//! Factory-owned demand, disposition and P5 observation boundary for AIKit model selection.
//!
//! Factory owns why developmental work needs execution and the Run-scoped evidence
//! produced afterwards. It does not own AIKit's Model/resource/provider registry.
//! AIKit selection is consumed as an opaque resource reference plus an inspectable
//! ranking receipt; Run identity remains Factory truth.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EXECUTION_INTELLIGENCE_INTEROP_VERSION: &str = "factory.execution-intelligence/v1";
pub const AIKIT_MODEL_ROSTER_VERSION: &str = "aikit.model-roster/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionDemand {
    pub project_ref: String,
    pub run_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_unit_ref: Option<String>,
    pub agency_ref: Option<String>,
    pub profile_ref: Option<String>,
    pub use_type: String,
    #[serde(default)]
    pub required_capabilities: BTreeSet<String>,
    #[serde(default)]
    pub required_modalities: BTreeSet<String>,
    #[serde(default)]
    pub required_actions: BTreeSet<String>,
    #[serde(default)]
    pub required_tools: BTreeSet<String>,
    #[serde(default)]
    pub context_characteristics: BTreeSet<String>,
    #[serde(default)]
    pub independence_from: BTreeSet<String>,
    pub cost_ceiling_usd: Option<f64>,
    pub latency_preference_ms: Option<u64>,
    pub requires_local_materialisation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AikitModelRosterSelection {
    /// Must name the AIKit application read-model version, not a Factory registry.
    pub roster_version: String,
    pub model_ref: String,
    pub provider_ref: String,
    pub ranking_policy: String,
    /// Complete AIKit-produced explanation snapshot for historical reconstruction.
    pub ranking_explanation: Value,
    #[serde(default)]
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionDisposition {
    pub schema_version: String,
    pub demand: ExecutionDemand,
    pub selection: AikitModelRosterSelection,
    pub decided_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExactExecutionSpend {
    pub amount: f64,
    pub currency: String,
    pub provider_receipt_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FitnessObservationScope {
    pub project_ref: String,
    pub use_type: String,
    pub agency_ref: Option<String>,
    pub profile_ref: Option<String>,
    pub harness_composition_ref: Option<String>,
    #[serde(default)]
    pub capability_body: BTreeSet<String>,
    #[serde(default)]
    pub context_characteristics: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct P5ModelFitnessObservation {
    /// Factory-owned Run provenance. AIKit may consume it but never owns this truth.
    pub run_ref: String,
    /// Opaque reference to AIKit's canonical Model identity.
    pub model_ref: String,
    pub provider_ref: String,
    pub provider_revision: Option<String>,
    pub scope: FitnessObservationScope,
    pub fitness: f64,
    pub exact_spend: Option<ExactExecutionSpend>,
    pub observed_at: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AikitFitnessObservationInput {
    pub source_system: String,
    pub source_run_ref: String,
    pub model_ref: String,
    pub provider_ref: String,
    pub provider_revision: Option<String>,
    pub project_ref: String,
    pub profile_ref: Option<String>,
    pub agency_ref: Option<String>,
    pub use_type: String,
    pub harness_composition_ref: Option<String>,
    pub capability_body: BTreeSet<String>,
    pub context_characteristics: BTreeSet<String>,
    pub fitness: f64,
    pub observed_at: String,
    #[serde(default)]
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionInteropError {
    WrongRosterVersion(String),
    EmptyModelRef,
    EmptyRunRef,
    InvalidWorkflowUnitRef(String),
}

pub fn accept_aikit_selection(
    demand: ExecutionDemand,
    selection: AikitModelRosterSelection,
    decided_at: impl Into<String>,
) -> Result<ExecutionDisposition, ExecutionInteropError> {
    if demand.run_ref.trim().is_empty() {
        return Err(ExecutionInteropError::EmptyRunRef);
    }
    if let Some(reference) = &demand.workflow_unit_ref {
        if reference
            .parse::<crate::core::run::WorkflowUnitRef>()
            .is_err()
        {
            return Err(ExecutionInteropError::InvalidWorkflowUnitRef(
                reference.clone(),
            ));
        }
    }
    if selection.model_ref.trim().is_empty() {
        return Err(ExecutionInteropError::EmptyModelRef);
    }
    if selection.roster_version != AIKIT_MODEL_ROSTER_VERSION {
        return Err(ExecutionInteropError::WrongRosterVersion(
            selection.roster_version,
        ));
    }
    Ok(ExecutionDisposition {
        schema_version: EXECUTION_INTELLIGENCE_INTEROP_VERSION.to_string(),
        demand,
        selection,
        decided_at: decided_at.into(),
    })
}

/// Project a Factory P5 observation into AIKit's learned-fitness intake boundary.
/// Exact spend is intentionally not converted into fitness; AIKit receives spend
/// through its separate spend/telemetry semantics.
pub fn fitness_for_aikit(observation: &P5ModelFitnessObservation) -> AikitFitnessObservationInput {
    AikitFitnessObservationInput {
        source_system: "factory".to_string(),
        source_run_ref: observation.run_ref.clone(),
        model_ref: observation.model_ref.clone(),
        provider_ref: observation.provider_ref.clone(),
        provider_revision: observation.provider_revision.clone(),
        project_ref: observation.scope.project_ref.clone(),
        profile_ref: observation.scope.profile_ref.clone(),
        agency_ref: observation.scope.agency_ref.clone(),
        use_type: observation.scope.use_type.clone(),
        harness_composition_ref: observation.scope.harness_composition_ref.clone(),
        capability_body: observation.scope.capability_body.clone(),
        context_characteristics: observation.scope.context_characteristics.clone(),
        fitness: observation.fitness,
        observed_at: observation.observed_at.clone(),
        provenance: observation.evidence_refs.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn demand() -> ExecutionDemand {
        ExecutionDemand {
            project_ref: "project:factory".into(),
            run_ref: "run:184".into(),
            workflow_unit_ref: None,
            agency_ref: Some("agency:mahamaya".into()),
            profile_ref: Some("profile:rust".into()),
            use_type: "coding".into(),
            required_capabilities: BTreeSet::from(["reasoning".into(), "structured-output".into()]),
            required_modalities: BTreeSet::from(["text".into()]),
            required_actions: BTreeSet::from(["apply-patch".into()]),
            required_tools: BTreeSet::from(["shell".into()]),
            context_characteristics: BTreeSet::from(["rust".into(), "large-repository".into()]),
            independence_from: BTreeSet::new(),
            cost_ceiling_usd: Some(30.0),
            latency_preference_ms: Some(30_000),
            requires_local_materialisation: false,
        }
    }

    fn selection(provider: &str) -> AikitModelRosterSelection {
        AikitModelRosterSelection {
            roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
            model_ref: "model:gpt-5.4".into(),
            provider_ref: provider.into(),
            ranking_policy: "BALANCED".into(),
            ranking_explanation: json!({
                "eligible": true,
                "hard_gates": ["available", "authorised", "contract-compatible"],
                "components": [{"name": "task-fit", "value": 0.91, "weight": 0.30}],
                "missing_data": []
            }),
            provenance: vec!["aikit:model-roster:request-42".into()],
        }
    }

    #[test]
    fn factory_consumes_selection_without_owning_model_registry() {
        let disposition = accept_aikit_selection(
            demand(),
            selection("provider:openai"),
            "2026-08-17T10:00:00+01:00",
        )
        .unwrap();
        assert_eq!(disposition.selection.model_ref, "model:gpt-5.4");
        assert_eq!(disposition.demand.run_ref, "run:184");
        assert_eq!(disposition.selection.ranking_explanation["eligible"], true);
    }

    #[test]
    fn provider_replacement_preserves_model_and_run_identity() {
        let first = accept_aikit_selection(demand(), selection("provider:openai-a"), "t1").unwrap();
        let second =
            accept_aikit_selection(demand(), selection("provider:openai-b"), "t2").unwrap();
        assert_eq!(first.selection.model_ref, second.selection.model_ref);
        assert_eq!(first.demand.run_ref, second.demand.run_ref);
        assert_ne!(first.selection.provider_ref, second.selection.provider_ref);
    }

    #[test]
    fn p5_fitness_is_scope_attributed_and_run_truth_stays_factory_owned() {
        let observation = P5ModelFitnessObservation {
            run_ref: "run:184".into(),
            model_ref: "model:gpt-5.4".into(),
            provider_ref: "provider:openai".into(),
            provider_revision: Some("gpt-5.4-2026-03-05".into()),
            scope: FitnessObservationScope {
                project_ref: "project:factory".into(),
                use_type: "coding".into(),
                agency_ref: Some("agency:mahamaya".into()),
                profile_ref: Some("profile:rust".into()),
                harness_composition_ref: Some("pi+factory-tools/v3".into()),
                capability_body: BTreeSet::from(["reasoning".into(), "apply-patch".into()]),
                context_characteristics: BTreeSet::from(["rust".into()]),
            },
            fitness: 0.92,
            exact_spend: Some(ExactExecutionSpend {
                amount: 3.27,
                currency: "USD".into(),
                provider_receipt_ref: Some("receipt:abc".into()),
            }),
            observed_at: "2026-08-17T10:05:00+01:00".into(),
            evidence_refs: vec!["gate:cargo-test".into(), "application:recognition".into()],
        };
        let input = fitness_for_aikit(&observation);
        assert_eq!(input.source_system, "factory");
        assert_eq!(input.source_run_ref, "run:184");
        assert_eq!(input.use_type, "coding");
        assert_eq!(input.profile_ref.as_deref(), Some("profile:rust"));
        assert_eq!(input.fitness, 0.92);
        assert_eq!(observation.exact_spend.as_ref().unwrap().amount, 3.27);
    }

    #[test]
    fn catalog_price_and_exact_spend_are_not_conflated() {
        let observation = P5ModelFitnessObservation {
            run_ref: "run:x".into(),
            model_ref: "model:x".into(),
            provider_ref: "provider:x".into(),
            provider_revision: None,
            scope: FitnessObservationScope {
                project_ref: "project:x".into(),
                use_type: "review".into(),
                ..Default::default()
            },
            fitness: 0.8,
            exact_spend: Some(ExactExecutionSpend {
                amount: 7.0,
                currency: "USD".into(),
                provider_receipt_ref: None,
            }),
            observed_at: "now".into(),
            evidence_refs: vec![],
        };
        let input = fitness_for_aikit(&observation);
        assert_eq!(input.fitness, 0.8);
        assert!(!serde_json::to_value(input)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("exact_spend"));
    }
}
