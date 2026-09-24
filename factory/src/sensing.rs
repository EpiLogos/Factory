//! Source-qualified sensing relations in Factory's existing developmental state.
//! Raw provider history stays at its owner. These records retain the evidence
//! address, coverage, classification and the ordinary work/Return relation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY_SCHEMA: &str = "factory.sensing-policy/v1";
pub const FIELD_SCHEMA: &str = "factory.telemetry-field/v1";
pub const COLLECTION_SCHEMA: &str = "factory.signal-collection/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: String,
    pub version: u32,
    pub project_world_ref: String,
    #[serde(default)]
    pub repositories: Vec<Repository>,
    pub sources: Vec<Source>,
    pub workflows: BTreeMap<String, Workflow>,
    #[serde(default)]
    pub skill_prompts: BTreeMap<String, String>,
    #[serde(default)]
    pub worktree: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub id: String,
    pub provider: String,
    pub remote: String,
    #[serde(default)]
    pub worktree: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub provider: String,
    pub scope: String,
    /// Exact source/native owner address, distinct from a display label.
    pub source_ref: String,
    #[serde(default)]
    pub page_limit: Option<usize>,
    #[serde(default)]
    pub arguments: BTreeMap<String, Value>,
    #[serde(default)]
    pub integration: Option<String>,
    #[serde(default)]
    pub read_tool: Option<String>,
    #[serde(default)]
    pub pagination: Option<String>,
    #[serde(default, rename = "type")]
    pub source_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub window: Option<String>,
    #[serde(default)]
    pub compare_with: Option<String>,
    #[serde(default)]
    pub repositories: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub granularity: Option<String>,
    #[serde(default)]
    pub filters: BTreeMap<String, Value>,
    #[serde(default)]
    pub target: BTreeMap<String, Value>,
    #[serde(default)]
    pub authorization: Option<String>,
    #[serde(default)]
    pub soak: Option<String>,
    #[serde(default)]
    pub focus: Option<String>,
    #[serde(default)]
    pub fix: ActionPolicy,
    #[serde(default)]
    pub implement: ActionPolicy,
    #[serde(default)]
    pub reply: ActionPolicy,
    #[serde(default)]
    pub close: ActionPolicy,
    #[serde(default)]
    pub review: ActionPolicy,
    #[serde(default)]
    pub approve: ActionPolicy,
    #[serde(default)]
    pub publish: ActionPolicy,
    #[serde(default)]
    pub merge: ActionPolicy,
    #[serde(default)]
    pub deploy: ActionPolicy,
    #[serde(default)]
    pub recover: ActionPolicy,
    #[serde(default)]
    pub notify: ActionPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPolicy {
    #[serde(default = "never")]
    pub mode: String,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub stop: Vec<String>,
    #[serde(default)]
    pub require: Vec<String>,
    #[serde(default)]
    pub tone: Option<String>,
    #[serde(default)]
    pub guidance: Option<String>,
}
fn never() -> String {
    "never".into()
}
impl Default for ActionPolicy {
    fn default() -> Self {
        Self {
            mode: never(),
            allow: vec![],
            stop: vec![],
            require: vec![],
            tone: None,
            guidance: None,
        }
    }
}

impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != POLICY_SCHEMA
            || self.version != 1
            || !(self.project_world_ref == "control:root"
                || self
                    .project_world_ref
                    .strip_prefix("project:")
                    .is_some_and(|id| !id.trim().is_empty() && id == id.trim()))
        {
            return Err("unsupported sensing policy or missing ProjectWorld identity".into());
        }
        let mut ids = BTreeSet::new();
        for source in &self.sources {
            if source.id.is_empty()
                || source.source_ref.is_empty()
                || source.scope.is_empty()
                || !ids.insert(source.id.clone())
                || source.page_limit == Some(0)
                || source.page_limit.is_some_and(|n| n > 10_000)
            {
                return Err("source IDs/refs/scopes must be nonempty and unique; page limit must be 1..10000".into());
            }
            if source.provider == "factory" && source.scope != self.project_world_ref {
                return Err("Factory source scope must equal the policy ProjectWorld".into());
            }
            if source.provider == "github"
                && !self
                    .repositories
                    .iter()
                    .any(|repo| repo.provider == "github" && repo.remote == source.scope)
            {
                return Err(format!(
                    "GitHub source {} has no explicitly configured repository relation",
                    source.id
                ));
            }
        }
        for flow in self.workflows.values() {
            if flow.sources.iter().any(|id| !ids.contains(id)) {
                return Err("workflow names an undefined source".into());
            }
            for action in [
                &flow.fix,
                &flow.implement,
                &flow.reply,
                &flow.close,
                &flow.review,
                &flow.approve,
                &flow.publish,
                &flow.merge,
                &flow.deploy,
                &flow.recover,
                &flow.notify,
            ] {
                if ![
                    "never",
                    "manual",
                    "criteria",
                    "after-fix",
                    "after-merge",
                    "meaningful-change-only",
                ]
                .contains(&action.mode.as_str())
                {
                    return Err(format!(
                        "unknown action policy {}; it cannot silently enable an action",
                        action.mode
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    pub since_unix_ms: i64,
    /// Exclusive: adjacent Days cannot double-count a boundary occurrence.
    pub until_unix_ms: i64,
}
impl Interval {
    pub fn contains(&self, time: i64) -> bool {
        time >= self.since_unix_ms && time < self.until_unix_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageState {
    Complete,
    Empty,
    Unavailable,
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub source_ref: String,
    pub provider_ref: String,
    pub scope: String,
    pub state: CoverageState,
    pub window: Interval,
    pub records: usize,
    pub pages: usize,
    pub cursor: Option<String>,
    pub reason: Option<String>,
    pub query_basis: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub source_ref: String,
    pub provider_ref: String,
    pub source_revision: String,
    pub occurred_at_unix_ms: Option<i64>,
    pub observed_at_unix_ms: i64,
    pub summary: String,
    pub dimension: String,
    pub standing: String,
    #[serde(default)]
    pub relation_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Classification {
    VerifiedDefect,
    RepeatedSymptom,
    FeatureRequest,
    SubjectiveFeedback,
    Duplicate,
    OutOfScope,
    NeedsEvidence,
    HumanDecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub classification: Classification,
    pub source_revision: String,
    pub evidence_refs: Vec<String>,
    pub reason: String,
    pub decision_needed: Option<String>,
    pub boundary_ref: Option<String>,
    pub actor_ref: String,
    pub authority_ref: String,
    pub recorded_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkRelation {
    pub custody_ref: String,
    pub work_ref: String,
    pub run_ref: Option<String>,
    pub position_ref: String,
    pub now_ref: Option<String>,
    pub authority_ref: String,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalReturn {
    pub return_ref: String,
    pub source_revision: String,
    pub change_revision: String,
    pub evidence_refs: Vec<String>,
    pub installed_revision: Option<String>,
    pub running_revision: Option<String>,
    pub live_evidence_ref: Option<String>,
    pub outcome: String,
    pub recorded_at_unix_ms: i64,
    pub actor_ref: String,
    pub authority_ref: String,
    #[serde(default)]
    pub attempt_ref: Option<String>,
    #[serde(default)]
    pub verification_ref: Option<String>,
    #[serde(default)]
    pub merge_basis: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub signal_ref: String,
    pub project_world_ref: String,
    pub observation: Observation,
    pub prior_source_revisions: Vec<String>,
    /// Bounded source-qualified versions, not copies of provider payloads.
    /// Historical Day reads must not borrow a later source observation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prior_observations: Vec<Observation>,
    pub first_observed_at_unix_ms: i64,
    pub decisions: Vec<Decision>,
    pub work: Option<WorkRelation>,
    pub returns: Vec<SignalReturn>,
}
impl Signal {
    pub fn decision(&self) -> Option<&Decision> {
        self.decisions
            .last()
            .filter(|d| d.source_revision == self.observation.source_revision)
    }
    pub fn classification(&self) -> Classification {
        self.decision()
            .map(|d| d.classification)
            .unwrap_or(Classification::NeedsEvidence)
    }
    pub fn disposition(&self) -> &'static str {
        if self.returns.last().is_some_and(|r| {
            r.outcome == "live-resolved" && r.source_revision == self.observation.source_revision
        }) {
            "live-resolved"
        } else if self.work.is_some() {
            "work"
        } else if self.classification() == Classification::HumanDecision {
            "human-decision"
        } else if matches!(
            self.classification(),
            Classification::OutOfScope | Classification::Duplicate
        ) {
            "retained"
        } else {
            "investigate"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub collection_ref: String,
    pub policy_ref: String,
    pub policy_revision: String,
    pub recorded_at_unix_ms: i64,
    pub coverage: Vec<Coverage>,
    pub signal_refs: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SensingState {
    pub revision: u64,
    pub project_world_ref: Option<String>,
    pub signals: BTreeMap<String, Signal>,
    pub collections: Vec<Collection>,
}
impl SensingState {
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty() && self.collections.is_empty()
    }
    pub fn validate(&self) -> Result<(), String> {
        for (key, signal) in &self.signals {
            if key != &signal.signal_ref
                || Some(&signal.project_world_ref) != self.project_world_ref.as_ref()
                || *key != signal_ref(&signal.project_world_ref, &signal.observation.source_ref)
            {
                return Err("sensing identity or ProjectWorld mismatch".into());
            }
        }
        Ok(())
    }
}

pub fn signal_ref(world: &str, source: &str) -> String {
    format!(
        "factory:signal:{}",
        blake3::hash(format!("{world}\0{source}").as_bytes()).to_hex()
    )
}

pub fn validate_native_relations(
    state: &crate::developmental_read::FactoryDevelopmentalState,
) -> Result<(), String> {
    let Some(world) = &state.sensing.project_world_ref else {
        return Ok(());
    };
    let expected = state
        .central_project_links
        .get(state.build.project().reference())
        .map(|link| {
            let reference = &link.central_project_ref;
            if reference == "control:root" || reference.starts_with("project:") {
                reference.clone()
            } else {
                format!("project:{reference}")
            }
        })
        .unwrap_or_else(|| state.project_ref().to_string());
    if world != &expected {
        return Err("sensing ProjectWorld differs from its native Project link".into());
    }
    for signal in state.sensing.signals.values() {
        if let Some(work) = &signal.work {
            let custody = state
                .work_custody
                .iter()
                .find(|c| c.custody_ref == work.custody_ref)
                .ok_or("signal work does not resolve to native custody")?;
            if work.work_ref != signal.signal_ref
                || custody.work_ref != work.work_ref
                || custody.position_ref != work.position_ref
                || custody.run_ref.as_ref().map(ToString::to_string) != work.run_ref
            {
                return Err("signal work differs from its native custody relation".into());
            }
        }
    }
    Ok(())
}

pub fn retain(state: &mut SensingState, world: &str, observation: Observation) -> String {
    let reference = signal_ref(world, &observation.source_ref);
    if let Some(existing) = state.signals.get_mut(&reference) {
        if existing.observation.source_revision != observation.source_revision {
            existing
                .prior_source_revisions
                .push(existing.observation.source_revision.clone());
            existing
                .prior_observations
                .push(existing.observation.clone());
            existing.observation = observation;
        }
    } else {
        state.signals.insert(
            reference.clone(),
            Signal {
                signal_ref: reference.clone(),
                project_world_ref: world.into(),
                first_observed_at_unix_ms: observation.observed_at_unix_ms,
                observation,
                prior_source_revisions: vec![],
                prior_observations: vec![],
                decisions: vec![],
                work: None,
                returns: vec![],
            },
        );
    }
    reference
}
