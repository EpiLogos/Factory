//! Factory-owned Agent-first workflow source and deterministic compilation.
//!
//! This module stops at the existing Run topology boundary. It describes and
//! compiles developmental work; Execution Intelligence still selects runtime
//! resources and `ExecutionDisposition` remains the executable decision.

use crate::core::identity::{Ref, Revision};
use crate::core::run::{
    EdgeKind, NodeId, NodeKind, NodeState, RunTopologyCommand, TopologyEdge, TopologyMutation,
    TopologyNode, WorkflowUnitRef,
};
use crate::execution_intelligence::ExecutionDemand;
pub use crate::workflow_reference::WorkflowSubjectRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use ulid::Ulid;

pub const WORKFLOW_SOURCE_SCHEMA: &str = "factory.agent-workflow-source/v1";
pub const COMPILED_WORKFLOW_SCHEMA: &str = "factory.compiled-agent-workflow/v1";
pub const BOUNDED_COORDINATION_CONTRACT: &str = "factory.bounded-coordination/v1";
pub const WORKFLOW_UNIT_IDENTITY_ALGORITHM: &str = "factory.workflow-unit.identity.blake3-ulid/v1";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowSourceProvenance {
    #[serde(rename = "ref")]
    pub reference: Ref,
    pub revision: String,
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring: Option<Box<crate::workflow_authoring::AuthoredBasis>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentRequirements {
    #[serde(default)]
    pub agent_refs: Vec<String>,
    #[serde(default)]
    pub agent_set_refs: Vec<String>,
    #[serde(default)]
    pub agency_refs: Vec<String>,
}

/// Optional explicit delivery requirements for a contribution. These are authored
/// requirements, never grants or assertions that a child has loaded them.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowContribution {
    pub description: String,
    pub role_source: WorkflowRoleSource,
    #[serde(default, deserialize_with = "unique_contribution_refs")]
    pub context_refs: BTreeSet<String>,
    #[serde(default, deserialize_with = "unique_contribution_refs")]
    pub required_tools: BTreeSet<String>,
    #[serde(default, deserialize_with = "unique_contribution_refs")]
    pub required_actions: BTreeSet<String>,
    #[serde(default, deserialize_with = "unique_contribution_refs")]
    pub required_modalities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_harness_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_model_ref: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowRoleSource {
    pub owner: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub revision: String,
}
fn unique_contribution_refs<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeSet<String>, D::Error> {
    let values = Vec::<String>::deserialize(deserializer)?;
    let set = values.iter().cloned().collect::<BTreeSet<_>>();
    if set.len() != values.len() || values.len() > 128 {
        return Err(serde::de::Error::custom(
            "contribution references must be unique and limited to 128",
        ));
    }
    Ok(set)
}
// Foreign owners retain their own qualified identifiers; a role/ContextSource
// does not acquire a Factory ULID merely because a workflow names it.
fn validate_contribution_ref(field: &'static str, value: &str) -> Result<(), WorkflowError> {
    if value.is_empty()
        || value.len() > 2048
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
        || !(value.contains(':') || value.contains('/'))
    {
        return Err(WorkflowError::InvalidReference {
            field,
            value: value.into(),
        });
    }
    Ok(())
}
impl WorkflowContribution {
    fn validate(&self) -> Result<(), WorkflowError> {
        required("contribution.description", &self.description)?;
        required("contribution.roleSource.owner", &self.role_source.owner)?;
        validate_contribution_ref("contribution.roleSource.ref", &self.role_source.reference)?;
        required(
            "contribution.roleSource.revision",
            &self.role_source.revision,
        )?;
        for (field, refs) in [
            ("contribution.contextRefs", &self.context_refs),
            ("contribution.requiredTools", &self.required_tools),
            ("contribution.requiredActions", &self.required_actions),
        ] {
            for reference in refs {
                validate_contribution_ref(field, reference)?;
            }
        }
        for modality in &self.required_modalities {
            required("contribution.requiredModalities", modality)?;
        }
        for (field, value) in [
            (
                "contribution.requiredHarnessRef",
                &self.required_harness_ref,
            ),
            ("contribution.requiredModelRef", &self.required_model_ref),
        ] {
            if let Some(value) = value {
                validate_contribution_ref(field, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowUnitSource {
    /// Source-local semantic locator. Dependencies use this key, never a runtime id.
    pub key: String,
    pub developmental_concern: String,
    pub required_difference: String,
    pub return_contract: String,
    pub subject_ref: WorkflowSubjectRef,
    pub basis_revision: String,
    pub agent_requirements: AgentRequirements,
    pub praxis_refs: Vec<String>,
    pub capability_refs: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub independence_from: Vec<String>,
    pub permitted_effects: Vec<String>,
    pub verification_obligations: Vec<String>,
    pub return_address: String,
    pub stop_conditions: String,
    pub escalation_conditions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution: Option<WorkflowContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<crate::workflow_inputs::WorkflowInputSource>,
    /// Optional C′ lowered from a registered domain adapter (QL Vāk). It is an
    /// execution-relevant requirement: it enters the semantic digest and unit
    /// identity. Generic workflows omit it and need no QL lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<crate::vak_orchestration::CPrimeExecutionBinding>,
}

/// A named join. Runtime fork/join lifecycle remains owned by #198; this is
/// the open source representation and its current RunMap projection.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowBarrierSource {
    pub key: String,
    pub waits_for: Vec<String>,
    pub releases: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowNestingSource {
    pub parent: String,
    pub child: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowSource {
    pub schema_version: String,
    pub coordination_contract: String,
    pub source: WorkflowSourceProvenance,
    pub workflow_key: String,
    pub units: Vec<WorkflowUnitSource>,
    #[serde(default)]
    pub barriers: Vec<WorkflowBarrierSource>,
    #[serde(default)]
    pub nesting: Vec<WorkflowNestingSource>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledWorkflowUnit {
    #[serde(rename = "ref")]
    pub reference: WorkflowUnitRef,
    pub key: String,
    pub developmental_concern: String,
    pub required_difference: String,
    pub return_contract: String,
    pub subject_ref: WorkflowSubjectRef,
    pub basis_revision: String,
    pub agent_requirements: CompiledAgentRequirements,
    pub praxis_refs: BTreeSet<String>,
    pub capability_refs: BTreeSet<String>,
    pub dependencies: BTreeSet<WorkflowUnitRef>,
    pub independence_from: BTreeSet<WorkflowUnitRef>,
    pub permitted_effects: BTreeSet<String>,
    pub verification_obligations: BTreeSet<String>,
    pub return_address: String,
    pub stop_conditions: String,
    pub escalation_conditions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution: Option<WorkflowContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<crate::workflow_inputs::CompiledWorkflowInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<crate::vak_orchestration::CPrimeExecutionBinding>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledAgentRequirements {
    pub agent_refs: BTreeSet<String>,
    pub agent_set_refs: BTreeSet<String>,
    pub agency_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledWorkflowBarrier {
    pub key: String,
    pub waits_for: BTreeSet<WorkflowUnitRef>,
    pub releases: BTreeSet<WorkflowUnitRef>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledWorkflowNesting {
    pub parent: WorkflowUnitRef,
    pub child: WorkflowUnitRef,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledWorkflow {
    pub schema_version: String,
    pub identity_algorithm: String,
    pub source: WorkflowSourceProvenance,
    pub workflow_key: String,
    pub units: BTreeMap<String, CompiledWorkflowUnit>,
    pub barriers: Vec<CompiledWorkflowBarrier>,
    pub nesting: Vec<CompiledWorkflowNesting>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WorkflowError {
    WrongSchemaVersion(String),
    WrongCoordinationContract(String),
    EmptyField(String),
    InvalidLocator {
        field: &'static str,
        value: String,
    },
    InvalidReference {
        field: &'static str,
        value: String,
    },
    InvalidDigest(String),
    SourceDigestMismatch {
        declared: String,
        computed: String,
    },
    EmptyCollection(&'static str),
    DuplicateValue {
        field: &'static str,
        value: String,
    },
    DuplicateUnit(String),
    DuplicateBarrier(String),
    DanglingUnit {
        field: &'static str,
        key: String,
    },
    SelfDependency(String),
    DependencyCycle(Vec<String>),
    NestingCycle(Vec<String>),
    ContradictoryRelations {
        unit: String,
        key: String,
    },
    InvalidBarrier(String),
    /// A unit's C′ is invalid for the native Vāk contract, its unit, or the
    /// compiled topology its thread form requires.
    InvalidComposition {
        unit: String,
        reason: String,
    },
    Serialization(String),
}

impl Display for WorkflowError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid Agent-first workflow source: {self:?}")
    }
}

impl Error for WorkflowError {}

/// Compute the canonical digest covered by `source.digest`.
///
/// Formatting, collection order, and optional Flow/temporal carrying context do
/// not affect it. Every authored semantic and structural field does.
pub fn workflow_source_digest(source: &WorkflowSource) -> Result<String, WorkflowError> {
    let canonical = canonical_source_content(source)?;
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| WorkflowError::Serialization(error.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

pub fn compile_workflow(source: WorkflowSource) -> Result<CompiledWorkflow, WorkflowError> {
    if source.schema_version != WORKFLOW_SOURCE_SCHEMA {
        return Err(WorkflowError::WrongSchemaVersion(source.schema_version));
    }
    if source.coordination_contract != BOUNDED_COORDINATION_CONTRACT {
        return Err(WorkflowError::WrongCoordinationContract(
            source.coordination_contract,
        ));
    }
    if source.source.reference.kind() != "workflow-source" {
        return Err(WorkflowError::InvalidReference {
            field: "source.ref",
            value: source.source.reference.to_string(),
        });
    }
    let workflow_key = locator("workflowKey", &source.workflow_key)?;
    let source_revision = required("source.revision", &source.source.revision)?;
    validate_digest(&source.source.digest)?;

    if let Some(basis) = &source.source.authoring {
        basis
            .validate_source(&source)
            .map_err(|e| WorkflowError::Serialization(e.to_string()))?;
    }
    let computed_digest = workflow_source_digest(&source)?;
    let declared_digest = source.source.digest.to_ascii_lowercase();
    if declared_digest != computed_digest {
        return Err(WorkflowError::SourceDigestMismatch {
            declared: declared_digest,
            computed: computed_digest,
        });
    }

    let canonical = canonical_source_content(&source)?;
    if canonical.units.is_empty() {
        return Err(WorkflowError::EmptyCollection("units"));
    }

    let canonical_units = canonical
        .units
        .iter()
        .cloned()
        .map(|unit| (unit.key.clone(), unit))
        .collect::<BTreeMap<_, _>>();
    validate_unit_relations(&canonical_units)?;
    validate_barrier_sources(&canonical.barriers, &canonical_units)?;
    validate_requires_acyclic(&canonical_units, &canonical.barriers)?;
    validate_nesting_sources(&canonical.nesting, &canonical_units)?;

    let mut unit_refs = BTreeMap::new();
    for (key, unit) in &canonical_units {
        let reference = derive_unit_ref(
            &source.source.reference,
            &source_revision,
            &declared_digest,
            &workflow_key,
            unit,
        )?;
        unit_refs.insert(key.clone(), reference);
    }

    let mut units = BTreeMap::new();
    for (key, unit) in canonical_units {
        units.insert(
            key.clone(),
            CompiledWorkflowUnit {
                reference: unit_refs[&key].clone(),
                key,
                developmental_concern: unit.developmental_concern,
                required_difference: unit.required_difference,
                return_contract: unit.return_contract,
                subject_ref: unit.subject_ref,
                basis_revision: unit.basis_revision,
                agent_requirements: unit.agent_requirements,
                praxis_refs: unit.praxis_refs,
                capability_refs: unit.capability_refs,
                dependencies: resolve_keys("dependencies", &unit.dependencies, &unit_refs)?,
                independence_from: resolve_keys(
                    "independenceFrom",
                    &unit.independence_from,
                    &unit_refs,
                )?,
                permitted_effects: unit.permitted_effects,
                verification_obligations: unit.verification_obligations,
                return_address: unit.return_address,
                stop_conditions: unit.stop_conditions,
                escalation_conditions: unit.escalation_conditions,
                contribution: unit.contribution,
                inputs: unit
                    .inputs
                    .iter()
                    .map(|input| crate::workflow_inputs::CompiledWorkflowInput {
                        predecessor: unit_refs[&input.predecessor].clone(),
                        receiving_context_ref: input.receiving_context_ref.clone(),
                    })
                    .collect(),
                composition: unit.composition,
            },
        );
    }

    let compiled = CompiledWorkflow {
        schema_version: COMPILED_WORKFLOW_SCHEMA.to_string(),
        identity_algorithm: WORKFLOW_UNIT_IDENTITY_ALGORITHM.to_string(),
        source: WorkflowSourceProvenance {
            reference: source.source.reference,
            revision: source_revision,
            digest: declared_digest,
            temporal_ref: normalize_optional(source.source.temporal_ref),
            flow_ref: normalize_optional(source.source.flow_ref),
            authoring: source.source.authoring,
        },
        workflow_key,
        units,
        barriers: compile_barriers(&canonical.barriers, &unit_refs)?,
        nesting: compile_nesting(&canonical.nesting, &unit_refs)?,
    };
    crate::vak_orchestration::validate_workflow_compositions(&compiled).map_err(|fault| {
        WorkflowError::InvalidComposition {
            unit: fault.unit,
            reason: fault.error.to_string(),
        }
    })?;
    Ok(compiled)
}

impl CompiledWorkflow {
    /// Produce one atomic command for the existing Run-owned mutation path.
    pub fn topology_command(&self, expected_revision: Revision) -> RunTopologyCommand {
        let mut mutations = Vec::new();
        let unit_nodes = self
            .units
            .iter()
            .map(|(key, unit)| (unit.reference.clone(), unit_node_id(key)))
            .collect::<BTreeMap<_, _>>();
        let barrier_releases = self
            .barriers
            .iter()
            .flat_map(|barrier| barrier.releases.iter().cloned())
            .collect::<BTreeSet<_>>();

        for unit in self.units.values() {
            mutations.push(TopologyMutation::AddNode {
                node: TopologyNode {
                    id: unit_nodes[&unit.reference].clone(),
                    kind: NodeKind::Work,
                    label: unit.developmental_concern.clone(),
                    state: Some(
                        if unit.dependencies.is_empty()
                            && !barrier_releases.contains(&unit.reference)
                        {
                            NodeState::Ready
                        } else {
                            NodeState::Planned
                        },
                    ),
                    semantic_ref: Some(unit.reference.clone().into()),
                },
            });
        }

        for barrier in &self.barriers {
            mutations.push(TopologyMutation::AddNode {
                node: TopologyNode {
                    id: barrier_node_id(&barrier.key),
                    kind: NodeKind::Gate,
                    label: format!("Workflow barrier {}", barrier.key),
                    state: None,
                    semantic_ref: None,
                },
            });
        }

        let mut consumed = BTreeSet::new();
        for unit in self.units.values() {
            for dependency in &unit.dependencies {
                consumed.insert(dependency.clone());
                mutations.push(TopologyMutation::AddEdge {
                    edge: TopologyEdge {
                        from: unit_nodes[&unit.reference].clone(),
                        to: unit_nodes[dependency].clone(),
                        relation: EdgeKind::Requires,
                    },
                });
            }
        }
        for barrier in &self.barriers {
            let barrier_node = barrier_node_id(&barrier.key);
            for waiting in &barrier.waits_for {
                consumed.insert(waiting.clone());
                mutations.push(TopologyMutation::AddEdge {
                    edge: TopologyEdge {
                        from: barrier_node.clone(),
                        to: unit_nodes[waiting].clone(),
                        relation: EdgeKind::Requires,
                    },
                });
            }
            for released in &barrier.releases {
                mutations.push(TopologyMutation::AddEdge {
                    edge: TopologyEdge {
                        from: unit_nodes[released].clone(),
                        to: barrier_node.clone(),
                        relation: EdgeKind::Requires,
                    },
                });
            }
        }
        for relation in &self.nesting {
            mutations.push(TopologyMutation::AddEdge {
                edge: TopologyEdge {
                    from: unit_nodes[&relation.parent].clone(),
                    to: unit_nodes[&relation.child].clone(),
                    relation: EdgeKind::Nests,
                },
            });
        }

        let destination = NodeId::new("destination").expect("static valid node id");
        for unit in self.units.values() {
            if !consumed.contains(&unit.reference) {
                mutations.push(TopologyMutation::AddEdge {
                    edge: TopologyEdge {
                        from: destination.clone(),
                        to: unit_nodes[&unit.reference].clone(),
                        relation: EdgeKind::BranchesTo,
                    },
                });
            }
        }

        RunTopologyCommand {
            command_id: format!(
                "compile-workflow:{}/{}/{}",
                self.source.reference, self.source.revision, self.source.digest
            ),
            expected_revision,
            mutation: TopologyMutation::Batch { mutations },
        }
    }

    pub fn unit(&self, key: &str) -> Option<&CompiledWorkflowUnit> {
        self.units.get(key)
    }
}

impl CompiledWorkflowUnit {
    /// Correlate this stable semantic unit with the existing Execution
    /// Intelligence demand. Runtime selection still happens only when that
    /// demand is accepted into an `ExecutionDisposition`.
    pub fn bind_execution_demand(&self, mut demand: ExecutionDemand) -> ExecutionDemand {
        demand.workflow_unit_ref = Some(self.reference.to_string());
        if let Some(contribution) = &self.contribution {
            demand
                .required_tools
                .extend(contribution.required_tools.clone());
            demand
                .required_actions
                .extend(contribution.required_actions.clone());
            demand
                .required_modalities
                .extend(contribution.required_modalities.clone());
        }
        demand
            .required_capabilities
            .extend(self.capability_refs.iter().cloned());
        demand
            .independence_from
            .extend(self.independence_from.iter().map(ToString::to_string));
        demand
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalWorkflowContent {
    coordination_contract: String,
    workflow_key: String,
    units: Vec<CanonicalUnit>,
    barriers: Vec<CanonicalBarrier>,
    nesting: Vec<CanonicalNesting>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalUnit {
    key: String,
    developmental_concern: String,
    required_difference: String,
    return_contract: String,
    subject_ref: WorkflowSubjectRef,
    basis_revision: String,
    agent_requirements: CompiledAgentRequirements,
    praxis_refs: BTreeSet<String>,
    capability_refs: BTreeSet<String>,
    dependencies: BTreeSet<String>,
    independence_from: BTreeSet<String>,
    permitted_effects: BTreeSet<String>,
    verification_obligations: BTreeSet<String>,
    return_address: String,
    stop_conditions: String,
    escalation_conditions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    contribution: Option<WorkflowContribution>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    inputs: Vec<crate::workflow_inputs::WorkflowInputSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    composition: Option<crate::vak_orchestration::CPrimeExecutionBinding>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalBarrier {
    key: String,
    waits_for: BTreeSet<String>,
    releases: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalNesting {
    parent: String,
    child: String,
}

fn canonical_source_content(
    source: &WorkflowSource,
) -> Result<CanonicalWorkflowContent, WorkflowError> {
    let workflow_key = locator("workflowKey", &source.workflow_key)?;
    let mut units = source
        .units
        .iter()
        .map(canonical_unit)
        .collect::<Result<Vec<_>, _>>()?;
    units.sort_by(|left, right| left.key.cmp(&right.key));
    if let Some(pair) = units.windows(2).find(|pair| pair[0].key == pair[1].key) {
        return Err(WorkflowError::DuplicateUnit(pair[0].key.clone()));
    }

    let mut barriers = source
        .barriers
        .iter()
        .map(|barrier| {
            Ok(CanonicalBarrier {
                key: locator("barriers.key", &barrier.key)?,
                waits_for: normalized_locators("barriers.waitsFor", &barrier.waits_for)?,
                releases: normalized_locators("barriers.releases", &barrier.releases)?,
            })
        })
        .collect::<Result<Vec<_>, WorkflowError>>()?;
    barriers.sort();
    if let Some(pair) = barriers.windows(2).find(|pair| pair[0].key == pair[1].key) {
        return Err(WorkflowError::DuplicateBarrier(pair[0].key.clone()));
    }

    let mut nesting = source
        .nesting
        .iter()
        .map(|relation| {
            Ok(CanonicalNesting {
                parent: locator("nesting.parent", &relation.parent)?,
                child: locator("nesting.child", &relation.child)?,
            })
        })
        .collect::<Result<Vec<_>, WorkflowError>>()?;
    nesting.sort();
    if let Some(pair) = nesting.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(WorkflowError::DuplicateValue {
            field: "nesting",
            value: format!("{}->{}", pair[0].parent, pair[0].child),
        });
    }

    Ok(CanonicalWorkflowContent {
        coordination_contract: BOUNDED_COORDINATION_CONTRACT.to_string(),
        workflow_key,
        units,
        barriers,
        nesting,
    })
}

/// Validate one source unit for source-editor diagnostics. Whole-graph validation
/// remains in compile_workflow; this does not approve dependencies in isolation.
pub fn validate_workflow_unit(unit: &WorkflowUnitSource) -> Result<(), WorkflowError> {
    canonical_unit(unit).map(|_| ())
}

fn canonical_inputs(
    unit: &WorkflowUnitSource,
) -> Result<Vec<crate::workflow_inputs::WorkflowInputSource>, WorkflowError> {
    if unit.inputs.len() > 128 {
        return Err(WorkflowError::InvalidBarrier(
            "inputs are limited to 128 routes".into(),
        ));
    }
    let mut result = unit.inputs.clone();
    for input in &result {
        locator("inputs.predecessor", &input.predecessor)?;
        validate_ref_string("inputs.receivingContextRef", &input.receiving_context_ref)?;
        if !unit.dependencies.contains(&input.predecessor) {
            return Err(WorkflowError::DanglingUnit {
                field: "inputs.predecessor (must be a declared dependency)",
                key: input.predecessor.clone(),
            });
        }
    }
    result.sort();
    if result.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(WorkflowError::DuplicateValue {
            field: "inputs",
            value: unit.key.clone(),
        });
    }
    Ok(result)
}

fn canonical_unit(unit: &WorkflowUnitSource) -> Result<CanonicalUnit, WorkflowError> {
    if let Some(contribution) = &unit.contribution {
        contribution.validate()?;
    }
    let agent_requirements = CompiledAgentRequirements {
        agent_refs: normalized_refs(
            "agentRequirements.agentRefs",
            &unit.agent_requirements.agent_refs,
        )?,
        agent_set_refs: normalized_refs(
            "agentRequirements.agentSetRefs",
            &unit.agent_requirements.agent_set_refs,
        )?,
        agency_refs: normalized_refs(
            "agentRequirements.agencyRefs",
            &unit.agent_requirements.agency_refs,
        )?,
    };
    if agent_requirements.agent_refs.is_empty()
        && agent_requirements.agent_set_refs.is_empty()
        && agent_requirements.agency_refs.is_empty()
    {
        return Err(WorkflowError::EmptyCollection("agentRequirements"));
    }
    let praxis_refs = normalized_refs("praxisRefs", &unit.praxis_refs)?;
    if praxis_refs.is_empty() {
        return Err(WorkflowError::EmptyCollection("praxisRefs"));
    }
    let capability_refs = normalized_refs("capabilityRefs", &unit.capability_refs)?;
    if capability_refs.is_empty() {
        return Err(WorkflowError::EmptyCollection("capabilityRefs"));
    }
    let permitted_effects = normalized_values("permittedEffects", &unit.permitted_effects)?;
    if permitted_effects.is_empty() {
        return Err(WorkflowError::EmptyCollection("permittedEffects"));
    }
    let verification_obligations =
        normalized_values("verificationObligations", &unit.verification_obligations)?;
    if verification_obligations.is_empty() {
        return Err(WorkflowError::EmptyCollection("verificationObligations"));
    }
    if let Some(binding) = &unit.composition {
        binding
            .validate_for_unit(unit.subject_ref.as_str(), &agent_requirements)
            .map_err(|error| WorkflowError::InvalidComposition {
                unit: unit.key.clone(),
                reason: error.to_string(),
            })?;
    }

    Ok(CanonicalUnit {
        key: locator("units.key", &unit.key)?,
        developmental_concern: required("units.developmentalConcern", &unit.developmental_concern)?,
        required_difference: required("units.requiredDifference", &unit.required_difference)?,
        return_contract: required("units.returnContract", &unit.return_contract)?,
        subject_ref: unit.subject_ref.clone(),
        basis_revision: required("units.basisRevision", &unit.basis_revision)?,
        agent_requirements,
        praxis_refs,
        capability_refs,
        dependencies: normalized_locators("dependencies", &unit.dependencies)?,
        independence_from: normalized_locators("independenceFrom", &unit.independence_from)?,
        permitted_effects,
        verification_obligations,
        return_address: validate_ref_string("returnAddress", &unit.return_address)?,
        stop_conditions: required("units.stopConditions", &unit.stop_conditions)?,
        escalation_conditions: required("units.escalationConditions", &unit.escalation_conditions)?,
        contribution: unit.contribution.clone(),
        inputs: canonical_inputs(unit)?,
        composition: unit.composition.clone(),
    })
}

fn validate_unit_relations(units: &BTreeMap<String, CanonicalUnit>) -> Result<(), WorkflowError> {
    for (key, unit) in units {
        for dependency in &unit.dependencies {
            ensure_unit("dependencies", dependency, units)?;
            if dependency == key {
                return Err(WorkflowError::SelfDependency(key.clone()));
            }
        }
        for independent in &unit.independence_from {
            ensure_unit("independenceFrom", independent, units)?;
            if independent == key || unit.dependencies.contains(independent) {
                return Err(WorkflowError::ContradictoryRelations {
                    unit: key.clone(),
                    key: independent.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_barrier_sources(
    barriers: &[CanonicalBarrier],
    units: &BTreeMap<String, CanonicalUnit>,
) -> Result<(), WorkflowError> {
    for barrier in barriers {
        if barrier.waits_for.is_empty()
            || barrier.releases.is_empty()
            || !barrier.waits_for.is_disjoint(&barrier.releases)
        {
            return Err(WorkflowError::InvalidBarrier(barrier.key.clone()));
        }
        for key in &barrier.waits_for {
            ensure_unit("barriers.waitsFor", key, units)?;
        }
        for key in &barrier.releases {
            ensure_unit("barriers.releases", key, units)?;
        }
    }
    Ok(())
}

fn validate_requires_acyclic(
    units: &BTreeMap<String, CanonicalUnit>,
    barriers: &[CanonicalBarrier],
) -> Result<(), WorkflowError> {
    let mut graph = units
        .iter()
        .map(|(key, unit)| (key.clone(), unit.dependencies.clone()))
        .collect::<BTreeMap<_, _>>();
    for barrier in barriers {
        let gate = format!("@barrier/{}", barrier.key);
        graph.insert(gate.clone(), barrier.waits_for.clone());
        for released in &barrier.releases {
            graph
                .get_mut(released)
                .expect("released unit validated")
                .insert(gate.clone());
        }
    }
    validate_string_graph(&graph, false)
}

fn validate_nesting_sources(
    nesting: &[CanonicalNesting],
    units: &BTreeMap<String, CanonicalUnit>,
) -> Result<(), WorkflowError> {
    let mut graph = units
        .keys()
        .cloned()
        .map(|key| (key, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for relation in nesting {
        ensure_unit("nesting.parent", &relation.parent, units)?;
        ensure_unit("nesting.child", &relation.child, units)?;
        graph
            .get_mut(&relation.parent)
            .expect("parent unit validated")
            .insert(relation.child.clone());
    }
    validate_string_graph(&graph, true)
}

fn validate_string_graph(
    graph: &BTreeMap<String, BTreeSet<String>>,
    nesting: bool,
) -> Result<(), WorkflowError> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for key in graph.keys() {
        visit_graph(
            key,
            graph,
            &mut visiting,
            &mut visited,
            &mut Vec::new(),
            nesting,
        )?;
    }
    Ok(())
}

fn visit_graph(
    key: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    path: &mut Vec<String>,
    nesting: bool,
) -> Result<(), WorkflowError> {
    if visited.contains(key) {
        return Ok(());
    }
    if !visiting.insert(key.to_string()) {
        path.push(key.to_string());
        return Err(if nesting {
            WorkflowError::NestingCycle(path.clone())
        } else {
            WorkflowError::DependencyCycle(path.clone())
        });
    }
    path.push(key.to_string());
    for dependency in &graph[key] {
        visit_graph(dependency, graph, visiting, visited, path, nesting)?;
    }
    path.pop();
    visiting.remove(key);
    visited.insert(key.to_string());
    Ok(())
}

fn compile_barriers(
    barriers: &[CanonicalBarrier],
    refs: &BTreeMap<String, WorkflowUnitRef>,
) -> Result<Vec<CompiledWorkflowBarrier>, WorkflowError> {
    barriers
        .iter()
        .map(|barrier| {
            Ok(CompiledWorkflowBarrier {
                key: barrier.key.clone(),
                waits_for: resolve_keys("barriers.waitsFor", &barrier.waits_for, refs)?,
                releases: resolve_keys("barriers.releases", &barrier.releases, refs)?,
            })
        })
        .collect()
}

fn compile_nesting(
    nesting: &[CanonicalNesting],
    refs: &BTreeMap<String, WorkflowUnitRef>,
) -> Result<Vec<CompiledWorkflowNesting>, WorkflowError> {
    nesting
        .iter()
        .map(|relation| {
            Ok(CompiledWorkflowNesting {
                parent: refs[&relation.parent].clone(),
                child: refs[&relation.child].clone(),
            })
        })
        .collect()
}

fn derive_unit_ref(
    source_ref: &Ref,
    source_revision: &str,
    source_digest: &str,
    workflow_key: &str,
    unit: &CanonicalUnit,
) -> Result<WorkflowUnitRef, WorkflowError> {
    let unit_bytes = serde_json::to_vec(unit)
        .map_err(|error| WorkflowError::Serialization(error.to_string()))?;
    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, WORKFLOW_UNIT_IDENTITY_ALGORITHM.as_bytes());
    frame(&mut hasher, source_ref.to_string().as_bytes());
    frame(&mut hasher, source_revision.as_bytes());
    frame(&mut hasher, source_digest.as_bytes());
    frame(&mut hasher, workflow_key.as_bytes());
    frame(&mut hasher, &unit_bytes);
    let digest = hasher.finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    WorkflowUnitRef::try_from(
        Ref::new("workflow-unit", Ulid::from_bytes(id))
            .expect("the workflow-unit kind is statically valid"),
    )
    .map_err(|error| WorkflowError::Serialization(error.to_string()))
}

fn frame(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn resolve_keys(
    field: &'static str,
    keys: &BTreeSet<String>,
    refs: &BTreeMap<String, WorkflowUnitRef>,
) -> Result<BTreeSet<WorkflowUnitRef>, WorkflowError> {
    keys.iter()
        .map(|key| {
            refs.get(key)
                .cloned()
                .ok_or_else(|| WorkflowError::DanglingUnit {
                    field,
                    key: key.clone(),
                })
        })
        .collect()
}

fn ensure_unit(
    field: &'static str,
    key: &str,
    units: &BTreeMap<String, CanonicalUnit>,
) -> Result<(), WorkflowError> {
    if units.contains_key(key) {
        Ok(())
    } else {
        Err(WorkflowError::DanglingUnit {
            field,
            key: key.to_string(),
        })
    }
}

fn unit_node_id(key: &str) -> NodeId {
    NodeId::new(format!("work-{key}")).expect("validated locator yields a valid work node id")
}

fn barrier_node_id(key: &str) -> NodeId {
    NodeId::new(format!("barrier-{key}")).expect("validated locator yields a valid barrier node id")
}

fn required(field: impl Into<String>, value: &str) -> Result<String, WorkflowError> {
    let field = field.into();
    let value = normalize(value);
    if value.is_empty() {
        Err(WorkflowError::EmptyField(field))
    } else {
        Ok(value)
    }
}

fn locator(field: &'static str, value: &str) -> Result<String, WorkflowError> {
    let value = normalize(value);
    NodeId::new(value.clone()).map_err(|_| WorkflowError::InvalidLocator {
        field,
        value: value.clone(),
    })?;
    Ok(value)
}

fn normalized_locators(
    field: &'static str,
    values: &[String],
) -> Result<BTreeSet<String>, WorkflowError> {
    collect_unique(field, values, |value| locator(field, value))
}

fn normalized_values(
    field: &'static str,
    values: &[String],
) -> Result<BTreeSet<String>, WorkflowError> {
    collect_unique(field, values, |value| required(field, value))
}

fn normalized_refs(
    field: &'static str,
    values: &[String],
) -> Result<BTreeSet<String>, WorkflowError> {
    collect_unique(field, values, |value| validate_ref_string(field, value))
}

fn validate_ref_string(field: &'static str, value: &str) -> Result<String, WorkflowError> {
    crate::workflow_reference::validate_qualified_reference(value).map_err(|_| {
        WorkflowError::InvalidReference {
            field,
            value: value.to_owned(),
        }
    })?;
    Ok(value.to_owned())
}

fn collect_unique(
    field: &'static str,
    values: &[String],
    normalize_value: impl Fn(&str) -> Result<String, WorkflowError>,
) -> Result<BTreeSet<String>, WorkflowError> {
    let mut result = BTreeSet::new();
    for value in values {
        let value = normalize_value(value)?;
        if !result.insert(value.clone()) {
            return Err(WorkflowError::DuplicateValue { field, value });
        }
    }
    Ok(result)
}

fn validate_digest(value: &str) -> Result<(), WorkflowError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(WorkflowError::InvalidDigest(value.to_string()))
    }
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| normalize(&value))
        .filter(|value| !value.is_empty())
}
