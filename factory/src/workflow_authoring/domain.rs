//! Declared registry of domain type modules a workflow source may import.
//!
//! The registry is data (`contracts/factory/workflow-domain-adapters.json`).
//! An adapter publishes types only; its declarations are vendored from the
//! owner and pinned by content. Factory lowers the adapter's declared unit field
//! through a native lowering it owns. A domain module that is not registered
//! stays refused, and no domain code is ever loaded or executed.
use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::OnceLock;

pub const REGISTRY_CONTRACT: &str = "factory.workflow-domain-adapters/v1";
pub const REGISTRY: &str = include_str!("../../../contracts/factory/workflow-domain-adapters.json");
/// Declaration bytes embedded in the binary, keyed by the registry path.
/// A registry entry naming a path absent here is an invalid registry.
pub const DECLARATIONS: &[(&str, &str)] = &[(
    "contracts/factory/upstream/ql-vak-workflow-types.d.ts",
    include_str!("../../../contracts/factory/upstream/ql-vak-workflow-types.d.ts"),
)];
/// Native unit fields a domain adapter may never claim.
const NATIVE_UNIT_FIELDS: &[&str] = &[
    "key",
    "developmentalConcern",
    "requiredDifference",
    "returnContract",
    "subjectRef",
    "basisRevision",
    "agentRequirements",
    "praxisRefs",
    "capabilityRefs",
    "dependencies",
    "independenceFrom",
    "permittedEffects",
    "verificationObligations",
    "returnAddress",
    "stopConditions",
    "escalationConditions",
    "contribution",
    "inputs",
];

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpstreamDeclaration {
    pub repository: String,
    pub path: String,
    pub git_blob: String,
    pub generator: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainAdapter {
    pub specifier: String,
    pub owner: String,
    pub types_contract: String,
    pub declarations: String,
    pub declarations_sha256: String,
    pub upstream: UpstreamDeclaration,
    pub unit_field: String,
    pub lowering: String,
    pub standing: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registry {
    contract: String,
    #[allow(dead_code)]
    description: String,
    adapters: Vec<DomainAdapter>,
}

/// What an authored basis retains about each adapter it imported.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterBasis {
    pub owner: String,
    pub types_contract: String,
    pub declarations_sha256: String,
    pub unit_field: String,
    pub lowering: String,
}

fn registry_error(message: impl Into<String>) -> Diagnostic {
    error("authoring.domain_registry", message)
}

fn load_registry() -> Result<Vec<DomainAdapter>, Diagnostic> {
    let registry: Registry =
        serde_json::from_str(REGISTRY).map_err(|e| registry_error(e.to_string()))?;
    if registry.contract != REGISTRY_CONTRACT {
        return Err(registry_error(
            "Unsupported domain adapter registry contract",
        ));
    }
    let mut specifiers = BTreeSet::new();
    let mut fields = BTreeSet::new();
    for adapter in &registry.adapters {
        if adapter.specifier == SDK
            || adapter.specifier.starts_with('.')
            || adapter.specifier.contains(':')
            || !specifiers.insert(adapter.specifier.as_str())
        {
            return Err(registry_error(format!(
                "Invalid or duplicate adapter specifier {}",
                adapter.specifier
            )));
        }
        if NATIVE_UNIT_FIELDS.contains(&adapter.unit_field.as_str())
            || !fields.insert(adapter.unit_field.as_str())
        {
            return Err(registry_error(format!(
                "Adapter {} claims a native or already claimed unit field",
                adapter.specifier
            )));
        }
        let declarations = DECLARATIONS
            .iter()
            .find(|(path, _)| *path == adapter.declarations)
            .ok_or_else(|| registry_error("Adapter declarations are not embedded"))?;
        if hex::sha256(declarations.1) != adapter.declarations_sha256 {
            return Err(registry_error(format!(
                "Vendored declarations for {} differ from their registered digest",
                adapter.specifier
            )));
        }
        if lowering(&adapter.lowering).is_none() {
            return Err(registry_error(format!(
                "Adapter {} names an unknown native lowering",
                adapter.specifier
            )));
        }
    }
    Ok(registry.adapters)
}

mod hex {
    use super::{Digest, Sha256};
    pub fn sha256(text: &str) -> String {
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// The registered adapters. The registry is compiled into the binary; an
/// invalid registry refuses every domain import rather than guessing.
pub fn adapters() -> Result<&'static [DomainAdapter], Diagnostic> {
    static REGISTERED: OnceLock<Result<Vec<DomainAdapter>, Diagnostic>> = OnceLock::new();
    REGISTERED
        .get_or_init(load_registry)
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}

pub fn adapter(specifier: &str) -> Result<Option<&'static DomainAdapter>, Diagnostic> {
    Ok(adapters()?.iter().find(|a| a.specifier == specifier))
}

impl DomainAdapter {
    pub fn declarations_text(&self) -> &'static str {
        DECLARATIONS
            .iter()
            .find(|(path, _)| *path == self.declarations)
            .map(|(_, text)| *text)
            .unwrap_or_default()
    }
    pub fn exports_type(&self, name: &str) -> bool {
        self.declarations_text()
            .lines()
            .any(|line| line.starts_with(&format!("export type {name} =")))
    }
    pub fn basis(&self) -> AdapterBasis {
        AdapterBasis {
            owner: self.owner.clone(),
            types_contract: self.types_contract.clone(),
            declarations_sha256: self.declarations_sha256.clone(),
            unit_field: self.unit_field.clone(),
            lowering: self.lowering.clone(),
        }
    }
}

type Lowering = fn(serde_json::Value, &str) -> Result<serde_json::Value, String>;

fn lowering(name: &str) -> Option<Lowering> {
    match name {
        crate::vak_orchestration::C_PRIME_LOWERING => {
            Some(crate::vak_orchestration::lower_authored_composition)
        }
        _ => None,
    }
}

/// Lower every adapter-declared unit field into its native form. A unit that
/// carries a declared field without its adapter imported is refused: a selected
/// interpretation is never silently dropped or guessed.
pub(super) fn lower_units(
    value: &mut serde_json::Value,
    imported: &BTreeSet<String>,
    locations: &BTreeMap<String, SourceLocation>,
) -> Result<(), Diagnostic> {
    let registered = adapters()?;
    let Some(units) = value
        .get_mut("units")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for (index, unit) in units.iter_mut().enumerate() {
        let Some(object) = unit.as_object_mut() else {
            continue;
        };
        let key = object
            .get("key")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let subject = object
            .get("subjectRef")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        for adapter in registered {
            let Some(authored) = object.remove(&adapter.unit_field) else {
                continue;
            };
            let pointer = format!("/units/{index}/{}", adapter.unit_field);
            let fail = |code: &str, message: String| {
                let mut d = error(code, message);
                d.unit = key.clone();
                d.field = Some(adapter.unit_field.clone());
                d.location = locations
                    .get(&pointer)
                    .or_else(|| locations.get(&format!("/units/{index}")))
                    .cloned()
                    .map(Box::new);
                d
            };
            if !imported.contains(&adapter.specifier) {
                return Err(fail(
                    "authoring.domain_adapter_required",
                    format!(
                        "`{}` is interpreted by the {} adapter; import its types from \"{}\" so the interpretation is declared",
                        adapter.unit_field, adapter.owner, adapter.specifier
                    ),
                ));
            }
            let subject = subject.clone().ok_or_else(|| {
                fail(
                    "authoring.domain_lowering",
                    "A unit composition needs the unit's subjectRef".into(),
                )
            })?;
            let lower = lowering(&adapter.lowering).ok_or_else(|| {
                fail(
                    "authoring.domain_lowering",
                    "Adapter lowering is not implemented by this Factory".into(),
                )
            })?;
            let native = lower(authored, &subject)
                .map_err(|message| fail("authoring.domain_lowering", message))?;
            object.insert(adapter.unit_field.clone(), native);
        }
    }
    Ok(())
}
