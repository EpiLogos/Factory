use crate::build::{
    FactoryBuildError, FactoryBuildSelection, FactoryBuildState, FACTORY_NATIVE_OWNER,
};
use crate::core::run::{RunThought, RunThoughtId, RunThoughtLifecycle, ThoughtConsumptionReceipt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FACTORY_BUILD_COGNITIVE_VIEW_CONTRACT: &str = "factory.build-cognitive-view/v1";
pub const FACTORY_BUILD_COGNITIVE_PROVIDER_CONTRACT: &str =
    "factory.build-cognitive-view-provider/v1";

/// Exact Factory-owned selectors over a Run Thought field.
///
/// This focus is deliberately mechanical. It does not perform semantic relevance,
/// parse authored relation metadata, or interpret QL coordinates. Those richer
/// readings belong to the knowledge/formal providers that own them.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildCognitiveFocus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_map_subject_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_evidence_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<RunThoughtLifecycle>,
}

impl FactoryBuildCognitiveFocus {
    fn matches(&self, thought: &RunThought) -> bool {
        self.run_map_subject_ref.as_deref().is_none_or(|subject| {
            thought
                .run_map_subject_refs
                .iter()
                .any(|candidate| candidate == subject)
        }) && self
            .producer_ref
            .as_deref()
            .is_none_or(|producer| thought.producer.contains_ref(producer))
            && self
                .anchor_ref
                .as_deref()
                .is_none_or(|anchor| thought.anchor_ref == anchor)
            && self.related_ref.as_deref().is_none_or(|related| {
                thought
                    .related_refs
                    .iter()
                    .any(|candidate| candidate == related)
            })
            && self
                .relation_evidence_ref
                .as_deref()
                .is_none_or(|relation| {
                    thought
                        .relation_evidence_refs
                        .iter()
                        .any(|candidate| candidate == relation)
                })
            && self
                .lifecycle
                .is_none_or(|lifecycle| thought.lifecycle == lifecycle)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildCognitiveSnapshot {
    pub contract: String,
    pub provider_contract: String,
    pub revision: u64,
    pub provenance: FactoryBuildCognitiveProvenance,
    pub view: FactoryBuildCognitiveView,
}

impl FactoryBuildCognitiveSnapshot {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildCognitiveProvenance {
    pub owner: String,
    pub factory_state_revision: u64,
    pub run_revision: u64,
    pub run_map_revision: u64,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildCognitiveView {
    pub run_ref: String,
    pub thought_count: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub consumption_refs: BTreeMap<RunThoughtId, RunThoughtId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumptions: Vec<ThoughtConsumptionReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<FactoryBuildCognitiveFocus>,
    pub thoughts: Vec<RunThought>,
    pub by_run_map_subject: BTreeMap<String, Vec<RunThoughtId>>,
    pub by_producer_ref: BTreeMap<String, Vec<RunThoughtId>>,
    pub by_anchor_ref: BTreeMap<String, Vec<RunThoughtId>>,
    pub by_related_ref: BTreeMap<String, Vec<RunThoughtId>>,
    pub by_relation_evidence_ref: BTreeMap<String, Vec<RunThoughtId>>,
    pub by_lifecycle: BTreeMap<String, Vec<RunThoughtId>>,
}

#[derive(Debug, Default)]
pub struct FactoryBuildCognitiveViewProvider;

impl FactoryBuildCognitiveViewProvider {
    pub fn snapshot(
        &self,
        state: &FactoryBuildState,
        selection: &FactoryBuildSelection,
    ) -> Result<FactoryBuildCognitiveSnapshot, FactoryBuildError> {
        self.materialise(state, selection, None)
    }

    pub fn focused_snapshot(
        &self,
        state: &FactoryBuildState,
        selection: &FactoryBuildSelection,
        focus: &FactoryBuildCognitiveFocus,
    ) -> Result<FactoryBuildCognitiveSnapshot, FactoryBuildError> {
        self.materialise(state, selection, Some(focus))
    }

    fn materialise(
        &self,
        state: &FactoryBuildState,
        selection: &FactoryBuildSelection,
        focus: Option<&FactoryBuildCognitiveFocus>,
    ) -> Result<FactoryBuildCognitiveSnapshot, FactoryBuildError> {
        if state.project().reference() != &selection.project_ref {
            return Err(FactoryBuildError::ProjectNotFound(
                selection.project_ref.to_string(),
            ));
        }
        let run = state
            .run(&selection.run_ref)
            .ok_or_else(|| FactoryBuildError::RunNotFound(selection.run_ref.to_string()))?;
        if run.project_ref() != state.project().reference() {
            return Err(FactoryBuildError::ProjectRunMismatch);
        }

        let thoughts = run
            .thought_field()
            .thoughts()
            .values()
            .filter(|thought| focus.is_none_or(|selector| selector.matches(thought)))
            .cloned()
            .collect::<Vec<_>>();
        let mut view = materialise_view(run.reference().to_string(), focus.cloned(), thoughts);
        for thought in &view.thoughts {
            if let Some(receipt) = run.thought_field().consumed_by(&thought.id) {
                view.consumption_refs.insert(
                    thought.id.clone(),
                    receipt.consumption.consumption_id.clone(),
                );
            }
        }
        // A focused projection does not expand into unrelated reports merely
        // because one of its reports was consumed together with them.
        view.consumptions = run
            .thought_field()
            .consumptions()
            .filter(|receipt| {
                receipt.consumption.inputs.iter().all(|input| {
                    view.thoughts
                        .iter()
                        .any(|thought| thought.id == input.thought_id)
                })
            })
            .cloned()
            .collect();

        Ok(FactoryBuildCognitiveSnapshot {
            contract: FACTORY_BUILD_COGNITIVE_VIEW_CONTRACT.into(),
            provider_contract: FACTORY_BUILD_COGNITIVE_PROVIDER_CONTRACT.into(),
            revision: state.revision().get(),
            provenance: FactoryBuildCognitiveProvenance {
                owner: FACTORY_NATIVE_OWNER.into(),
                factory_state_revision: state.revision().get(),
                run_revision: run.revision().get(),
                run_map_revision: run.map().topology_revision().get(),
                source: "canonical FactoryBuildState -> canonical Run/RunThoughtField".into(),
            },
            view,
        })
    }
}

fn materialise_view(
    run_ref: String,
    focus: Option<FactoryBuildCognitiveFocus>,
    thoughts: Vec<RunThought>,
) -> FactoryBuildCognitiveView {
    let mut by_run_map_subject = BTreeMap::new();
    let mut by_producer_ref = BTreeMap::new();
    let mut by_anchor_ref = BTreeMap::new();
    let mut by_related_ref = BTreeMap::new();
    let mut by_relation_evidence_ref = BTreeMap::new();
    let mut by_lifecycle = BTreeMap::new();

    for thought in &thoughts {
        for subject_ref in &thought.run_map_subject_refs {
            index(&mut by_run_map_subject, subject_ref, &thought.id);
        }
        for producer_ref in [
            thought.producer.agent_ref.as_deref(),
            thought.producer.agency_ref.as_deref(),
            thought.producer.agent_session_ref.as_deref(),
            thought.producer.execution_ref.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            index(&mut by_producer_ref, producer_ref, &thought.id);
        }
        index(&mut by_anchor_ref, &thought.anchor_ref, &thought.id);
        for related_ref in &thought.related_refs {
            index(&mut by_related_ref, related_ref, &thought.id);
        }
        for relation_ref in &thought.relation_evidence_refs {
            index(&mut by_relation_evidence_ref, relation_ref, &thought.id);
        }
        index(
            &mut by_lifecycle,
            thought_lifecycle_name(thought.lifecycle),
            &thought.id,
        );
    }

    FactoryBuildCognitiveView {
        run_ref,
        thought_count: thoughts.len(),
        consumption_refs: BTreeMap::new(),
        consumptions: Vec::new(),
        focus,
        thoughts,
        by_run_map_subject,
        by_producer_ref,
        by_anchor_ref,
        by_related_ref,
        by_relation_evidence_ref,
        by_lifecycle,
    }
}

fn index(index: &mut BTreeMap<String, Vec<RunThoughtId>>, key: &str, thought_id: &RunThoughtId) {
    let entry = index.entry(key.to_owned()).or_default();
    if !entry.contains(thought_id) {
        entry.push(thought_id.clone());
    }
}

fn thought_lifecycle_name(lifecycle: RunThoughtLifecycle) -> &'static str {
    match lifecycle {
        RunThoughtLifecycle::Active => "active",
        RunThoughtLifecycle::Resolved => "resolved",
        RunThoughtLifecycle::Superseded => "superseded",
        RunThoughtLifecycle::Integrated => "integrated",
        RunThoughtLifecycle::Retained => "retained",
    }
}
