//! Consuming source-backed cognition in the existing Run, not a second store.
//!
//! Outputs already belong to native owners. Factory checks their observed source
//! revisions, joins them to the original thought and retires redundant active
//! material. This is neither a source write nor a grant to train/promote a model.
use super::{RunRef, RunThoughtField, RunThoughtId, RunThoughtLifecycle, ThoughtFieldError};
use crate::core::identity::Revision;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const THOUGHT_CONSUMPTION_CONTRACT: &str = "factory.run-thought-consumption/v1";
const MAX_ITEMS: usize = 1024;

fn require(ok: bool, message: &str) -> Result<(), ThoughtFieldError> {
    if ok {
        Ok(())
    } else {
        Err(ThoughtFieldError::InvalidConsumption(message.into()))
    }
}
fn reference(value: &str) -> Result<(), ThoughtFieldError> {
    require(
        !value.is_empty() && value == value.trim() && value.len() <= 16384 && !value.contains('\0'),
        "invalid consumption reference",
    )
}

/// Native source identity is not a Factory artifact identity or a guessed path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThoughtSource {
    pub owner: String,
    pub reference: String,
    pub revision: String,
}
impl ThoughtSource {
    fn validate(&self) -> Result<(), ThoughtFieldError> {
        for value in [&self.owner, &self.reference, &self.revision] {
            reference(value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ThoughtSourceObservation {
    Current {
        source: ThoughtSource,
        receipt: ThoughtSource,
    },
    Unavailable {
        reason: String,
    },
    Denied {
        reason: String,
    },
    Changed {
        observed: ThoughtSource,
    },
}

/// Implemented at the native source/Knowledge/Activity boundary. Read the owner;
/// do not manufacture Current by echoing the requested revision. A source may
/// be inspectable without being writable or permitted as a training input.
pub trait ThoughtConsumptionSources {
    fn observe(
        &self,
        source: &ThoughtSource,
    ) -> Result<ThoughtSourceObservation, ThoughtFieldError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThoughtConsumptionInput {
    pub thought_id: RunThoughtId,
    pub anchor: ThoughtSource,
    /// Optional original vocabulary interpretation, e.g. a source-qualified T3
    /// or T3-prime reading. Factory neither relabels these nor owns their grammar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpretation: Option<ThoughtSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThoughtUseKind {
    Context,
    WikiReading,
    Evaluation,
    SkillImprovement,
    RecognisedPraxis,
    ContinuingQuestion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThoughtUse {
    pub kind: ThoughtUseKind,
    pub source: ThoughtSource,
    /// Native applicability, receiving or Recognition evidence, not a boolean
    /// that invents source authority. A Skill remains its native Skill identity.
    pub receiving: ThoughtSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThoughtConsumption {
    pub consumption_id: RunThoughtId,
    pub run_ref: RunRef,
    pub consumer_ref: String,
    /// The actual working field (Central NOW in Epi), independently revisioned.
    pub working_field: ThoughtSource,
    pub inputs: Vec<ThoughtConsumptionInput>,
    /// Actual retrieval, output or execution evidence against which reports were
    /// assessed. Human response is optional but never fabricated when absent.
    pub actual_evidence: Vec<ThoughtSource>,
    pub human_response: Vec<ThoughtSource>,
    pub assessment: ThoughtSource,
    pub uses: Vec<ThoughtUse>,
    pub retention_policy: ThoughtSource,
    pub resulting_lifecycle: RunThoughtLifecycle,
}

impl ThoughtConsumption {
    fn sources(&self) -> BTreeSet<&ThoughtSource> {
        let mut sources = BTreeSet::from([
            &self.working_field,
            &self.assessment,
            &self.retention_policy,
        ]);
        for input in &self.inputs {
            sources.insert(&input.anchor);
            sources.extend(input.interpretation.iter());
        }
        sources.extend(&self.actual_evidence);
        sources.extend(&self.human_response);
        for output in &self.uses {
            sources.extend([&output.source, &output.receiving]);
        }
        sources
    }

    fn validate(
        &self,
        owning_run: &RunRef,
        field: &RunThoughtField,
    ) -> Result<(), ThoughtFieldError> {
        require(
            &self.run_ref == owning_run,
            "consumption belongs to another Run",
        )?;
        reference(&self.consumer_ref)?;
        require(
            !self.inputs.is_empty() && self.inputs.len() <= MAX_ITEMS,
            "consumption requires a bounded nonempty input set",
        )?;
        require(
            !self.actual_evidence.is_empty()
                && self.actual_evidence.len() <= MAX_ITEMS
                && self.human_response.len() <= MAX_ITEMS
                && !self.uses.is_empty()
                && self.uses.len() <= MAX_ITEMS,
            "consumption needs actual evidence and useful output",
        )?;
        require(
            self.resulting_lifecycle != RunThoughtLifecycle::Active,
            "consumption must remove redundant material from the active field",
        )?;
        let mut inputs = BTreeSet::new();
        for input in &self.inputs {
            require(
                inputs.insert(&input.thought_id),
                "duplicate consumption input",
            )?;
            let thought = field.get(&input.thought_id).ok_or_else(|| {
                ThoughtFieldError::InvalidConsumption("unknown consumption input".into())
            })?;
            require(
                thought.anchor_ref == input.anchor.reference
                    && thought.anchor_revision.as_deref() == Some(input.anchor.revision.as_str()),
                "consumption must identify the exact retained source revision",
            )?;
        }
        let mut outputs = BTreeSet::new();
        for output in &self.uses {
            require(
                outputs.insert(&output.source),
                "duplicate consumption output",
            )?;
            require(
                !self.inputs.iter().any(|i| {
                    i.anchor.reference == output.source.reference
                        && i.anchor.revision == output.source.revision
                }),
                "repeating the input is not consuming it into a new result",
            )?;
        }
        for source in self.sources() {
            source.validate()?;
        }
        Ok(())
    }
}

/// Immutable historical receipt inside RunThoughtField. Current source freshness
/// is re-observed for each subsequent use, not inferred from this old receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThoughtConsumptionReceipt {
    pub contract: String,
    pub command_id: String,
    pub consumption: ThoughtConsumption,
    pub observations: Vec<ThoughtSourceObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunThoughtConsumptionCommand {
    pub command_id: String,
    pub expected_revision: Revision,
    pub consumption: ThoughtConsumption,
}

impl RunThoughtField {
    /// Active projection is a view of the same retained field. Source-backed
    /// reports and their consumption lineage remain available for exact Return.
    pub fn active(&self) -> impl Iterator<Item = &super::RunThought> {
        self.thoughts
            .values()
            .filter(|t| t.lifecycle == RunThoughtLifecycle::Active)
    }

    pub fn consumptions(&self) -> impl Iterator<Item = &ThoughtConsumptionReceipt> {
        self.consumptions.values()
    }

    pub fn consumed_by(&self, id: &RunThoughtId) -> Option<&ThoughtConsumptionReceipt> {
        self.consumptions.values().find(|receipt| {
            receipt
                .consumption
                .inputs
                .iter()
                .any(|input| &input.thought_id == id)
        })
    }

    pub(crate) fn consumption_replay(
        &self,
        command: &RunThoughtConsumptionCommand,
    ) -> Result<bool, ThoughtFieldError> {
        let existing = self.consumptions.values().find(|receipt| {
            receipt.command_id == command.command_id
                || receipt.consumption.consumption_id == command.consumption.consumption_id
        });
        match existing {
            None => Ok(false),
            Some(receipt) => {
                require(
                    receipt.command_id == command.command_id
                        && receipt.consumption == command.consumption,
                    "consumption identity or command reused with a different payload",
                )?;
                Ok(true)
            }
        }
    }

    pub(crate) fn consume<P: ThoughtConsumptionSources>(
        &mut self,
        owning_run: &RunRef,
        command: RunThoughtConsumptionCommand,
        sources: &P,
    ) -> Result<(), ThoughtFieldError> {
        let consumption = command.consumption;
        consumption.validate(owning_run, self)?;
        for input in &consumption.inputs {
            require(
                self.get(&input.thought_id)
                    .is_some_and(|thought| thought.lifecycle == RunThoughtLifecycle::Active)
                    && self.consumed_by(&input.thought_id).is_none(),
                "input has already been consumed or is not active",
            )?;
        }
        // Observe everything before mutating any lifecycle; failure is atomic.
        let mut observations = Vec::new();
        for expected in consumption.sources() {
            let observation = sources.observe(expected)?;
            if let ThoughtSourceObservation::Current { source, receipt } = &observation {
                require(
                    source == expected,
                    "native observation changed the requested source",
                )?;
                receipt.validate()?;
            } else {
                return Err(ThoughtFieldError::InvalidConsumption(
                    "consumption source is denied, changed or unavailable".into(),
                ));
            }
            observations.push(observation);
        }
        for input in &consumption.inputs {
            self.thoughts
                .get_mut(&input.thought_id)
                .expect("validated retained thought")
                .lifecycle = consumption.resulting_lifecycle;
        }
        self.consumptions.insert(
            consumption.consumption_id.clone(),
            ThoughtConsumptionReceipt {
                contract: THOUGHT_CONSUMPTION_CONTRACT.into(),
                command_id: command.command_id,
                consumption,
                observations,
            },
        );
        Ok(())
    }

    pub(crate) fn validate_consumptions(
        &self,
        owning_run: &RunRef,
    ) -> Result<(), ThoughtFieldError> {
        let mut commands = BTreeSet::new();
        let mut consumed = BTreeSet::new();
        for (key, receipt) in &self.consumptions {
            require(
                receipt.contract == THOUGHT_CONSUMPTION_CONTRACT
                    && key == &receipt.consumption.consumption_id,
                "corrupt consumption contract or identity",
            )?;
            reference(&receipt.command_id)?;
            require(
                commands.insert(&receipt.command_id),
                "duplicate consumption command",
            )?;
            receipt.consumption.validate(owning_run, self)?;
            let expected = receipt.consumption.sources();
            let mut observed = BTreeSet::new();
            require(
                receipt.observations.len() == expected.len(),
                "incomplete consumption observation",
            )?;
            for observation in &receipt.observations {
                if let ThoughtSourceObservation::Current { source, receipt } = observation {
                    source.validate()?;
                    receipt.validate()?;
                    require(
                        expected.contains(source) && observed.insert(source),
                        "foreign or duplicate source observation",
                    )?;
                } else {
                    return Err(ThoughtFieldError::InvalidConsumption(
                        "stored consumption lacks original current-source evidence".into(),
                    ));
                }
            }
            for input in &receipt.consumption.inputs {
                require(
                    consumed.insert(&input.thought_id)
                        && self.get(&input.thought_id).is_some_and(|thought| {
                            thought.lifecycle == receipt.consumption.resulting_lifecycle
                        }),
                    "corrupt consumption lineage or lifecycle",
                )?;
            }
        }
        Ok(())
    }
}
