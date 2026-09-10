use super::{
    ProjectRef, RunMap, RunRef, RunThought, RunThoughtField, ThoughtFieldError, TopologyError,
    TopologyMutation,
};
use crate::core::identity::Revision;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(rename = "ref")]
    reference: ProjectRef,
    revision: Revision,
}

impl Project {
    pub fn new(reference: ProjectRef) -> Self {
        Self {
            reference,
            revision: Revision::INITIAL,
        }
    }

    pub fn reference(&self) -> &ProjectRef {
        &self.reference
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunLifecycle {
    Seeded,
    Active,
    WaitingHuman,
    Suspended,
    Finishing,
    Finished,
    Aborted,
    Archived,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteAuthority {
    owner: String,
    epoch: u64,
}

impl WriteAuthority {
    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RunMutationAuthority {
    run_ref: RunRef,
    owner: String,
    epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTopologyCommand {
    pub command_id: String,
    pub expected_revision: Revision,
    pub mutation: TopologyMutation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunThoughtCommand {
    pub command_id: String,
    pub expected_revision: Revision,
    pub thought: RunThought,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CommandOutcome {
    Applied {
        revision: Revision,
        topology_revision: Revision,
    },
    AlreadyApplied {
        revision: Revision,
        topology_revision: Revision,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RunThoughtOutcome {
    Applied { revision: Revision },
    AlreadyApplied { revision: Revision },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[serde(rename = "ref")]
    reference: RunRef,
    project_ref: ProjectRef,
    revision: Revision,
    destination: String,
    lifecycle: RunLifecycle,
    write_authority: WriteAuthority,
    map: RunMap,
    /// Run-owned cognition. `serde(default)` keeps historical v1 Run records readable;
    /// canonical new writes always materialise the field.
    #[serde(default)]
    thought_field: RunThoughtField,
    applied_command_ids: BTreeSet<String>,
}

impl Run {
    pub fn new(
        reference: RunRef,
        project_ref: ProjectRef,
        destination: impl Into<String>,
        write_owner: impl Into<String>,
    ) -> Result<Self, RunContractError> {
        let destination = destination.into();
        let write_owner = write_owner.into();
        if write_owner.trim().is_empty() {
            return Err(RunContractError::InvalidWriteOwner);
        }
        let map = RunMap::new(reference.clone(), destination.clone())?;
        Ok(Self {
            reference,
            project_ref,
            revision: Revision::INITIAL,
            destination,
            lifecycle: RunLifecycle::Seeded,
            write_authority: WriteAuthority {
                owner: write_owner,
                epoch: 1,
            },
            map,
            thought_field: RunThoughtField::default(),
            applied_command_ids: BTreeSet::new(),
        })
    }

    pub fn reference(&self) -> &RunRef {
        &self.reference
    }

    pub fn project_ref(&self) -> &ProjectRef {
        &self.project_ref
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }

    pub fn lifecycle(&self) -> RunLifecycle {
        self.lifecycle
    }

    pub fn write_authority(&self) -> &WriteAuthority {
        &self.write_authority
    }

    pub fn map(&self) -> &RunMap {
        &self.map
    }

    pub fn thought_field(&self) -> &RunThoughtField {
        &self.thought_field
    }

    pub fn mutation_authority(&self) -> RunMutationAuthority {
        RunMutationAuthority {
            run_ref: self.reference.clone(),
            owner: self.write_authority.owner.clone(),
            epoch: self.write_authority.epoch,
        }
    }

    pub fn apply_topology_command(
        &mut self,
        authority: &RunMutationAuthority,
        command: RunTopologyCommand,
    ) -> Result<CommandOutcome, RunContractError> {
        self.validate_authority(authority)?;
        if command.command_id.trim().is_empty() {
            return Err(RunContractError::InvalidCommandId);
        }
        if self.applied_command_ids.contains(&command.command_id) {
            return Ok(CommandOutcome::AlreadyApplied {
                revision: self.revision,
                topology_revision: self.map.topology_revision(),
            });
        }
        if command.expected_revision != self.revision {
            return Err(RunContractError::RevisionConflict {
                expected: command.expected_revision,
                actual: self.revision,
            });
        }

        let next_map = self.map.apply(command.mutation)?;
        let next_revision = self
            .revision
            .next()
            .ok_or(RunContractError::RevisionOverflow)?;
        self.map = next_map;
        self.revision = next_revision;
        self.applied_command_ids.insert(command.command_id);
        Ok(CommandOutcome::Applied {
            revision: self.revision,
            topology_revision: self.map.topology_revision(),
        })
    }

    /// Retain one source-backed cognitive determination inside this Run.
    ///
    /// This advances Run revision but leaves RunMap topology untouched. The same
    /// Run mutation authority governs retention, so cognition cannot silently
    /// acquire a second write-authority path.
    pub fn apply_thought_command(
        &mut self,
        authority: &RunMutationAuthority,
        command: RunThoughtCommand,
    ) -> Result<RunThoughtOutcome, RunContractError> {
        self.validate_authority(authority)?;
        if command.command_id.trim().is_empty() {
            return Err(RunContractError::InvalidCommandId);
        }
        if self.applied_command_ids.contains(&command.command_id) {
            return Ok(RunThoughtOutcome::AlreadyApplied {
                revision: self.revision,
            });
        }
        if command.expected_revision != self.revision {
            return Err(RunContractError::RevisionConflict {
                expected: command.expected_revision,
                actual: self.revision,
            });
        }

        let next_revision = self
            .revision
            .next()
            .ok_or(RunContractError::RevisionOverflow)?;
        self.thought_field
            .retain(&self.reference, command.thought)?;
        self.revision = next_revision;
        self.applied_command_ids.insert(command.command_id);
        Ok(RunThoughtOutcome::Applied {
            revision: self.revision,
        })
    }

    pub fn transfer_write_authority(
        &mut self,
        authority: &RunMutationAuthority,
        expected_revision: Revision,
        new_owner: impl Into<String>,
    ) -> Result<RunMutationAuthority, RunContractError> {
        self.validate_authority(authority)?;
        if expected_revision != self.revision {
            return Err(RunContractError::RevisionConflict {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let new_owner = new_owner.into();
        if new_owner.trim().is_empty() {
            return Err(RunContractError::InvalidWriteOwner);
        }
        self.write_authority.owner = new_owner;
        self.write_authority.epoch = self
            .write_authority
            .epoch
            .checked_add(1)
            .ok_or(RunContractError::AuthorityEpochOverflow)?;
        self.revision = self
            .revision
            .next()
            .ok_or(RunContractError::RevisionOverflow)?;
        Ok(self.mutation_authority())
    }

    pub(crate) fn validate(&self) -> Result<(), RunContractError> {
        if self.destination.trim().is_empty() || self.write_authority.owner.trim().is_empty() {
            return Err(RunContractError::CorruptRun);
        }
        if self.write_authority.epoch == 0 || self.map.run_ref() != &self.reference {
            return Err(RunContractError::CorruptRun);
        }
        self.map.validate()?;
        self.thought_field.validate(&self.reference)?;
        Ok(())
    }

    fn validate_authority(&self, authority: &RunMutationAuthority) -> Result<(), RunContractError> {
        if authority.run_ref != self.reference
            || authority.owner != self.write_authority.owner
            || authority.epoch != self.write_authority.epoch
        {
            return Err(RunContractError::InvalidMutationAuthority);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRegistry {
    runs: BTreeMap<RunRef, Run>,
}

impl RunRegistry {
    pub fn insert(&mut self, run: Run) -> Result<(), RunContractError> {
        run.validate()?;
        if self.runs.contains_key(run.reference()) {
            return Err(RunContractError::DuplicateCanonicalRunMap(
                run.reference().clone(),
            ));
        }
        self.runs.insert(run.reference().clone(), run);
        Ok(())
    }

    pub fn get(&self, run_ref: &RunRef) -> Option<&Run> {
        self.runs.get(run_ref)
    }

    pub fn get_mut(&mut self, run_ref: &RunRef) -> Option<&mut Run> {
        self.runs.get_mut(run_ref)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(serialized: &str) -> Result<Self, RunContractError> {
        let registry: Self = serde_json::from_str(serialized)
            .map_err(|error| RunContractError::CorruptRegistry(error.to_string()))?;
        for (key, run) in &registry.runs {
            if key != run.reference() {
                return Err(RunContractError::CorruptRegistry(
                    "Run registry key does not match Run Ref".to_owned(),
                ));
            }
            run.validate()?;
        }
        Ok(registry)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunContractError {
    InvalidWriteOwner,
    InvalidCommandId,
    InvalidMutationAuthority,
    RevisionConflict {
        expected: Revision,
        actual: Revision,
    },
    DuplicateCanonicalRunMap(RunRef),
    RevisionOverflow,
    AuthorityEpochOverflow,
    MissingCanonicalRunRef {
        provider: String,
        external_id: String,
    },
    CorruptRun,
    CorruptRegistry(String),
    Topology(TopologyError),
    Thought(ThoughtFieldError),
}

impl From<TopologyError> for RunContractError {
    fn from(error: TopologyError) -> Self {
        Self::Topology(error)
    }
}

impl From<ThoughtFieldError> for RunContractError {
    fn from(error: ThoughtFieldError) -> Self {
        Self::Thought(error)
    }
}

impl Display for RunContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Run contract error: {self:?}")
    }
}

impl Error for RunContractError {}
