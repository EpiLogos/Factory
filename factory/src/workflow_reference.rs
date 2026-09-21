//! Foreign subjects keep their owner's reference grammar. Factory Run and
//! WorkflowUnit identities still use the existing core identity types.
use crate::core::identity::Ref;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

/// An explicitly qualified reference, retained byte-for-byte. Qualification is
/// not resolution, existence, membership, permission or a grant of authority.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct WorkflowSubjectRef(String);

pub fn validate_qualified_reference(value: &str) -> Result<(), &'static str> {
    let boundary = value
        .find([':', '/'])
        .ok_or("reference needs its native owner qualification")?;
    if value.len() > 2048
        || boundary == 0
        || boundary + 1 == value.len()
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("reference must be bounded, nonempty and qualified; whitespace and controls are forbidden");
    }
    Ok(())
}
impl WorkflowSubjectRef {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for WorkflowSubjectRef {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_qualified_reference(value)?;
        Ok(Self(value.to_owned()))
    }
}
impl From<Ref> for WorkflowSubjectRef {
    fn from(value: Ref) -> Self {
        Self(value.to_string())
    }
}
impl fmt::Display for WorkflowSubjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Serialize for WorkflowSubjectRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for WorkflowSubjectRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
