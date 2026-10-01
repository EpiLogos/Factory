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

/// One explicit lifecycle transition, governed by the same Run authority and
/// revision as topology. Eligibility for closure is checked by the native
/// attempt owner before this core command is applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunLifecycleCommand {
    pub command_id: String,
    pub expected_revision: Revision,
    pub lifecycle: RunLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunLifecycleReceipt {
    pub command: RunLifecycleCommand,
    pub previous_lifecycle: RunLifecycle,
    pub next_revision: Revision,
    pub authority_owner: String,
    pub authority_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunLifecycleOutcome {
    Applied(RunLifecycleReceipt),
    AlreadyApplied(RunLifecycleReceipt),
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    /// Exact commands, rather than only their names, fence changed retries.
    /// Absent on historical Runs; a replay returns its original receipt.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    lifecycle_commands: BTreeMap<String, RunLifecycleReceipt>,
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
            lifecycle_commands: BTreeMap::new(),
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

    /// Archival retains the terminal outcome; the word "archived" alone
    /// cannot establish whether this undertaking finished or was aborted.
    pub fn archived_from(&self) -> Option<RunLifecycle> {
        if self.lifecycle != RunLifecycle::Archived {
            return None;
        }
        self.lifecycle_commands
            .values()
            .filter(|receipt| receipt.command.lifecycle == RunLifecycle::Archived)
            .max_by_key(|receipt| receipt.next_revision.get())
            .map(|receipt| receipt.previous_lifecycle)
            .filter(|lifecycle| matches!(lifecycle, RunLifecycle::Finished | RunLifecycle::Aborted))
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

    /// Validate the lifecycle portion of a provider CAS without accepting a
    /// deserialized enum assignment as an owner command. A lifecycle Action
    /// contains an exact command chain; topology revisions cannot be hidden
    /// between its receipts. Topology-only Actions preserve the whole history.
    pub(crate) fn validate_lifecycle_successor(&self, next: &Run) -> Result<(), RunContractError> {
        if self.reference != next.reference
            || self.write_authority != next.write_authority
            || !self
                .applied_command_ids
                .is_subset(&next.applied_command_ids)
            || self
                .lifecycle_commands
                .iter()
                .any(|(id, receipt)| next.lifecycle_commands.get(id) != Some(receipt))
        {
            return Err(RunContractError::CorruptRun);
        }
        let mut additions = next
            .lifecycle_commands
            .iter()
            .filter(|(id, _)| !self.lifecycle_commands.contains_key(*id))
            .map(|(_, receipt)| receipt)
            .collect::<Vec<_>>();
        additions.sort_by_key(|receipt| receipt.next_revision);
        if additions.is_empty() {
            return if self.lifecycle == next.lifecycle {
                Ok(())
            } else {
                Err(RunContractError::CorruptRun)
            };
        }
        let mut replay = self.clone();
        let authority = replay.mutation_authority();
        for receipt in additions {
            if replay.apply_lifecycle_command(&authority, receipt.command.clone())?
                != RunLifecycleOutcome::Applied(receipt.clone())
            {
                return Err(RunContractError::CorruptRun);
            }
        }
        if replay.lifecycle != next.lifecycle
            || replay.revision != next.revision
            || replay.map != next.map
            || replay.applied_command_ids != next.applied_command_ids
        {
            return Err(RunContractError::CorruptRun);
        }
        Ok(())
    }

    pub fn apply_lifecycle_command(
        &mut self,
        authority: &RunMutationAuthority,
        command: RunLifecycleCommand,
    ) -> Result<RunLifecycleOutcome, RunContractError> {
        self.validate_authority(authority)?;
        if command.command_id.trim().is_empty() {
            return Err(RunContractError::InvalidCommandId);
        }
        if let Some(receipt) = self.lifecycle_commands.get(&command.command_id) {
            if receipt.command != command {
                return Err(RunContractError::LifecycleCommandConflict(
                    command.command_id,
                ));
            }
            return Ok(RunLifecycleOutcome::AlreadyApplied(receipt.clone()));
        }
        if self.applied_command_ids.contains(&command.command_id) {
            return Err(RunContractError::LifecycleCommandConflict(
                command.command_id,
            ));
        }
        if command.expected_revision != self.revision {
            return Err(RunContractError::RevisionConflict {
                expected: command.expected_revision,
                actual: self.revision,
            });
        }
        if !legal_lifecycle_transition(self.lifecycle, command.lifecycle) {
            return Err(RunContractError::InvalidLifecycleTransition {
                from: self.lifecycle,
                to: command.lifecycle,
            });
        }
        let next_revision = self
            .revision
            .next()
            .ok_or(RunContractError::RevisionOverflow)?;
        let receipt = RunLifecycleReceipt {
            command,
            previous_lifecycle: self.lifecycle,
            next_revision,
            authority_owner: authority.owner.clone(),
            authority_epoch: authority.epoch,
        };
        self.lifecycle = receipt.command.lifecycle;
        self.revision = next_revision;
        self.applied_command_ids
            .insert(receipt.command.command_id.clone());
        self.lifecycle_commands
            .insert(receipt.command.command_id.clone(), receipt.clone());
        Ok(RunLifecycleOutcome::Applied(receipt))
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

    /// Consume retained cognition only after its native inputs and useful outputs
    /// have been observed. The same Run authority/revision governs this mutation.
    pub fn apply_thought_consumption<P: super::ThoughtConsumptionSources>(
        &mut self,
        authority: &RunMutationAuthority,
        command: super::RunThoughtConsumptionCommand,
        sources: &P,
    ) -> Result<RunThoughtOutcome, RunContractError> {
        self.validate_authority(authority)?;
        if command.command_id.trim().is_empty() {
            return Err(RunContractError::InvalidCommandId);
        }
        if self.thought_field.consumption_replay(&command)? {
            return Ok(RunThoughtOutcome::AlreadyApplied {
                revision: self.revision,
            });
        }
        if self.applied_command_ids.contains(&command.command_id) {
            return Err(RunContractError::Thought(
                ThoughtFieldError::InvalidConsumption(
                    "command already used by a different Run operation".into(),
                ),
            ));
        }
        if command.expected_revision != self.revision {
            return Err(RunContractError::RevisionConflict {
                expected: command.expected_revision,
                actual: self.revision,
            });
        }
        let next = self
            .revision
            .next()
            .ok_or(RunContractError::RevisionOverflow)?;
        let command_id = command.command_id.clone();
        self.thought_field
            .consume(&self.reference, command, sources)?;
        self.revision = next;
        self.applied_command_ids.insert(command_id);
        Ok(RunThoughtOutcome::Applied { revision: next })
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
        if self
            .thought_field
            .consumptions()
            .any(|receipt| !self.applied_command_ids.contains(&receipt.command_id))
        {
            return Err(RunContractError::CorruptRun);
        }
        for (command_id, receipt) in &self.lifecycle_commands {
            if command_id != &receipt.command.command_id
                || !self.applied_command_ids.contains(command_id)
                || receipt.authority_owner.trim().is_empty()
                || receipt.authority_epoch == 0
                || receipt.command.expected_revision.next() != Some(receipt.next_revision)
                || receipt.next_revision > self.revision
                || !legal_lifecycle_transition(
                    receipt.previous_lifecycle,
                    receipt.command.lifecycle,
                )
            {
                return Err(RunContractError::CorruptRun);
            }
        }
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

fn legal_lifecycle_transition(from: RunLifecycle, to: RunLifecycle) -> bool {
    use RunLifecycle::*;
    matches!(
        (from, to),
        (Seeded, Active | Aborted)
            | (Active, WaitingHuman | Suspended | Finishing | Aborted)
            | (WaitingHuman | Suspended, Active | Aborted)
            | (Finishing, Active | WaitingHuman | Finished | Aborted)
            | (Finished | Aborted, Archived)
    )
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRegistry {
    runs: BTreeMap<RunRef, Run>,
}

impl RunRegistry {
    /// Number of registered runs; read-only telemetry access.
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    /// Registered run refs; read-only telemetry access.
    pub fn refs(&self) -> Vec<RunRef> {
        self.runs.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

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
    LifecycleCommandConflict(String),
    InvalidLifecycleTransition {
        from: RunLifecycle,
        to: RunLifecycle,
    },
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

#[cfg(test)]
mod lifecycle_successor_tests {
    use super::*;

    fn run() -> Run {
        Run::new(
            "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse().unwrap(),
            "project:01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap(),
            "native lifecycle receipt validation",
            "factory",
        )
        .unwrap()
    }

    fn transition(run: &mut Run, id: &str, lifecycle: RunLifecycle) {
        run.apply_lifecycle_command(
            &run.mutation_authority(),
            RunLifecycleCommand {
                command_id: id.into(),
                expected_revision: run.revision(),
                lifecycle,
            },
        )
        .unwrap();
    }

    #[test]
    fn lifecycle_successor_requires_exact_owner_receipts_and_preserved_history() {
        let source = run();
        let mut successor = source.clone();
        transition(&mut successor, "activate", RunLifecycle::Active);
        source.validate_lifecycle_successor(&successor).unwrap();

        let mut forged = source.clone();
        forged.lifecycle = RunLifecycle::Active;
        forged.revision = Revision::new(2).unwrap();
        assert!(source.validate_lifecycle_successor(&forged).is_err());

        let active = successor.clone();
        transition(&mut successor, "wait", RunLifecycle::WaitingHuman);
        active.validate_lifecycle_successor(&successor).unwrap();
        successor.lifecycle_commands.remove("activate");
        assert!(active.validate_lifecycle_successor(&successor).is_err());
    }

    #[test]
    fn lifecycle_receipts_cannot_hide_a_topology_revision_between_commands() {
        let source = run();
        let mut successor = source.clone();
        transition(&mut successor, "activate", RunLifecycle::Active);
        successor
            .apply_topology_command(
                &successor.mutation_authority(),
                RunTopologyCommand {
                    command_id: "add-work".into(),
                    expected_revision: successor.revision(),
                    mutation: TopologyMutation::Batch {
                        mutations: vec![
                            TopologyMutation::AddNode {
                                node: super::super::TopologyNode {
                                    id: super::super::NodeId::new("work-unit").unwrap(),
                                    kind: super::super::NodeKind::Work,
                                    label: "real native topology mutation".into(),
                                    state: Some(super::super::NodeState::Ready),
                                    semantic_ref: None,
                                },
                            },
                            TopologyMutation::AddEdge {
                                edge: super::super::TopologyEdge {
                                    from: super::super::NodeId::new("destination").unwrap(),
                                    to: super::super::NodeId::new("work-unit").unwrap(),
                                    relation: super::super::EdgeKind::BranchesTo,
                                },
                            },
                        ],
                    },
                },
            )
            .unwrap();
        transition(&mut successor, "wait", RunLifecycle::WaitingHuman);
        assert!(source.validate_lifecycle_successor(&successor).is_err());
    }
}
