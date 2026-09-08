use crate::core::identity::{Ref, RefParseError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TypedRefError {
    Parse(RefParseError),
    WrongKind {
        expected: &'static str,
        actual: String,
    },
    InvalidMapAddress,
}

impl Display for TypedRefError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "{error}"),
            Self::WrongKind { expected, actual } => {
                write!(formatter, "expected {expected} Ref, found {actual} Ref")
            }
            Self::InvalidMapAddress => write!(formatter, "Run Map address must be <run-ref>/map"),
        }
    }
}

impl Error for TypedRefError {}

impl From<RefParseError> for TypedRefError {
    fn from(error: RefParseError) -> Self {
        Self::Parse(error)
    }
}

macro_rules! typed_ref {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(Ref);

        impl AsRef<Ref> for $name {
            fn as_ref(&self) -> &Ref {
                &self.0
            }
        }

        impl TryFrom<Ref> for $name {
            type Error = TypedRefError;

            fn try_from(reference: Ref) -> Result<Self, Self::Error> {
                if reference.kind() != $kind {
                    return Err(TypedRefError::WrongKind {
                        expected: $kind,
                        actual: reference.kind().to_owned(),
                    });
                }
                Ok(Self(reference))
            }
        }

        impl From<$name> for Ref {
            fn from(reference: $name) -> Self {
                reference.0
            }
        }

        impl FromStr for $name {
            type Err = TypedRefError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from(value.parse::<Ref>()?)
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self.0, formatter)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let reference = Ref::deserialize(deserializer)?;
                Self::try_from(reference).map_err(serde::de::Error::custom)
            }
        }
    };
}

typed_ref!(ProjectRef, "project");
typed_ref!(RunRef, "run");
typed_ref!(WorkflowUnitRef, "workflow-unit");

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct RunMapAddress(RunRef);

impl RunMapAddress {
    pub fn for_run(run_ref: RunRef) -> Self {
        Self(run_ref)
    }

    pub fn run_ref(&self) -> &RunRef {
        &self.0
    }
}

impl Display for RunMapAddress {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/map", self.0)
    }
}

impl FromStr for RunMapAddress {
    type Err = TypedRefError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let run = value
            .strip_suffix("/map")
            .ok_or(TypedRefError::InvalidMapAddress)?;
        if run.contains('/') {
            return Err(TypedRefError::InvalidMapAddress);
        }
        Ok(Self(run.parse()?))
    }
}

impl Serialize for RunMapAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RunMapAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}
