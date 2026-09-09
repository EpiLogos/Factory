//! Factory-owned self-hosting Commission and closed developmental mutations.
//!
//! Central supplies composition evidence; it does not mint Factory identity or
//! confer Actuation authority. Commission creates only developmental state.

use crate::build::FactoryBuildState;
use crate::core::identity::{Ref, Revision};
use crate::core::run::{Project, ProjectRef, Run, RunRef};
use crate::developmental_read::FactoryDevelopmentalState;
use crate::journey::{
    Journey, JourneyCommission, JourneyParticipant, JourneyRecognitionLink, JourneyRef,
    JourneyReturn,
};
use crate::workflow::{compile_workflow, WorkflowSource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use ulid::Ulid;

pub const FACTORY_COMMISSION_REQUEST: &str = "factory.commission-request/v1";
pub const FACTORY_COMMISSION_CONTRACT: &str = "factory.commission/v1";
pub const FACTORY_COMMISSION_RECEIPT: &str = "factory.commission-receipt/v1";
pub const FACTORY_COMMISSION_READING: &str = "factory.commission-reading/v1";
pub const FACTORY_DEVELOPMENTAL_MUTATION_REQUEST: &str =
    "factory.developmental-mutation-request/v1";
pub const FACTORY_DEVELOPMENTAL_MUTATION_RECORD: &str = "factory.developmental-mutation/v1";
pub const FACTORY_DEVELOPMENTAL_MUTATION_RECEIPT: &str =
    "factory.developmental-mutation-receipt/v1";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CentralSourceRecordReceipt {
    #[serde(rename = "ref")]
    pub record_ref: String,
    pub revision: String,
    pub source_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CentralAgentCompositionEvidence {
    pub owner: String,
    pub development_agent_set: CentralSourceRecordReceipt,
    pub guardian_agent_set: CentralSourceRecordReceipt,
    pub resolved_agent_refs: Vec<String>,
    pub agent_profile_receipts: Vec<CentralSourceRecordReceipt>,
    pub authority_standing: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundedRootAct {
    pub act_ref: String,
    pub agent_ref: String,
    pub purpose: String,
    pub scope_refs: Vec<String>,
    pub standing: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryCommissionRequest {
    pub contract: String,
    pub request_ref: String,
    pub project_key: String,
    pub purpose: String,
    pub frontier: String,
    pub run_destination: String,
    pub write_owner: String,
    pub commissioned_at: String,
    pub central_composition: CentralAgentCompositionEvidence,
    pub root_act: BoundedRootAct,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryCommission {
    pub contract: String,
    pub revision: Revision,
    pub request: FactoryCommissionRequest,
    pub project_ref: ProjectRef,
    pub journey_ref: JourneyRef,
    pub run_ref: RunRef,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryAdmissionStatus {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryCommissionReceipt {
    pub contract: String,
    pub status: FactoryAdmissionStatus,
    pub commission: FactoryCommission,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryCommissionEdge {
    pub subject_ref: String,
    pub relation: String,
    pub object_ref: String,
    pub owner: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryCommissionReading {
    pub contract: String,
    pub commission: FactoryCommission,
    pub traversal: Vec<FactoryCommissionEdge>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryMutationSource {
    pub owner: String,
    pub reference: String,
    pub revision: String,
    pub standing: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    tag = "kind",
    deny_unknown_fields
)]
pub enum FactoryDevelopmentalMutation {
    AttachWorkflowSource {
        run_ref: RunRef,
        workflow_source: WorkflowSource,
    },
    CorrelateOwnerActivity {
        journey_ref: JourneyRef,
        activity_ref: String,
    },
    RecordOwnerReturn {
        journey_ref: JourneyRef,
        returned: JourneyReturn,
    },
    RecordOwnerRecognition {
        journey_ref: JourneyRef,
        recognition: JourneyRecognitionLink,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryDevelopmentalMutationRequest {
    pub contract: String,
    pub mutation_ref: String,
    pub occurrence_ref: String,
    pub source: FactoryMutationSource,
    pub observed_at: String,
    pub mutation: FactoryDevelopmentalMutation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryDevelopmentalMutationRecord {
    pub contract: String,
    pub request: FactoryDevelopmentalMutationRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryDevelopmentalMutationReceipt {
    pub contract: String,
    pub status: FactoryAdmissionStatus,
    pub record: FactoryDevelopmentalMutationRecord,
}

impl FactoryCommissionRequest {
    pub fn validate(&self) -> Result<(), CommissionError> {
        if self.contract != FACTORY_COMMISSION_REQUEST {
            return Err(CommissionError::Invalid("contract".into()));
        }
        stable_ref(&self.request_ref, "requestRef")?;
        stable_ref(&self.project_key, "projectKey")?;
        for (name, value) in [
            ("purpose", &self.purpose),
            ("frontier", &self.frontier),
            ("runDestination", &self.run_destination),
        ] {
            required(value, name)?;
        }
        if self.write_owner != "factory" {
            return Err(CommissionError::Invalid("writeOwner".into()));
        }
        let moment = timestamp(&self.commissioned_at, "commissionedAt")?;
        timestamp_ms(moment)?;
        let composition = &self.central_composition;
        if composition.owner != "central"
            || composition.authority_standing != "membership-non-authoritative"
        {
            return Err(CommissionError::Invalid("centralComposition".into()));
        }
        if composition.development_agent_set.record_ref == composition.guardian_agent_set.record_ref
        {
            return Err(CommissionError::Invalid(
                "centralComposition.agentSetReceipt".into(),
            ));
        }
        for receipt in [
            &composition.development_agent_set,
            &composition.guardian_agent_set,
        ] {
            stable_ref(&receipt.record_ref, "agentSetReceipt.ref")?;
            stable_ref(&receipt.revision, "agentSetReceipt.revision")?;
            if !receipt.source_path.starts_with("Control/")
                || receipt.source_path.contains("..")
                || receipt.source_path.chars().any(char::is_whitespace)
                || !lower_hex_digest(&receipt.sha256)
            {
                return Err(CommissionError::Invalid(
                    "centralComposition.agentSetReceipt".into(),
                ));
            }
        }
        unique_required(
            &composition.resolved_agent_refs,
            "centralComposition.resolvedAgentRefs",
        )?;
        if composition
            .resolved_agent_refs
            .iter()
            .any(|value| !(value.starts_with("agent/") || value.starts_with("agent:")))
        {
            return Err(CommissionError::Invalid(
                "centralComposition.resolvedAgentRefs".into(),
            ));
        }
        if !composition
            .resolved_agent_refs
            .contains(&self.root_act.agent_ref)
            || self.root_act.standing != "commissioned-not-executed"
        {
            return Err(CommissionError::Invalid("rootAct".into()));
        }
        let mut profiles = BTreeSet::new();
        for receipt in &composition.agent_profile_receipts {
            stable_ref(&receipt.record_ref, "agentProfileReceipt.ref")?;
            stable_ref(&receipt.revision, "agentProfileReceipt.revision")?;
            if !receipt.record_ref.starts_with("profile/")
                || !receipt.source_path.starts_with("Control/agents/profiles/")
                || receipt.source_path.contains("..")
                || !lower_hex_digest(&receipt.sha256)
                || !profiles.insert(&receipt.record_ref)
            {
                return Err(CommissionError::Invalid(
                    "centralComposition.agentProfileReceipts".into(),
                ));
            }
        }
        stable_ref(&self.root_act.act_ref, "rootAct.actRef")?;
        stable_ref(&self.root_act.agent_ref, "rootAct.agentRef")?;
        required(&self.root_act.purpose, "rootAct.purpose")?;
        unique_required(&self.root_act.scope_refs, "rootAct.scopeRefs")
    }
}

impl FactoryCommission {
    pub fn validate(&self) -> Result<(), CommissionError> {
        if self.contract != FACTORY_COMMISSION_CONTRACT || self.revision != Revision::INITIAL {
            return Err(CommissionError::InvalidStored);
        }
        self.request.validate()?;
        let expected = derive_identities(&self.request)?;
        if (
            self.project_ref.clone(),
            self.journey_ref.clone(),
            self.run_ref.clone(),
        ) != expected
        {
            return Err(CommissionError::InvalidStored);
        }
        Ok(())
    }

    pub fn reading(&self) -> FactoryCommissionReading {
        let central = &self.request.central_composition;
        let mut traversal = vec![
            edge(
                &self.request.request_ref,
                "commissioned",
                &self.project_ref.to_string(),
                "factory",
            ),
            edge(
                &self.project_ref.to_string(),
                "contains",
                &self.journey_ref.to_string(),
                "factory",
            ),
            edge(
                &self.journey_ref.to_string(),
                "bounded-as",
                &self.run_ref.to_string(),
                "factory",
            ),
            edge(
                &self.request.root_act.act_ref,
                "bounds",
                &self.run_ref.to_string(),
                "factory",
            ),
            edge(
                &central.development_agent_set.record_ref,
                "composition-evidence-for",
                &self.journey_ref.to_string(),
                "central",
            ),
            edge(
                &central.guardian_agent_set.record_ref,
                "composition-evidence-for",
                &self.journey_ref.to_string(),
                "central",
            ),
        ];
        traversal.extend(
            [&central.development_agent_set, &central.guardian_agent_set]
                .into_iter()
                .map(|source| {
                    edge(
                        &source.source_path,
                        "supports-membership-reading",
                        &self.request.request_ref,
                        "central",
                    )
                }),
        );
        FactoryCommissionReading {
            contract: FACTORY_COMMISSION_READING.into(),
            commission: self.clone(),
            traversal,
        }
    }

    pub fn validate_against(
        &self,
        state: &FactoryDevelopmentalState,
    ) -> Result<(), CommissionError> {
        self.validate()?;
        if self.project_ref != *state.build.project().reference() {
            return Err(CommissionError::InvalidStored);
        }
        let run = state
            .build
            .run(&self.run_ref)
            .ok_or(CommissionError::InvalidStored)?;
        if run.project_ref() != &self.project_ref
            || run.destination() != self.request.run_destination
            || run.write_authority().owner() != "factory"
        {
            return Err(CommissionError::InvalidStored);
        }
        let journey = state
            .journeys
            .iter()
            .find(|item| item.journey_ref == self.journey_ref)
            .ok_or(CommissionError::InvalidStored)?;
        if journey.project_ref != self.project_ref
            || journey.commission.commission_ref.as_deref() != Some(&self.request.request_ref)
            || journey.commission.purpose != self.request.purpose
            || !journey.runs.iter().any(|link| link.run_ref == self.run_ref)
        {
            return Err(CommissionError::InvalidStored);
        }
        let participants = journey
            .participants
            .iter()
            .map(|item| item.participant_ref.as_str())
            .collect::<BTreeSet<_>>();
        if !participants.contains(
            self.request
                .central_composition
                .development_agent_set
                .record_ref
                .as_str(),
        ) || !participants.contains(
            self.request
                .central_composition
                .guardian_agent_set
                .record_ref
                .as_str(),
        ) || self
            .request
            .central_composition
            .resolved_agent_refs
            .iter()
            .any(|item| !participants.contains(item.as_str()))
        {
            return Err(CommissionError::InvalidStored);
        }
        Ok(())
    }
}

impl FactoryDevelopmentalState {
    pub fn from_commission(
        request: FactoryCommissionRequest,
    ) -> Result<(Self, FactoryCommissionReceipt), CommissionError> {
        request.validate()?;
        let (project_ref, journey_ref, run_ref) = derive_identities(&request)?;
        let project = Project::new(project_ref.clone());
        let run = Run::new(
            run_ref.clone(),
            project_ref.clone(),
            request.run_destination.clone(),
            request.write_owner.clone(),
        )
        .map_err(debug)?;
        let build = FactoryBuildState::new(project, run).map_err(debug)?;
        let mut journey = Journey::new(
            journey_ref.clone(),
            project_ref.clone(),
            JourneyCommission {
                purpose: request.purpose.clone(),
                commission_ref: Some(request.request_ref.clone()),
                why_refs: commission_basis(&request),
            },
            request.frontier.clone(),
            request.commissioned_at.clone(),
        )
        .map_err(debug)?;
        add_composition_participants(&mut journey, &request)?;
        journey
            .add_run(run_ref.clone(), commission_basis(&request), Vec::new())
            .map_err(debug)?;
        let commission = FactoryCommission {
            contract: FACTORY_COMMISSION_CONTRACT.into(),
            revision: Revision::INITIAL,
            request,
            project_ref,
            journey_ref,
            run_ref,
        };
        let mut state = FactoryDevelopmentalState::new(build, vec![journey]).map_err(debug)?;
        state.commissions.push(commission.clone());
        state.validate().map_err(debug)?;
        Ok((
            state,
            FactoryCommissionReceipt {
                contract: FACTORY_COMMISSION_RECEIPT.into(),
                status: FactoryAdmissionStatus::Applied,
                commission,
            },
        ))
    }

    pub fn admit_commission(
        &mut self,
        request: FactoryCommissionRequest,
    ) -> Result<FactoryCommissionReceipt, CommissionError> {
        request.validate()?;
        if let Some(existing) = self
            .commissions
            .iter()
            .find(|item| item.request.request_ref == request.request_ref)
        {
            if existing.request != request {
                return Err(CommissionError::ReplayConflict(request.request_ref));
            }
            return Ok(FactoryCommissionReceipt {
                contract: FACTORY_COMMISSION_RECEIPT.into(),
                status: FactoryAdmissionStatus::AlreadyApplied,
                commission: existing.clone(),
            });
        }
        let (project_ref, journey_ref, run_ref) = derive_identities(&request)?;
        if self.build.project().reference() != &project_ref {
            return Err(CommissionError::ForeignProject);
        }
        let mut candidate = self.clone();
        let run = Run::new(
            run_ref.clone(),
            project_ref.clone(),
            request.run_destination.clone(),
            request.write_owner.clone(),
        )
        .map_err(debug)?;
        candidate.build.insert_run(run).map_err(debug)?;
        let mut journey = Journey::new(
            journey_ref.clone(),
            project_ref.clone(),
            JourneyCommission {
                purpose: request.purpose.clone(),
                commission_ref: Some(request.request_ref.clone()),
                why_refs: commission_basis(&request),
            },
            request.frontier.clone(),
            request.commissioned_at.clone(),
        )
        .map_err(debug)?;
        add_composition_participants(&mut journey, &request)?;
        journey
            .add_run(run_ref.clone(), commission_basis(&request), Vec::new())
            .map_err(debug)?;
        candidate.journeys.push(journey);
        candidate
            .journeys
            .sort_by(|a, b| a.journey_ref.cmp(&b.journey_ref));
        let commission = FactoryCommission {
            contract: FACTORY_COMMISSION_CONTRACT.into(),
            revision: Revision::INITIAL,
            request,
            project_ref,
            journey_ref,
            run_ref,
        };
        candidate.commissions.push(commission.clone());
        candidate
            .commissions
            .sort_by(|a, b| a.request.request_ref.cmp(&b.request.request_ref));
        candidate.validate().map_err(debug)?;
        *self = candidate;
        Ok(FactoryCommissionReceipt {
            contract: FACTORY_COMMISSION_RECEIPT.into(),
            status: FactoryAdmissionStatus::Applied,
            commission,
        })
    }

    pub fn commission_reading(
        &self,
        request_ref: &str,
    ) -> Result<FactoryCommissionReading, CommissionError> {
        self.commissions
            .iter()
            .find(|item| item.request.request_ref == request_ref)
            .map(FactoryCommission::reading)
            .ok_or_else(|| CommissionError::NotFound(request_ref.into()))
    }

    pub fn apply_developmental_mutation(
        &mut self,
        request: FactoryDevelopmentalMutationRequest,
    ) -> Result<FactoryDevelopmentalMutationReceipt, CommissionError> {
        request.validate()?;
        if let Some(existing) = self.developmental_mutations.iter().find(|item| {
            item.request.mutation_ref == request.mutation_ref
                || item.request.occurrence_ref == request.occurrence_ref
        }) {
            if existing.request != request {
                return Err(CommissionError::ReplayConflict(request.mutation_ref));
            }
            return Ok(FactoryDevelopmentalMutationReceipt {
                contract: FACTORY_DEVELOPMENTAL_MUTATION_RECEIPT.into(),
                status: FactoryAdmissionStatus::AlreadyApplied,
                record: existing.clone(),
            });
        }
        let mut candidate = self.clone();
        match &request.mutation {
            FactoryDevelopmentalMutation::AttachWorkflowSource {
                run_ref,
                workflow_source,
            } => {
                let compiled = compile_workflow(workflow_source.clone()).map_err(debug)?;
                if candidate
                    .workflow_sources
                    .iter()
                    .any(|source| source.source.reference == workflow_source.source.reference)
                {
                    return Err(CommissionError::Conflict(
                        "workflow source identity already exists".into(),
                    ));
                }
                let authority = candidate
                    .build
                    .run_mutation_authority(run_ref)
                    .ok_or_else(|| CommissionError::ForeignReference(run_ref.to_string()))?;
                let revision = candidate
                    .build
                    .run(run_ref)
                    .expect("authority requires Run")
                    .revision();
                candidate
                    .build
                    .apply_run_topology_command(
                        run_ref,
                        &authority,
                        compiled.topology_command(revision),
                    )
                    .map_err(debug)?;
                candidate.workflow_sources.push(workflow_source.clone());
                candidate
                    .workflow_sources
                    .sort_by_key(|source| source.source.reference.to_string());
            }
            FactoryDevelopmentalMutation::CorrelateOwnerActivity {
                journey_ref,
                activity_ref,
            } => candidate
                .journey_mut(journey_ref)?
                .correlate_activity(activity_ref.clone())
                .map_err(debug)?,
            FactoryDevelopmentalMutation::RecordOwnerReturn {
                journey_ref,
                returned,
            } => candidate
                .journey_mut(journey_ref)?
                .record_return(returned.clone())
                .map_err(debug)?,
            FactoryDevelopmentalMutation::RecordOwnerRecognition {
                journey_ref,
                recognition,
            } => candidate
                .journey_mut(journey_ref)?
                .recognize(
                    recognition.recognition_ref.clone(),
                    recognition.subject_ref.clone(),
                    recognition.basis_refs.clone(),
                )
                .map_err(debug)?,
        }
        let record = FactoryDevelopmentalMutationRecord {
            contract: FACTORY_DEVELOPMENTAL_MUTATION_RECORD.into(),
            request,
        };
        candidate.developmental_mutations.push(record.clone());
        candidate
            .developmental_mutations
            .sort_by(|a, b| a.request.mutation_ref.cmp(&b.request.mutation_ref));
        candidate.validate().map_err(debug)?;
        *self = candidate;
        Ok(FactoryDevelopmentalMutationReceipt {
            contract: FACTORY_DEVELOPMENTAL_MUTATION_RECEIPT.into(),
            status: FactoryAdmissionStatus::Applied,
            record,
        })
    }

    fn journey_mut(&mut self, reference: &JourneyRef) -> Result<&mut Journey, CommissionError> {
        self.journeys
            .iter_mut()
            .find(|item| &item.journey_ref == reference)
            .ok_or_else(|| CommissionError::ForeignReference(reference.to_string()))
    }
}

impl FactoryDevelopmentalMutationRequest {
    pub fn validate(&self) -> Result<(), CommissionError> {
        if self.contract != FACTORY_DEVELOPMENTAL_MUTATION_REQUEST {
            return Err(CommissionError::Invalid("contract".into()));
        }
        for (name, value) in [
            ("mutationRef", &self.mutation_ref),
            ("occurrenceRef", &self.occurrence_ref),
            ("source.owner", &self.source.owner),
            ("source.reference", &self.source.reference),
            ("source.revision", &self.source.revision),
        ] {
            stable_ref(value, name)?;
        }
        if self.source.standing != "owner-native-observation" {
            return Err(CommissionError::Invalid("source.standing".into()));
        }
        timestamp_ms(timestamp(&self.observed_at, "observedAt")?)?;
        match &self.mutation {
            FactoryDevelopmentalMutation::AttachWorkflowSource {
                workflow_source, ..
            } if self.source.reference != workflow_source.source.reference.to_string() => {
                return Err(CommissionError::Invalid("source.reference".into()))
            }
            FactoryDevelopmentalMutation::CorrelateOwnerActivity { activity_ref, .. }
                if self.source.reference != *activity_ref =>
            {
                return Err(CommissionError::Invalid("source.reference".into()))
            }
            FactoryDevelopmentalMutation::RecordOwnerReturn { returned, .. }
                if !returned.evidence_refs.contains(&self.source.reference)
                    && !returned.basis_refs.contains(&self.source.reference) =>
            {
                return Err(CommissionError::Invalid("return.source".into()))
            }
            FactoryDevelopmentalMutation::RecordOwnerRecognition { recognition, .. }
                if !recognition.basis_refs.contains(&self.source.reference) =>
            {
                return Err(CommissionError::Invalid("recognition.source".into()))
            }
            _ => {}
        }
        Ok(())
    }
}

impl FactoryDevelopmentalMutationRecord {
    pub fn validate_against(
        &self,
        state: &FactoryDevelopmentalState,
    ) -> Result<(), CommissionError> {
        if self.contract != FACTORY_DEVELOPMENTAL_MUTATION_RECORD {
            return Err(CommissionError::InvalidStored);
        }
        self.request.validate()?;
        match &self.request.mutation {
            FactoryDevelopmentalMutation::AttachWorkflowSource {
                run_ref,
                workflow_source,
            } => {
                let compiled = compile_workflow(workflow_source.clone()).map_err(debug)?;
                let run = state
                    .build
                    .run(run_ref)
                    .ok_or(CommissionError::InvalidStored)?;
                if !state
                    .workflow_sources
                    .iter()
                    .any(|item| item == workflow_source)
                {
                    return Err(CommissionError::InvalidStored);
                }
                let semantic_refs = run
                    .map()
                    .nodes()
                    .values()
                    .filter_map(|node| node.semantic_ref.as_ref())
                    .collect::<BTreeSet<_>>();
                if compiled
                    .units
                    .values()
                    .any(|unit| !semantic_refs.contains(unit.reference.as_ref()))
                {
                    return Err(CommissionError::InvalidStored);
                }
            }
            FactoryDevelopmentalMutation::CorrelateOwnerActivity {
                journey_ref,
                activity_ref,
            } => {
                if !state.journeys.iter().any(|item| {
                    &item.journey_ref == journey_ref && item.activity_refs.contains(activity_ref)
                }) {
                    return Err(CommissionError::InvalidStored);
                }
            }
            FactoryDevelopmentalMutation::RecordOwnerReturn {
                journey_ref,
                returned,
            } => {
                if !state
                    .journeys
                    .iter()
                    .any(|item| &item.journey_ref == journey_ref && item.returns.contains(returned))
                {
                    return Err(CommissionError::InvalidStored);
                }
            }
            FactoryDevelopmentalMutation::RecordOwnerRecognition {
                journey_ref,
                recognition,
            } => {
                if !state.journeys.iter().any(|item| {
                    &item.journey_ref == journey_ref && item.recognitions.contains(recognition)
                }) {
                    return Err(CommissionError::InvalidStored);
                }
            }
        }
        Ok(())
    }
}

fn add_composition_participants(
    journey: &mut Journey,
    request: &FactoryCommissionRequest,
) -> Result<(), CommissionError> {
    journey
        .add_participant(JourneyParticipant {
            participant_ref: request
                .central_composition
                .development_agent_set
                .record_ref
                .clone(),
            role: Some("central-composition-evidence-non-authoritative".into()),
        })
        .map_err(debug)?;
    journey
        .add_participant(JourneyParticipant {
            participant_ref: request
                .central_composition
                .guardian_agent_set
                .record_ref
                .clone(),
            role: Some("central-guardian-membership-evidence-non-authoritative".into()),
        })
        .map_err(debug)?;
    for agent in &request.central_composition.resolved_agent_refs {
        journey
            .add_participant(JourneyParticipant {
                participant_ref: agent.clone(),
                role: Some(if agent == &request.root_act.agent_ref {
                    "bounded-root-act-commissioned-not-executed".into()
                } else {
                    "resolved-membership-evidence-non-authoritative".into()
                }),
            })
            .map_err(debug)?;
    }
    Ok(())
}

fn derive_identities(
    request: &FactoryCommissionRequest,
) -> Result<(ProjectRef, JourneyRef, RunRef), CommissionError> {
    let moment = timestamp(&request.commissioned_at, "commissionedAt")?;
    let millis = timestamp_ms(moment)?;
    let project_ref = ProjectRef::try_from(
        Ref::new(
            "project",
            deterministic_ulid(0, request.project_key.as_bytes()),
        )
        .map_err(debug)?,
    )
    .map_err(debug)?;
    let journey_ref = JourneyRef::new(
        Ref::new(
            "journey",
            deterministic_ulid(
                millis,
                format!("{}\0{}\0journey", project_ref, request.request_ref).as_bytes(),
            ),
        )
        .map_err(debug)?,
    )
    .map_err(debug)?;
    let run_ref = RunRef::try_from(
        Ref::new(
            "run",
            deterministic_ulid(
                millis,
                format!("{}\0{}\0run", journey_ref, request.request_ref).as_bytes(),
            ),
        )
        .map_err(debug)?,
    )
    .map_err(debug)?;
    Ok((project_ref, journey_ref, run_ref))
}

fn commission_basis(request: &FactoryCommissionRequest) -> Vec<String> {
    let mut values = vec![
        request.request_ref.clone(),
        request.root_act.act_ref.clone(),
        request
            .central_composition
            .development_agent_set
            .record_ref
            .clone(),
        request
            .central_composition
            .guardian_agent_set
            .record_ref
            .clone(),
        request
            .central_composition
            .development_agent_set
            .source_path
            .clone(),
        request
            .central_composition
            .guardian_agent_set
            .source_path
            .clone(),
    ];
    values.sort();
    values.dedup();
    values
}

fn edge(subject: &str, relation: &str, object: &str, owner: &str) -> FactoryCommissionEdge {
    FactoryCommissionEdge {
        subject_ref: subject.into(),
        relation: relation.into(),
        object_ref: object.into(),
        owner: owner.into(),
    }
}
fn timestamp(value: &str, field: &str) -> Result<DateTime<Utc>, CommissionError> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| CommissionError::Invalid(field.into()))
}
fn timestamp_ms(value: DateTime<Utc>) -> Result<u64, CommissionError> {
    u64::try_from(value.timestamp_millis())
        .ok()
        .filter(|v| *v < (1_u64 << 48))
        .ok_or(CommissionError::TimestampOutOfRange)
}
fn deterministic_ulid(timestamp_ms: u64, basis: &[u8]) -> Ulid {
    let digest = blake3::hash(basis);
    let mut bytes = [0_u8; 16];
    bytes[6..].copy_from_slice(&digest.as_bytes()[..10]);
    Ulid::from_parts(timestamp_ms, u128::from_be_bytes(bytes))
}
fn required(value: &str, field: &str) -> Result<(), CommissionError> {
    if value.trim().is_empty() || value != value.trim() || value.chars().any(char::is_control) {
        Err(CommissionError::Invalid(field.into()))
    } else {
        Ok(())
    }
}
fn stable_ref(value: &str, field: &str) -> Result<(), CommissionError> {
    required(value, field)?;
    if value.chars().any(char::is_whitespace) {
        Err(CommissionError::Invalid(field.into()))
    } else {
        Ok(())
    }
}
fn unique_required(values: &[String], field: &str) -> Result<(), CommissionError> {
    if values.is_empty() {
        return Err(CommissionError::Invalid(field.into()));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        stable_ref(value, field)?;
        if !seen.insert(value) {
            return Err(CommissionError::Invalid(field.into()));
        }
    }
    Ok(())
}
fn lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn debug(error: impl std::fmt::Debug) -> CommissionError {
    CommissionError::Conflict(format!("{error:?}"))
}

#[derive(Debug)]
pub enum CommissionError {
    Invalid(String),
    InvalidStored,
    TimestampOutOfRange,
    ReplayConflict(String),
    ForeignProject,
    ForeignReference(String),
    Conflict(String),
    NotFound(String),
}
impl Display for CommissionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Factory Commission error: {self:?}")
    }
}
impl Error for CommissionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::execute_cli;
    use crate::developmental_read::FactoryDevelopmentalFileProvider;
    use serde_json::Value;
    use std::sync::{Arc, Barrier};

    fn request() -> FactoryCommissionRequest {
        FactoryCommissionRequest {
            contract: FACTORY_COMMISSION_REQUEST.into(),
            request_ref: "commission-request:oi-self-hosting-201".into(),
            project_key: "factory-programme-195".into(),
            purpose: "Self-host the bounded Factory programme continuation".into(),
            frontier: "Commissioned; owner-native work remains unexecuted".into(),
            run_destination: "factory-programme/remaining-oi-aikit-and-snapshot-work".into(),
            write_owner: "factory".into(),
            commissioned_at: "2026-09-09T12:00:00Z".into(),
            central_composition: CentralAgentCompositionEvidence {
                owner: "central".into(),
                development_agent_set: CentralSourceRecordReceipt {
                    record_ref: "oi-development-agency".into(),
                    revision: "r1".into(),
                    source_path: "Control/relations/agent-sets/agent-set-cbf7f912c48546eb.json"
                        .into(),
                    sha256: "1".repeat(64),
                },
                guardian_agent_set: CentralSourceRecordReceipt {
                    record_ref: "oi-product-guardians".into(),
                    revision: "r1".into(),
                    source_path: "Control/relations/agent-sets/agent-set-94404c319e7e5e8a.json"
                        .into(),
                    sha256: "2".repeat(64),
                },
                resolved_agent_refs: vec![
                    "agent/oi-guardian-mahamaya".into(),
                    "agent/oi-guardian-parasakti".into(),
                    "agent:hermes".into(),
                ],
                agent_profile_receipts: Vec::new(),
                authority_standing: "membership-non-authoritative".into(),
            },
            root_act: BoundedRootAct {
                act_ref: "act:factory-201-root".into(),
                agent_ref: "agent:hermes".into(),
                purpose: "Bound only the named remaining programme work".into(),
                scope_refs: vec!["issue:201".into(), "issue:188".into()],
                standing: "commissioned-not-executed".into(),
            },
        }
    }

    #[test]
    fn commission_roundtrip_replay_and_conflict_are_atomic() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.json");
        let applied = FactoryDevelopmentalFileProvider::commission(&path, request()).unwrap();
        assert_eq!(applied.status, FactoryAdmissionStatus::Applied);
        let replay = FactoryDevelopmentalFileProvider::commission(&path, request()).unwrap();
        assert_eq!(replay.status, FactoryAdmissionStatus::AlreadyApplied);
        let before = std::fs::read(&path).unwrap();
        let mut conflicting = request();
        conflicting.purpose.push_str(" changed");
        assert!(FactoryDevelopmentalFileProvider::commission(&path, conflicting).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();
        assert_eq!(
            provider
                .commission_reading("commission-request:oi-self-hosting-201")
                .unwrap()
                .commission,
            applied.commission
        );
    }

    #[test]
    fn concurrent_duplicate_commission_creates_one_state_without_clobber() {
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(directory.path().join("state.json"));
        let barrier = Arc::new(Barrier::new(3));
        let handles = (0..2)
            .map(|_| {
                let path = Arc::clone(&path);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    FactoryDevelopmentalFileProvider::commission(path.as_path(), request())
                        .unwrap()
                        .status
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let statuses = handles
            .into_iter()
            .map(|item| item.join().unwrap())
            .collect::<Vec<_>>();
        assert!(statuses.contains(&FactoryAdmissionStatus::Applied));
        assert!(statuses.contains(&FactoryAdmissionStatus::AlreadyApplied));
        let provider = FactoryDevelopmentalFileProvider::open(path.as_path()).unwrap();
        assert!(provider
            .commission_reading("commission-request:oi-self-hosting-201")
            .is_ok());
    }

    #[test]
    fn foreign_project_and_late_build_failure_leave_memory_unchanged() {
        let (mut state, _) = FactoryDevelopmentalState::from_commission(request()).unwrap();
        let before = serde_json::to_value(&state).unwrap();
        let mut foreign = request();
        foreign.request_ref = "commission-request:foreign".into();
        foreign.project_key = "foreign".into();
        assert!(matches!(
            state.admit_commission(foreign),
            Err(CommissionError::ForeignProject)
        ));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);

        let mut encoded = serde_json::to_value(&state).unwrap();
        encoded["build"]["revision"] = Value::from(u64::MAX);
        let mut overflow: FactoryDevelopmentalState = serde_json::from_value(encoded).unwrap();
        let before = serde_json::to_value(&overflow).unwrap();
        let mut second = request();
        second.request_ref = "commission-request:second".into();
        second.commissioned_at = "2026-09-09T12:01:00Z".into();
        assert!(overflow.admit_commission(second).is_err());
        assert_eq!(serde_json::to_value(&overflow).unwrap(), before);
    }

    #[test]
    fn owner_activity_mutation_is_bound_replay_safe_and_tamper_evident() {
        let (mut state, receipt) = FactoryDevelopmentalState::from_commission(request()).unwrap();
        let mutation = FactoryDevelopmentalMutationRequest {
            contract: FACTORY_DEVELOPMENTAL_MUTATION_REQUEST.into(),
            mutation_ref: "mutation:activity-1".into(),
            occurrence_ref: "occurrence:activity-1".into(),
            source: FactoryMutationSource {
                owner: "actuation".into(),
                reference: "activity:owner-1".into(),
                revision: "1".into(),
                standing: "owner-native-observation".into(),
            },
            observed_at: "2026-09-09T12:02:00Z".into(),
            mutation: FactoryDevelopmentalMutation::CorrelateOwnerActivity {
                journey_ref: receipt.commission.journey_ref,
                activity_ref: "activity:owner-1".into(),
            },
        };
        assert_eq!(
            state
                .apply_developmental_mutation(mutation.clone())
                .unwrap()
                .status,
            FactoryAdmissionStatus::Applied
        );
        assert_eq!(
            state
                .apply_developmental_mutation(mutation.clone())
                .unwrap()
                .status,
            FactoryAdmissionStatus::AlreadyApplied
        );
        let mut conflict = mutation;
        conflict.source.revision = "2".into();
        assert!(state.apply_developmental_mutation(conflict).is_err());
        state.journeys[0].activity_refs.clear();
        assert!(state.validate().is_err());
    }

    #[test]
    fn owner_return_and_recognition_use_existing_journey_models_and_source_basis() {
        let (mut state, receipt) = FactoryDevelopmentalState::from_commission(request()).unwrap();
        let journey_ref = receipt.commission.journey_ref;
        let run_ref = receipt.commission.run_ref;
        let returned = JourneyReturn {
            return_ref: "return:owner-1".into(),
            run_refs: vec![run_ref],
            basis_refs: Vec::new(),
            evidence_refs: vec!["activity:owner-return-1".into()],
            recognition_ref: None,
            summary: "Owner-native partial return; no completion inferred".into(),
        };
        let return_request = FactoryDevelopmentalMutationRequest {
            contract: FACTORY_DEVELOPMENTAL_MUTATION_REQUEST.into(),
            mutation_ref: "mutation:return-1".into(),
            occurrence_ref: "occurrence:return-1".into(),
            source: FactoryMutationSource {
                owner: "actuation".into(),
                reference: "activity:owner-return-1".into(),
                revision: "1".into(),
                standing: "owner-native-observation".into(),
            },
            observed_at: "2026-09-09T12:03:00Z".into(),
            mutation: FactoryDevelopmentalMutation::RecordOwnerReturn {
                journey_ref: journey_ref.clone(),
                returned: returned.clone(),
            },
        };
        state.apply_developmental_mutation(return_request).unwrap();
        let recognition = JourneyRecognitionLink {
            recognition_ref: "recognition:owner-1".into(),
            subject_ref: returned.return_ref.clone(),
            basis_refs: vec!["recognition:owner-1".into()],
        };
        let recognition_request = FactoryDevelopmentalMutationRequest {
            contract: FACTORY_DEVELOPMENTAL_MUTATION_REQUEST.into(),
            mutation_ref: "mutation:recognition-1".into(),
            occurrence_ref: "occurrence:recognition-1".into(),
            source: FactoryMutationSource {
                owner: "central".into(),
                reference: "recognition:owner-1".into(),
                revision: "1".into(),
                standing: "owner-native-observation".into(),
            },
            observed_at: "2026-09-09T12:04:00Z".into(),
            mutation: FactoryDevelopmentalMutation::RecordOwnerRecognition {
                journey_ref,
                recognition: recognition.clone(),
            },
        };
        state
            .apply_developmental_mutation(recognition_request)
            .unwrap();
        state.validate().unwrap();
        assert_eq!(state.journeys[0].returns, vec![returned]);
        assert_eq!(state.journeys[0].recognitions, vec![recognition]);
        assert_eq!(
            state.journeys[0].status,
            crate::journey::JourneyStatus::Active
        );
    }

    #[test]
    fn real_cli_commissions_and_reopens_public_read() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.json");
        let body = serde_json::to_string(&request()).unwrap();
        let commissioned = execute_cli(
            &[
                "development".into(),
                "commission".into(),
                path.display().to_string(),
                "-".into(),
                "--json".into(),
            ],
            Some(&body),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&commissioned).unwrap()["status"],
            "applied"
        );
        let reading = execute_cli(
            &[
                "development".into(),
                "commission-read".into(),
                path.display().to_string(),
                "commission-request:oi-self-hosting-201".into(),
                "--json".into(),
            ],
            None,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&reading).unwrap()["contract"],
            FACTORY_COMMISSION_READING
        );
    }

    #[test]
    fn strict_evidence_rejects_foreign_authority_bad_digest_and_pre_epoch_time() {
        let mut invalid = request();
        invalid.write_owner = "consumer".into();
        assert!(invalid.validate().is_err());
        let mut invalid = request();
        invalid.commissioned_at = "1960-01-01T00:00:00Z".into();
        assert!(matches!(
            invalid.validate(),
            Err(CommissionError::TimestampOutOfRange)
        ));
        let mut invalid = request();
        invalid.central_composition.development_agent_set.sha256 = "A".repeat(64);
        assert!(invalid.validate().is_err());
        let mut invalid = request();
        invalid.central_composition.resolved_agent_refs[0] = "agent/bad ref".into();
        assert!(invalid.validate().is_err());
        let mut invalid = request();
        invalid.root_act.scope_refs[0] = "issue:201 bad".into();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn self_hosting_workflow_fixture_compiles_with_exact_digest() {
        let source: WorkflowSource = serde_json::from_str(include_str!(
            "../../contracts/factory/fixtures/oi-self-hosting-workflow-source.json"
        ))
        .unwrap();
        assert_eq!(
            crate::workflow::workflow_source_digest(&source).unwrap(),
            source.source.digest
        );
        let compiled = compile_workflow(source).unwrap();
        assert!(compiled
            .units
            .contains_key("factory-guardian-owner-surface"));
        assert!(compiled.units.contains_key("aikit-guardian-consumer"));
        assert!(compiled.units.contains_key("oi-snapshot-188"));
    }
}
