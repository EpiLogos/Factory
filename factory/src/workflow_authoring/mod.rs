//! Restricted TypeScript data authoring. No submitted JavaScript is executed.
//! Native workflow compilation, Run topology and attempt admission remain owners.
pub mod cli;
pub mod inspect;
mod parse;

use crate::workflow::{compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

pub const AUTHORING_CONTRACT: &str = "factory.authored-workflow-basis/v1";
pub const COMPILER: &str = "factory.restricted-typescript/v1";
pub const SDK: &str = "@epilogos/factory-workflow";
pub const MAX_MODULE_BYTES: usize = 524_288;
pub const MAX_BUNDLE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MODULES: usize = 64;
pub const MAX_NODES: usize = 32_768;
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub byte_offset: usize,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleBasis {
    pub path: String,
    pub digest: String,
    pub bytes: usize,
    /// Exact source in the existing native provenance. Omitted from summaries.
    /// Never reread an edited file to render the basis of a historical attempt.
    pub content: String,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePointer {
    #[serde(rename = "ref")]
    pub reference: String,
    pub revision: String,
    pub digest: String,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoredBasis {
    pub contract: String,
    pub compiler: String,
    pub compiler_digest: String,
    pub entry: String,
    /// Source-byte revision, not the native semantic edition revision.
    pub revision: String,
    pub bundle_digest: String,
    pub modules: BTreeMap<String, ModuleBasis>,
    pub locations: BTreeMap<String, SourceLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor_of: Option<SourcePointer>,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub location: Option<Box<SourceLocation>>,
    pub field: Option<String>,
    pub unit: Option<String>,
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(loc) = &self.location {
            write!(f, "{}:{}:{}: ", loc.file, loc.line, loc.column)?;
        }
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(field) = &self.field {
            write!(f, " ({field})")?;
        }
        Ok(())
    }
}
impl std::error::Error for Diagnostic {}
pub(crate) fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: message.into(),
        location: None,
        field: None,
        unit: None,
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredWorkflow {
    pub contract: &'static str,
    pub source: WorkflowSource,
    pub compiled: CompiledWorkflow,
}

pub fn compiler_digest() -> String {
    let mut h = blake3::Hasher::new();
    for bytes in [
        COMPILER.as_bytes(),
        include_bytes!("mod.rs").as_slice(),
        include_bytes!("parse.rs").as_slice(),
        include_bytes!("../workflow.rs").as_slice(),
        include_bytes!("../workflow_reference.rs").as_slice(),
        include_bytes!("../workflow_inputs.rs").as_slice(),
        include_bytes!("../../workflow-sdk/schema.json").as_slice(),
        include_bytes!("../../workflow-sdk/index.d.ts").as_slice(),
    ] {
        frame(&mut h, bytes);
    }
    h.finalize().to_hex().to_string()
}
fn frame(h: &mut blake3::Hasher, bytes: &[u8]) {
    h.update(&(bytes.len() as u64).to_be_bytes());
    h.update(bytes);
}
fn bundle_digest(modules: &BTreeMap<String, ModuleBasis>, entry: &str, compiler: &str) -> String {
    let mut h = blake3::Hasher::new();
    for b in [
        AUTHORING_CONTRACT.as_bytes(),
        entry.as_bytes(),
        compiler.as_bytes(),
    ] {
        frame(&mut h, b);
    }
    for (path, module) in modules {
        frame(&mut h, path.as_bytes());
        frame(&mut h, module.digest.as_bytes());
    }
    h.finalize().to_hex().to_string()
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn module_path(s: &str) -> bool {
    !s.is_empty()
        && !s.contains('\\')
        && !s.contains(':')
        && !s.starts_with('/')
        && s.split('/').all(|p| !p.is_empty() && p != "." && p != "..")
        && s.ends_with(".ts")
}
impl AuthoredBasis {
    /// Historical records remain readable under newer compilers. Validate the
    /// stored compiler/source fingerprint, never replace it with today's one.
    pub fn validate_source(&self, source: &WorkflowSource) -> Result<(), Diagnostic> {
        self.validate()?;
        let mut original = parse::replay(self)?.value;
        let object = original.as_object_mut().ok_or_else(|| {
            error(
                "authoring.basis_invalid",
                "Retained source is not a workflow object",
            )
        })?;
        object
            .entry("schemaVersion")
            .or_insert_with(|| crate::workflow::WORKFLOW_SOURCE_SCHEMA.into());
        object
            .entry("coordinationContract")
            .or_insert_with(|| crate::workflow::BOUNDED_COORDINATION_CONTRACT.into());
        let provenance = object
            .get_mut("source")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| {
                error(
                    "authoring.basis_invalid",
                    "Retained source has no provenance",
                )
            })?;
        if provenance.contains_key("digest") || provenance.contains_key("authoring") {
            return Err(error(
                "authoring.reserved",
                "Retained source supplied compiler-reserved fields",
            ));
        }
        let predecessor = provenance
            .remove("successorOf")
            .map(serde_json::from_value::<SourcePointer>)
            .transpose()
            .map_err(|e| error("authoring.predecessor_invalid", e.to_string()))?;
        if predecessor != self.successor_of {
            return Err(error(
                "authoring.predecessor_invalid",
                "Predecessor differs from original authored declaration",
            ));
        }
        provenance.insert("digest".into(), source.source.digest.clone().into());
        let actual: WorkflowSource = serde_json::from_value(original)
            .map_err(|e| error("authoring.schema", e.to_string()))?;
        let mut expected = source.clone();
        expected.source.authoring = None;
        if actual != expected {
            return Err(error("authoring.semantic_mismatch","Native workflow differs from its original authored source; compile an attributable successor instead"));
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.contract != AUTHORING_CONTRACT
            || self.compiler != COMPILER
            || !hex(&self.compiler_digest)
            || self.modules.is_empty()
            || self.modules.len() > MAX_MODULES
            || !self.modules.contains_key(&self.entry)
            || self.locations.len() > MAX_NODES
        {
            return Err(error(
                "authoring.basis_invalid",
                "Unsupported or incomplete authored basis",
            ));
        }
        let mut bytes = 0;
        for (path, m) in &self.modules {
            if path != &m.path
                || !module_path(path)
                || m.bytes > MAX_MODULE_BYTES
                || m.content.len() != m.bytes
                || !hex(&m.digest)
                || blake3::hash(m.content.as_bytes()).to_hex().as_str() != m.digest
            {
                return Err(error(
                    "authoring.basis_invalid",
                    format!("Source bytes do not match retained module {path}"),
                ));
            }
            bytes += m.bytes;
        }
        if bytes > MAX_BUNDLE_BYTES
            || self.bundle_digest
                != bundle_digest(&self.modules, &self.entry, &self.compiler_digest)
            || self.revision != format!("blake3:{}", self.bundle_digest)
        {
            return Err(error(
                "authoring.basis_invalid",
                "Source bundle/revision fingerprint mismatch",
            ));
        }
        for (pointer, loc) in &self.locations {
            let module = self.modules.get(&loc.file).ok_or_else(|| {
                error(
                    "authoring.location_invalid",
                    "Location names an unretained module",
                )
            })?;
            if (!pointer.is_empty() && !pointer.starts_with('/'))
                || loc.byte_offset > module.bytes
                || !module.content.is_char_boundary(loc.byte_offset)
                || location(&loc.file, &module.content, loc.byte_offset) != *loc
            {
                return Err(error(
                    "authoring.location_invalid",
                    "Location does not match the retained source bytes",
                ));
            }
        }
        if let Some(p) = &self.successor_of {
            if p.reference.parse::<crate::core::identity::Ref>().is_err()
                || !p.reference.starts_with("workflow-source:")
                || p.revision.trim().is_empty()
                || !hex(&p.digest)
            {
                return Err(error(
                    "authoring.predecessor_invalid",
                    "Predecessor must name an exact workflow source, edition and semantic digest",
                ));
            }
        }
        Ok(())
    }
}
pub(crate) fn location(file: &str, text: &str, offset: usize) -> SourceLocation {
    let before = &text[..offset];
    SourceLocation {
        file: file.into(),
        byte_offset: offset,
        line: before.bytes().filter(|b| *b == b'\n').count() + 1,
        column: before.rsplit('\n').next().unwrap_or("").chars().count() + 1,
    }
}

/// Load only explicitly selected source and literal relative imports under root.
/// npm, tsconfig plugins, JS modules, ambient globals and user code never run.
pub fn load_workflow(root: &Path, entry: &Path) -> Result<AuthoredWorkflow, Diagnostic> {
    let (mut parsed, modules, entry_name) = parse::load(root, entry)?;
    let mut basis = AuthoredBasis {
        contract: AUTHORING_CONTRACT.into(),
        compiler: COMPILER.into(),
        compiler_digest: compiler_digest(),
        entry: entry_name,
        revision: String::new(),
        bundle_digest: String::new(),
        modules,
        locations: parsed.locations.clone(),
        successor_of: None,
    };
    let object = parsed.value.as_object_mut().ok_or_else(|| {
        error(
            "authoring.definition",
            "The default export must be a workflow data object",
        )
    })?;
    object
        .entry("schemaVersion")
        .or_insert_with(|| crate::workflow::WORKFLOW_SOURCE_SCHEMA.into());
    object
        .entry("coordinationContract")
        .or_insert_with(|| crate::workflow::BOUNDED_COORDINATION_CONTRACT.into());
    let source = object
        .get_mut("source")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| error("authoring.source", "Declare source.ref and source.revision"))?;
    if source.contains_key("digest") || source.contains_key("authoring") {
        return Err(error(
            "authoring.reserved",
            "Factory computes source.digest and source.authoring; remove these fields",
        ));
    }
    basis.successor_of = source
        .remove("successorOf")
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| error("authoring.predecessor_invalid", e.to_string()))?;
    basis.bundle_digest = bundle_digest(&basis.modules, &basis.entry, &basis.compiler_digest);
    basis.revision = format!("blake3:{}", basis.bundle_digest);
    basis.validate()?;
    source.insert("digest".into(), "0".repeat(64).into());
    source.insert(
        "authoring".into(),
        serde_json::to_value(basis).map_err(|e| error("authoring.serialization", e.to_string()))?,
    );
    // Deserialize units individually so a field error in an imported module
    // names that module/unit, not just the first line of the entry file.
    if let Some(units) = parsed
        .value
        .get("units")
        .and_then(serde_json::Value::as_array)
    {
        for (index, value) in units.iter().enumerate() {
            let pointer = format!("/units/{index}");
            let unit = serde_json::from_value::<crate::workflow::WorkflowUnitSource>(value.clone())
                .map_err(|e| {
                    schema_diagnostic(
                        e,
                        &pointer,
                        value.get("key").and_then(serde_json::Value::as_str),
                        &parsed.locations,
                    )
                })?;
            crate::workflow::validate_workflow_unit(&unit).map_err(|e| {
                let mut diagnostic = error("authoring.native_validation", e.to_string());
                diagnostic.unit = Some(unit.key.clone());
                let field = error_field(&e).unwrap_or("key");
                diagnostic.field = Some(field.into());
                diagnostic.location = location_for_field(&parsed.locations, &pointer, field);
                diagnostic
            })?;
        }
    }
    let mut source: WorkflowSource = serde_json::from_value(parsed.value)
        .map_err(|e| schema_diagnostic(e, "", None, &parsed.locations))?;
    source.source.digest =
        workflow_source_digest(&source).map_err(|e| native_diagnostic(&e, &source))?;
    let compiled = compile_workflow(source.clone()).map_err(|e| native_diagnostic(&e, &source))?;
    Ok(AuthoredWorkflow {
        contract: "factory.authored-workflow/v1",
        source,
        compiled,
    })
}

fn error_field(e: &crate::workflow::WorkflowError) -> Option<&str> {
    use crate::workflow::WorkflowError::*;
    match e {
        EmptyField(field) => Some(field.strip_prefix("units.").unwrap_or(field)),
        EmptyCollection(field)
        | InvalidLocator { field, .. }
        | InvalidReference { field, .. }
        | DuplicateValue { field, .. } => Some(field.strip_prefix("units.").unwrap_or(field)),
        _ => None,
    }
}
fn location_for_field(
    locations: &BTreeMap<String, SourceLocation>,
    parent: &str,
    field: &str,
) -> Option<Box<SourceLocation>> {
    let mut pointer = format!("{parent}/{}", field.replace('.', "/"));
    loop {
        if let Some(location) = locations.get(&pointer) {
            return Some(Box::new(location.clone()));
        }
        match pointer.rfind('/') {
            Some(index) => pointer.truncate(index),
            None => return locations.get("").cloned().map(Box::new),
        }
    }
}
fn schema_diagnostic(
    e: serde_json::Error,
    parent: &str,
    unit: Option<&str>,
    locations: &BTreeMap<String, SourceLocation>,
) -> Diagnostic {
    let text = e.to_string();
    let field = ["missing field `", "unknown field `", "duplicate field `"]
        .iter()
        .find_map(|prefix| {
            text.split_once(prefix)
                .and_then(|(_, s)| s.split_once('`').map(|(field, _)| field))
        });
    let mut d = error(
        "authoring.schema",
        format!("{e}; use the Factory WorkflowDefinition contract"),
    );
    d.unit = unit.map(str::to_owned);
    d.field = field.map(str::to_owned);
    d.location = field
        .and_then(|f| location_for_field(locations, parent, f))
        .or_else(|| locations.get(parent).cloned().map(Box::new));
    d
}
fn native_diagnostic(e: &crate::workflow::WorkflowError, source: &WorkflowSource) -> Diagnostic {
    use crate::workflow::WorkflowError::*;
    let mut d = error("authoring.native_validation", e.to_string());
    let mut pointer = String::new();
    match e {
        DanglingUnit { field, key } => {
            d.field = Some((*field).into());
            if let Some((i, u)) =
                source.units.iter().enumerate().find(|(_, u)| {
                    u.dependencies.contains(key) || u.independence_from.contains(key)
                })
            {
                d.unit = Some(u.key.clone());
                let refs = if *field == "dependencies" {
                    &u.dependencies
                } else {
                    &u.independence_from
                };
                pointer = format!(
                    "/units/{i}/{field}/{}",
                    refs.iter().position(|r| r == key).unwrap_or(0)
                );
            } else if let Some((i, b)) = source
                .barriers
                .iter()
                .enumerate()
                .find(|(_, b)| b.waits_for.contains(key) || b.releases.contains(key))
            {
                let (name, refs) = if b.waits_for.contains(key) {
                    ("waitsFor", &b.waits_for)
                } else {
                    ("releases", &b.releases)
                };
                pointer = format!(
                    "/barriers/{i}/{name}/{}",
                    refs.iter().position(|r| r == key).unwrap_or(0)
                );
            }
        }
        SelfDependency(key) | DuplicateUnit(key) => {
            d.unit = Some(key.clone());
            d.field = Some("key".into());
        }
        ContradictoryRelations { unit, .. } => {
            d.unit = Some(unit.clone());
            d.field = Some("independenceFrom".into());
        }
        DependencyCycle(keys) => {
            d.unit = keys.first().cloned();
            d.field = Some("dependencies".into());
        }
        NestingCycle(_) => {
            d.field = Some("nesting".into());
            pointer = "/nesting".into();
        }
        _ => {
            d.field = error_field(e).map(str::to_owned);
        }
    }
    if let Some(basis) = &source.source.authoring {
        if pointer.is_empty() {
            let parent = source
                .units
                .iter()
                .position(|u| Some(&u.key) == d.unit.as_ref())
                .map(|i| format!("/units/{i}"))
                .unwrap_or_default();
            d.location =
                location_for_field(&basis.locations, &parent, d.field.as_deref().unwrap_or(""));
        } else {
            d.location = basis.locations.get(&pointer).cloned().map(Box::new);
        }
        if d.location.is_none() {
            d.location = basis.locations.get("").cloned().map(Box::new);
        }
    }
    d
}
