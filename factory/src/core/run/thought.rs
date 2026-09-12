use super::RunRef;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct RunThoughtId(String);

impl RunThoughtId {
    pub fn new(value: impl Into<String>) -> Result<Self, ThoughtFieldError> {
        let value = value.into();
        let mut characters = value.chars();
        let valid = characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
            && characters.all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            });
        if !valid {
            return Err(ThoughtFieldError::InvalidThoughtId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RunThoughtId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for RunThoughtId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RunThoughtId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunThoughtLifecycle {
    Active,
    Resolved,
    Superseded,
    Integrated,
    Retained,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageAnchor {
    pub start_byte: u64,
    pub end_byte: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl PassageAnchor {
    fn validate(&self) -> Result<(), ThoughtFieldError> {
        if self.start_byte >= self.end_byte {
            return Err(ThoughtFieldError::InvalidPassageRange {
                start_byte: self.start_byte,
                end_byte: self.end_byte,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtProducer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agency_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_ref: Option<String>,
}

impl ThoughtProducer {
    pub fn contains_ref(&self, candidate: &str) -> bool {
        self.agent_ref.as_deref() == Some(candidate)
            || self.agency_ref.as_deref() == Some(candidate)
            || self.agent_session_ref.as_deref() == Some(candidate)
            || self.execution_ref.as_deref() == Some(candidate)
    }

    fn validate(&self) -> Result<(), ThoughtFieldError> {
        for value in [
            self.agent_ref.as_deref(),
            self.agency_ref.as_deref(),
            self.agent_session_ref.as_deref(),
            self.execution_ref.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if value.trim().is_empty() {
                return Err(ThoughtFieldError::EmptyProducerRef);
            }
        }
        Ok(())
    }
}

/// One retained cognitive determination inside a Run.
///
/// `anchor_ref` addresses the retained source/artifact/note carrying the cognition.
/// Factory does not require copied prose and does not parse Markdown/OKF here.
/// `relation_evidence_refs` are opaque refs to authored/derived relation evidence
/// owned by the relevant knowledge/formal provider; their vocabulary stays open.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunThought {
    pub id: RunThoughtId,
    pub run_ref: RunRef,
    pub anchor_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passage: Option<PassageAnchor>,
    #[serde(default)]
    pub producer: ThoughtProducer,
    #[serde(default)]
    pub run_map_subject_refs: Vec<String>,
    #[serde(default)]
    pub related_refs: Vec<String>,
    #[serde(default)]
    pub relation_evidence_refs: Vec<String>,
    pub lifecycle: RunThoughtLifecycle,
}

impl RunThought {
    fn validate(&self, owning_run: &RunRef) -> Result<(), ThoughtFieldError> {
        if &self.run_ref != owning_run {
            return Err(ThoughtFieldError::WrongRun {
                expected: owning_run.clone(),
                actual: self.run_ref.clone(),
            });
        }
        if self.anchor_ref.trim().is_empty() {
            return Err(ThoughtFieldError::EmptyAnchorRef);
        }
        if self
            .anchor_revision
            .as_deref()
            .is_some_and(|revision| revision.trim().is_empty())
        {
            return Err(ThoughtFieldError::EmptyAnchorRevision);
        }
        if let Some(passage) = &self.passage {
            passage.validate()?;
        }
        self.producer.validate()?;
        ensure_non_empty_refs(
            &self.run_map_subject_refs,
            ThoughtFieldError::EmptyRunMapSubjectRef,
        )?;
        ensure_non_empty_refs(&self.related_refs, ThoughtFieldError::EmptyRelatedRef)?;
        ensure_non_empty_refs(
            &self.relation_evidence_refs,
            ThoughtFieldError::EmptyRelationEvidenceRef,
        )?;
        Ok(())
    }
}

/// Exactly one cognitive field owned by a Run.
///
/// The field has no independent global Ref. Its identity is the owning RunRef;
/// each retained Thought is addressed by a Run-local `RunThoughtId`.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunThoughtField {
    #[serde(default)]
    pub(super) thoughts: BTreeMap<RunThoughtId, RunThought>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) consumptions: BTreeMap<RunThoughtId, super::ThoughtConsumptionReceipt>,
}

impl RunThoughtField {
    pub fn thoughts(&self) -> &BTreeMap<RunThoughtId, RunThought> {
        &self.thoughts
    }

    pub fn get(&self, thought_id: &RunThoughtId) -> Option<&RunThought> {
        self.thoughts.get(thought_id)
    }

    pub(crate) fn retain(
        &mut self,
        owning_run: &RunRef,
        thought: RunThought,
    ) -> Result<(), ThoughtFieldError> {
        thought.validate(owning_run)?;
        if self.thoughts.contains_key(&thought.id) {
            return Err(ThoughtFieldError::DuplicateThought(thought.id));
        }
        self.thoughts.insert(thought.id.clone(), thought);
        Ok(())
    }

    pub fn for_run_map_subject(&self, subject_ref: &str) -> Vec<&RunThought> {
        self.thoughts
            .values()
            .filter(|thought| {
                thought
                    .run_map_subject_refs
                    .iter()
                    .any(|candidate| candidate == subject_ref)
            })
            .collect()
    }

    pub fn by_producer_ref(&self, producer_ref: &str) -> Vec<&RunThought> {
        self.thoughts
            .values()
            .filter(|thought| thought.producer.contains_ref(producer_ref))
            .collect()
    }

    pub fn related_to(&self, related_ref: &str) -> Vec<&RunThought> {
        self.thoughts
            .values()
            .filter(|thought| {
                thought
                    .related_refs
                    .iter()
                    .any(|candidate| candidate == related_ref)
            })
            .collect()
    }

    pub(crate) fn validate(&self, owning_run: &RunRef) -> Result<(), ThoughtFieldError> {
        for (key, thought) in &self.thoughts {
            if key != &thought.id {
                return Err(ThoughtFieldError::CorruptThoughtKey {
                    key: key.clone(),
                    thought_id: thought.id.clone(),
                });
            }
            thought.validate(owning_run)?;
        }
        self.validate_consumptions(owning_run)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThoughtFieldError {
    InvalidThoughtId(String),
    InvalidConsumption(String),
    WrongRun {
        expected: RunRef,
        actual: RunRef,
    },
    DuplicateThought(RunThoughtId),
    EmptyAnchorRef,
    EmptyAnchorRevision,
    InvalidPassageRange {
        start_byte: u64,
        end_byte: u64,
    },
    EmptyProducerRef,
    EmptyRunMapSubjectRef,
    EmptyRelatedRef,
    EmptyRelationEvidenceRef,
    CorruptThoughtKey {
        key: RunThoughtId,
        thought_id: RunThoughtId,
    },
}

impl Display for ThoughtFieldError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Run Thought field error: {self:?}")
    }
}

impl Error for ThoughtFieldError {}

fn ensure_non_empty_refs(
    refs: &[String],
    error: ThoughtFieldError,
) -> Result<(), ThoughtFieldError> {
    if refs.iter().any(|reference| reference.trim().is_empty()) {
        Err(error)
    } else {
        Ok(())
    }
}
