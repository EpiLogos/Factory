//! A song's score and the writes that reshape it (O-I #843; DLW manual §19.1 Notes and Tools).
//!
//! The score is one read of the song a Run performs: its units with their compositions, the agents and agent sets the units
//! name (the rows a face draws them on), its joins, its waves and each unit's leg. It is read from the song's own file under
//! its project root when that file is still the same song, so an edit shows at once; the Run keeps performing the edition it
//! was commissioned with, and the score says whether that is still the file's.
//!
//! The edit writes that file in place and nothing else: a unit given to another agent or agent set (Allocate), a unit made to
//! wait on another (Connect), a unit's heads (Thread form, Accept). Every edit is compared against the file's byte revision,
//! spliced into the authored text at the fields the compiler located (comments and the rest of the file stand), compiled
//! before the file changes, and refused in Factory's words when Factory cannot write it: a stale revision, a field the song
//! shares through a const or keeps in another module, a field holding comments, an edit the compiler refuses.
use crate::attempt_task::coherent_read;
use crate::core::run::RunRef;
use crate::workflow::{compile_workflow, WorkflowSource, WorkflowUnitSource};
use crate::workflow_authoring::{
    error, load_workflow, AuthoredWorkflow, Diagnostic, SourceLocation,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SCORE_CONTRACT: &str = "factory.workflow-score/v1";
pub const EDIT_CONTRACT: &str = "factory.workflow-edit/v1";
pub const EDIT_RECEIPT_CONTRACT: &str = "factory.workflow-edit-receipt/v1";
const PERFORMS: &str =
    "The Run performs the edition it was commissioned with; the song's next Run performs this one.";
const HEADS: [&str; 7] = ["CPF", "CT", "CP", "CF", "CFP", "CS", "direction"];

fn failure(message: impl Into<String>) -> Diagnostic {
    error("workflow.score", message)
}

/// The song's file, when it is the song the Run performs and it compiles.
struct Song {
    root: PathBuf,
    file: PathBuf,
    entry: String,
    loaded: AuthoredWorkflow,
}

/// Why the score reads the retained edition instead of a file, and the compiler's word when the file does not compile.
struct Unplaced {
    why: String,
    diagnostic: Option<Diagnostic>,
}

fn unplaced(why: impl Into<String>) -> Unplaced {
    Unplaced {
        why: why.into(),
        diagnostic: None,
    }
}

fn project_root(state_path: &Path) -> Result<PathBuf, Unplaced> {
    let state = state_path
        .canonicalize()
        .map_err(|e| unplaced(format!("The developmental state cannot be read ({e}).")))?;
    let root = state
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == ".factory"))
        .and_then(Path::parent)
        .ok_or_else(|| unplaced("The developmental state is not a placed project's (.factory/development-state.json), so the song has no project root."))?;
    let located = crate::project_setup::locate(root)
        .map_err(|e| unplaced(format!("The project's placement cannot be read ({e}).")))?;
    if located["statePath"].as_str() != state.to_str() {
        return Err(unplaced(
            "The project's placement names another developmental state.",
        ));
    }
    Ok(root.to_path_buf())
}

fn song(state_path: &Path, retained: &WorkflowSource) -> Result<Song, Unplaced> {
    let basis = retained.source.authoring.as_ref().ok_or_else(|| {
        unplaced("The Run's song carries no authored basis: it was not commissioned from a source file, so no file holds it.")
    })?;
    let root = project_root(state_path)?;
    let file = root.join(&basis.entry);
    if !fs::symlink_metadata(&file).is_ok_and(|meta| meta.is_file()) {
        return Err(unplaced(format!(
            "The song's file does not stand under its project root ({}): the score reads the edition the Run performs.",
            file.display()
        )));
    }
    let loaded = load_workflow(&root, Path::new(&basis.entry)).map_err(|d| Unplaced {
        why: format!(
            "The song's file does not compile ({d}); the score reads the edition the Run performs."
        ),
        diagnostic: Some(d),
    })?;
    if loaded.source.source.reference != retained.source.reference {
        return Err(unplaced(format!(
            "The file at {} is another song ({}), not the one this Run performs ({}).",
            basis.entry, loaded.source.source.reference, retained.source.reference
        )));
    }
    Ok(Song {
        root,
        file,
        entry: basis.entry.clone(),
        loaded,
    })
}

fn retained_source(
    state_path: &Path,
    run: &RunRef,
) -> Result<
    (
        crate::developmental_read::FactoryDevelopmentalState,
        crate::attempt_runtime::FactoryAttemptReading,
        WorkflowSource,
    ),
    Diagnostic,
> {
    let (state, reading) = coherent_read(state_path, run).map_err(failure)?;
    let source = state
        .attempt_states
        .get(run)
        .ok_or_else(|| failure("Run has no native workflow attachment"))?
        .workflow_source()
        .clone();
    Ok((state, reading, source))
}

/// The row a unit is drawn on: who does it. Its first agent, else its first agent set, else its first agency.
fn row_of(unit: &WorkflowUnitSource) -> Option<(&str, &'static str)> {
    let requirements = &unit.agent_requirements;
    requirements
        .agent_refs
        .first()
        .map(|r| (r.as_str(), "agent"))
        .or_else(|| {
            requirements
                .agent_set_refs
                .first()
                .map(|r| (r.as_str(), "agent-set"))
        })
        .or_else(|| {
            requirements
                .agency_refs
                .first()
                .map(|r| (r.as_str(), "agency"))
        })
}

fn heads(unit: &WorkflowUnitSource) -> Value {
    unit.composition.as_ref().map_or(Value::Null, |c| {
        json!({"CPF": c.participation, "CT": c.content, "CP": c.position, "CF": c.frame, "CFP": c.thread,
            "CS": c.sequence, "direction": c.direction})
    })
}

/// `factory workflow score <state> <run-ref>`.
pub fn score(state_path: &Path, run: &RunRef) -> Result<Value, Diagnostic> {
    let (state, reading, retained) = retained_source(state_path, run)?;
    let performed = compile_workflow(retained.clone()).map_err(|e| failure(e.to_string()))?;
    let retained_refs = performed
        .units
        .values()
        .map(|u| (u.key.clone(), u.reference.clone()))
        .collect::<BTreeMap<_, _>>();
    let retained_keys = performed
        .units
        .values()
        .map(|u| (u.reference.clone(), u.key.clone()))
        .collect::<BTreeMap<_, _>>();
    let placed = song(state_path, &retained);
    let (source, file, why, check) = match &placed {
        Ok(song) => {
            let basis = song
                .loaded
                .source
                .source
                .authoring
                .as_ref()
                .expect("a loaded song is authored");
            (
                &song.loaded.source,
                json!({"path": song.file, "entry": song.entry, "byteRevision": basis.revision,
                    "semanticDigest": song.loaded.source.source.digest}),
                Value::Null,
                json!({"state": "admitted", "edition": "file", "by": "factory workflow check"}),
            )
        }
        Err(unplaced) => (
            &retained,
            Value::Null,
            json!(unplaced.why),
            match &unplaced.diagnostic {
                Some(d) => {
                    json!({"state": "refused", "edition": "file", "by": "factory workflow check", "diagnostic": d})
                }
                None => {
                    json!({"state": "admitted", "edition": "retained", "by": "factory workflow commission"})
                }
            },
        ),
    };
    let edition = if placed.is_ok() { "file" } else { "retained" };
    let retained_units = retained
        .units
        .iter()
        .map(|u| (u.key.as_str(), u))
        .collect::<BTreeMap<_, _>>();
    let waves = crate::wave_sdlc::read(&state, run);
    let unit_waves = waves
        .as_ref()
        .map(|w| {
            w.units
                .iter()
                .filter_map(|u| {
                    Some((
                        retained_keys.get(&u.workflow_unit_ref)?.clone(),
                        u.wave.clone(),
                    ))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut rows = Vec::<Value>::new();
    let mut seen = BTreeSet::new();
    for unit in &source.units {
        let r = &unit.agent_requirements;
        for (refs, kind) in [
            (&r.agent_refs, "agent"),
            (&r.agent_set_refs, "agent-set"),
            (&r.agency_refs, "agency"),
        ] {
            for reference in refs {
                if seen.insert(reference.clone()) {
                    rows.push(json!({"ref": reference, "kind": kind}));
                }
            }
        }
    }
    let locations = source.source.authoring.as_ref().map(|a| &a.locations);
    let units = source
        .units
        .iter()
        .enumerate()
        .map(|(index, unit)| {
            let retained_ref = retained_refs.get(&unit.key);
            let leg = retained_ref.and_then(|r| reading.legs.get(r));
            let standing = match (retained_ref, leg) {
                (None, _) => "not-in-run",
                (Some(_), None) => "not-started",
                (Some(_), Some(_)) => "native-leg",
            };
            let row = row_of(unit);
            json!({
                "key": unit.key,
                "unitRef": retained_ref,
                "row": row.map(|(r, _)| r),
                "rowKind": row.map(|(_, k)| k),
                "agentRequirements": unit.agent_requirements,
                "dependencies": unit.dependencies,
                "independenceFrom": unit.independence_from,
                "inputs": unit.inputs.iter().map(|i| &i.predecessor).collect::<Vec<_>>(),
                "heads": heads(unit),
                "composition": unit.composition.as_ref().map(|c| json!({"actor": c.actor_ref,
                    "authority": c.undertaking_authority_ref, "whole": c.whole_ref,
                    "interpretation": {"ref": c.ql_binding_ref, "revision": c.ql_binding_revision}})),
                "concern": unit.developmental_concern,
                "requiredDifference": unit.required_difference,
                "returns": unit.return_contract,
                "sources": unit.composition.as_ref().map(|c| &c.source_refs),
                "success": unit.verification_obligations,
                "may": unit.permitted_effects,
                "stop": unit.stop_conditions,
                "escalate": unit.escalation_conditions,
                "parent": source.nesting.iter().find(|n| n.child == unit.key).map(|n| &n.parent),
                "children": source.nesting.iter().filter(|n| n.parent == unit.key).map(|n| &n.child).collect::<Vec<_>>(),
                "wave": unit_waves.get(&unit.key),
                "leg": {"status": leg.map(|l| l.status), "standing": standing},
                "edited": edition == "file" && retained_units.get(unit.key.as_str()).is_none_or(|r| *r != unit),
                "sourceLocation": locations.and_then(|l| l.get(&format!("/units/{index}/key"))),
            })
        })
        .collect::<Vec<_>>();
    let joins = source
        .barriers
        .iter()
        .map(|b| {
            let owner = reading.barriers.iter().find(|r| r.key == b.key);
            json!({"key": b.key, "waitsFor": b.waits_for, "releases": b.releases,
                "join": owner.map_or(json!("all"), |r| json!(r.join)),
                "complete": owner.is_some_and(|r| r.state == crate::orchestration::BarrierState::Complete)})
        })
        .collect::<Vec<_>>();
    let (waves, waves_why) = match &waves {
        Ok(w) => (
            w.waves
                .iter()
                .map(|s| {
                    json!({"wave": s.wave, "ordinal": s.ordinal, "state": s.state, "gate": s.gate, "next": s.next,
                        "units": s.units.iter().filter_map(|r| retained_keys.get(r)).collect::<Vec<_>>()})
                })
                .collect::<Vec<_>>(),
            Value::Null,
        ),
        Err(refusal) => (vec![], json!(refusal.to_string())),
    };
    let retained_basis = retained.source.authoring.as_ref();
    Ok(json!({
        "contract": SCORE_CONTRACT,
        "runRef": run,
        "song": {
            "workflowKey": source.workflow_key,
            "sourceRef": source.source.reference,
            "sourceRevision": source.source.revision,
            "edition": edition,
            "file": file,
            "why": why,
            "performed": match &placed {
                Ok(song) => song.loaded.source.source.digest == retained.source.digest,
                Err(_) => true,
            },
            "run": {"sourceRevision": retained.source.revision, "semanticDigest": retained.source.digest,
                "byteRevision": retained_basis.map(|b| &b.revision)},
        },
        "rows": rows,
        "units": units,
        "joins": joins,
        "waves": waves,
        "wavesWhy": waves_why,
        "check": check,
        "execution": "not-requested",
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditRequest {
    contract: String,
    expected_revision: String,
    edits: Vec<Edit>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum Edit {
    Allocate {
        units: Vec<String>,
        to: Allocation,
    },
    Connect {
        unit: String,
        #[serde(rename = "waitsOn")]
        waits_on: String,
    },
    Heads {
        unit: String,
        heads: BTreeMap<String, String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Allocation {
    kind: String,
    #[serde(rename = "ref")]
    reference: String,
}

/// One replacement of the entry module's bytes; an insertion has `start == end`.
#[derive(Debug)]
struct Splice {
    start: usize,
    end: usize,
    text: String,
    pointer: String,
}

struct Text<'a> {
    entry: &'a str,
    text: &'a str,
    locations: &'a BTreeMap<String, SourceLocation>,
    shared: BTreeMap<(&'a str, usize), Vec<&'a str>>,
}

fn related(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

fn refusal(
    code: &str,
    unit: &str,
    field: &str,
    message: String,
    at: Option<&SourceLocation>,
) -> Diagnostic {
    let mut d = error(code, message);
    d.unit = Some(unit.into());
    d.field = Some(field.into());
    d.location = at.cloned().map(Box::new);
    d
}

impl<'a> Text<'a> {
    fn new(entry: &'a str, text: &'a str, locations: &'a BTreeMap<String, SourceLocation>) -> Self {
        let mut shared = BTreeMap::<(&str, usize), Vec<&str>>::new();
        for (pointer, at) in locations {
            shared
                .entry((at.file.as_str(), at.byte_offset))
                .or_default()
                .push(pointer.as_str());
        }
        Self {
            entry,
            text,
            locations,
            shared,
        }
    }

    /// Where a field's value starts in the entry module, when Factory may write it there.
    fn writable(
        &self,
        unit: &str,
        pointer: &str,
        field: &str,
    ) -> Result<Option<usize>, Diagnostic> {
        let Some(at) = self.locations.get(pointer) else {
            return Ok(None);
        };
        if at.file != self.entry {
            return Err(refusal("workflow.edit_elsewhere", unit, field,
                format!("{unit}'s {field} is written in {} at line {}, not in the song's file; Factory edits only the song's own file. Edit it there by hand.",
                    at.file, at.line), Some(at)));
        }
        let others = self.shared[&(at.file.as_str(), at.byte_offset)]
            .iter()
            .filter(|other| !related(other, pointer))
            .count();
        if others > 0 {
            return Err(refusal("workflow.edit_shared", unit, field,
                format!("{unit}'s {field} is one value the song shares through a const at line {} with {others} other field(s); writing it would change them too. Give the unit its own value in the song first.",
                    at.line), Some(at)));
        }
        Ok(Some(at.byte_offset))
    }

    fn replace(
        &self,
        unit: &str,
        pointer: &str,
        field: &str,
        value: &Value,
    ) -> Result<Option<Splice>, Diagnostic> {
        let Some(start) = self.writable(unit, pointer, field)? else {
            return Ok(None);
        };
        let end = literal_end(self.text, start).ok_or_else(|| {
            refusal(
                "workflow.edit_unreadable",
                unit,
                field,
                format!("{unit}'s {field} is not a literal Factory can read back."),
                self.locations.get(pointer),
            )
        })?;
        if has_comment(&self.text[start..end]) {
            return Err(refusal(
                "workflow.edit_by_hand",
                unit,
                field,
                format!(
                    "{unit}'s {field} holds comments Factory will not rewrite; edit it by hand."
                ),
                self.locations.get(pointer),
            ));
        }
        Ok(Some(Splice {
            start,
            end,
            text: value.to_string(),
            pointer: pointer.into(),
        }))
    }

    /// Write a field: replace its value, or insert it into its object when the song leaves it out.
    fn set(
        &self,
        unit: &str,
        object: &str,
        name: &str,
        value: &Value,
    ) -> Result<Splice, Diagnostic> {
        let pointer = format!("{object}/{name}");
        let field = pointer.split('/').skip(3).collect::<Vec<_>>().join(".");
        if let Some(splice) = self.replace(unit, &pointer, &field, value)? {
            return Ok(splice);
        }
        let parent_field = object.split('/').skip(3).collect::<Vec<_>>().join(".");
        let parent_field = if parent_field.is_empty() {
            "the unit".to_owned()
        } else {
            parent_field
        };
        let start = self.writable(unit, object, &parent_field)?.ok_or_else(|| {
            refusal(
                "workflow.edit_unreadable",
                unit,
                &field,
                format!("{unit} has no {parent_field} to write {name} into."),
                None,
            )
        })?;
        let open = object_open(self.text, start).ok_or_else(|| {
            refusal(
                "workflow.edit_unreadable",
                unit,
                &field,
                format!("{unit}'s {parent_field} is not an object Factory can write into."),
                self.locations.get(object),
            )
        })?;
        let after = open + 1;
        let rest = &self.text[after..];
        let space = &rest[..rest.len() - rest.trim_start().len()];
        let first = rest.trim_start().chars().next();
        let key = if matches!(first, Some('"') | Some('\'')) {
            format!("\"{name}\"")
        } else {
            name.to_owned()
        };
        let (at, text) = if first == Some('}') {
            (after, format!(" {key}: {value} "))
        } else {
            (after + space.len(), format!("{key}: {value},{space}"))
        };
        Ok(Splice {
            start: at,
            end: at,
            text,
            pointer,
        })
    }
}

/// The byte after a literal that starts at `start`: a string, an array or object (brackets matched outside strings and
/// comments), or a bare word or number.
fn literal_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let first = *bytes.get(start)?;
    let string_end = |from: usize, quote: u8| -> Option<usize> {
        let mut i = from + 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b if b == quote => return Some(i + 1),
                _ => i += 1,
            }
        }
        None
    };
    match first {
        b'"' | b'\'' | b'`' => string_end(start, first),
        b'[' | b'{' => {
            let mut depth = 0usize;
            let mut i = start;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' | b'\'' | b'`' => {
                        i = string_end(i, bytes[i])?;
                        continue;
                    }
                    b'/' if bytes.get(i + 1) == Some(&b'/') => {
                        i = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
                        continue;
                    }
                    b'/' if bytes.get(i + 1) == Some(&b'*') => {
                        i = text[i + 2..].find("*/").map(|n| i + 2 + n + 2)?;
                        continue;
                    }
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        _ => {
            let len = text[start..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || "_$.-+".contains(c)))
                .unwrap_or(text.len() - start);
            (len > 0).then_some(start + len)
        }
    }
}

/// Whether a literal's text holds a comment outside its strings.
fn has_comment(literal: &str) -> bool {
    let bytes = literal.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'"' | b'\'' | b'`') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'/' if matches!(bytes.get(i + 1), Some(b'/') | Some(b'*')) => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// The `{` an object's location names: the brace itself, or a builder call's (`unit({`).
fn object_open(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start..];
    if rest.starts_with('{') {
        return Some(start);
    }
    let word = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))?;
    let after = rest[word..].trim_start();
    let after = after.strip_prefix('(')?.trim_start();
    after.starts_with('{').then(|| text.len() - after.len())
}

fn unit_index(source: &WorkflowSource, key: &str) -> Result<usize, Diagnostic> {
    source
        .units
        .iter()
        .position(|u| u.key == key)
        .ok_or_else(|| {
            let mut d = error(
                "workflow.edit_unit",
                format!("{key} is not a unit of this song"),
            );
            d.unit = Some(key.into());
            d
        })
}

fn splices(song: &Song, text: &str, edits: &[Edit]) -> Result<Vec<Splice>, Diagnostic> {
    let source = &song.loaded.source;
    let basis = source
        .source
        .authoring
        .as_ref()
        .expect("a loaded song is authored");
    let writer = Text::new(&song.entry, text, &basis.locations);
    let mut out = Vec::new();
    for edit in edits {
        match edit {
            Edit::Allocate { units, to } => {
                if units.is_empty() {
                    return Err(error(
                        "workflow.edit_invalid",
                        "Allocate names at least one unit",
                    ));
                }
                for key in units {
                    let index = unit_index(source, key)?;
                    let unit = &source.units[index];
                    let requirements = format!("/units/{index}/agentRequirements");
                    match to.kind.as_str() {
                        "agent" => out.push(writer.set(
                            key,
                            &requirements,
                            "agentRefs",
                            &json!([to.reference]),
                        )?),
                        "agent-set" => {
                            if !unit.agent_requirements.agent_refs.is_empty() {
                                out.push(writer.set(
                                    key,
                                    &requirements,
                                    "agentRefs",
                                    &json!([]),
                                )?);
                            }
                            out.push(writer.set(
                                key,
                                &requirements,
                                "agentSetRefs",
                                &json!([to.reference]),
                            )?);
                        }
                        other => {
                            return Err(error(
                                "workflow.edit_invalid",
                                format!(
                                "Allocate gives a unit to an agent or an agent set, not {other:?}"
                            ),
                            ))
                        }
                    }
                    if unit.composition.is_some() {
                        out.push(writer.set(
                            key,
                            &format!("/units/{index}/composition"),
                            "actor",
                            &json!(to.reference),
                        )?);
                    }
                }
            }
            Edit::Connect { unit, waits_on } => {
                let index = unit_index(source, unit)?;
                let mut dependencies = source.units[index].dependencies.clone();
                if !dependencies.contains(waits_on) {
                    dependencies.push(waits_on.clone());
                    out.push(writer.set(
                        unit,
                        &format!("/units/{index}"),
                        "dependencies",
                        &json!(dependencies),
                    )?);
                }
            }
            Edit::Heads { unit, heads } => {
                let index = unit_index(source, unit)?;
                if source.units[index].composition.is_none() {
                    let mut d = error("workflow.edit_composition",
                        format!("{unit} has no composition to write heads into: a composition names its actor, interpretation, whole, resolve path, context resolution and sources besides its heads. Compose it in the song first."));
                    d.unit = Some(unit.clone());
                    d.field = Some("composition".into());
                    return Err(d);
                }
                if heads.is_empty() {
                    return Err(error(
                        "workflow.edit_invalid",
                        "A heads edit names at least one head",
                    ));
                }
                for (name, value) in heads {
                    if !HEADS.contains(&name.as_str()) {
                        return Err(error(
                            "workflow.edit_invalid",
                            format!("{name} is not a head; the heads are {}", HEADS.join(", ")),
                        ));
                    }
                    out.push(writer.set(
                        unit,
                        &format!("/units/{index}/composition"),
                        name,
                        &json!(value),
                    )?);
                }
            }
        }
    }
    out.sort_by_key(|s| (s.start, s.end));
    for pair in out.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if b.start < a.end || (a.start == b.start && a.end == b.end && a.end > a.start) {
            return Err(error("workflow.edit_conflict",
                format!("Two edits in one request write the same field ({} and {}); send them one at a time.", a.pointer, b.pointer)));
        }
    }
    Ok(out)
}

fn apply(text: &str, splices: &[Splice]) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    let mut at = 0;
    for splice in splices {
        out.push_str(&text[at..splice.start]);
        out.push_str(&splice.text);
        at = splice.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Compile the edited song apart from the project, from the same modules at the same paths, so the compiler's word names
/// the song's own file and line, and nothing is written when it refuses.
fn compile_apart(song: &Song, text: &str) -> Result<AuthoredWorkflow, Diagnostic> {
    let basis = song
        .loaded
        .source
        .source
        .authoring
        .as_ref()
        .expect("a loaded song is authored");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let apart = std::env::temp_dir().join(format!(
        "factory-workflow-edit-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&apart).map_err(|e| failure(format!("Cannot compile the edit apart: {e}")))?;
    let compiled = (|| {
        for (path, module) in &basis.modules {
            let target = apart.join(path);
            if let Some(dir) = target.parent() {
                fs::create_dir_all(dir).map_err(|e| failure(e.to_string()))?;
            }
            let bytes = if *path == song.entry {
                text
            } else {
                module.content.as_str()
            };
            fs::write(&target, bytes).map_err(|e| failure(e.to_string()))?;
        }
        load_workflow(&apart, Path::new(&song.entry))
    })();
    let _ = fs::remove_dir_all(&apart);
    compiled
}

/// Replace the song's file with the edited text in one rename, if its bytes are still the ones that were edited.
fn write_song(song: &Song, digest: &str, text: &str) -> Result<(), Diagnostic> {
    let stale = || {
        error("workflow.edit_stale", "The song's file changed while the edit was checked; read the score again and repeat the edit.")
    };
    let before = fs::read(&song.file).map_err(|e| failure(e.to_string()))?;
    if blake3::hash(&before).to_hex().as_str() != digest {
        return Err(stale());
    }
    let name = song
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("song");
    let temp = song
        .file
        .with_file_name(format!(".{name}.factory-edit-{}", std::process::id()));
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&temp, fs::metadata(&song.file)?.permissions())?;
        if blake3::hash(&fs::read(&song.file)?).to_hex().as_str() != digest {
            return Err(std::io::Error::other("changed"));
        }
        fs::rename(&temp, &song.file)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&temp);
        return Err(if e.to_string() == "changed" {
            stale()
        } else {
            failure(e.to_string())
        });
    }
    Ok(())
}

/// `factory workflow edit <state> <run-ref> <edit.json|->`.
pub fn edit(state_path: &Path, run: &RunRef, request: Value) -> Result<Value, Diagnostic> {
    let request: EditRequest = serde_json::from_value(request.clone()).map_err(|e| {
        error(
            "workflow.edit_invalid",
            format!("{e}; send a {EDIT_CONTRACT} request"),
        )
    })?;
    if request.contract != EDIT_CONTRACT {
        return Err(error(
            "workflow.edit_invalid",
            format!("The edit names {}, not {EDIT_CONTRACT}", request.contract),
        ));
    }
    if request.edits.is_empty() {
        return Err(error(
            "workflow.edit_invalid",
            "An edit names at least one change",
        ));
    }
    let (_, _, retained) = retained_source(state_path, run)?;
    let song = song(state_path, &retained).map_err(|unplaced| {
        let mut d = error(
            "workflow.song_unplaced",
            format!("{} No edit can be written.", unplaced.why),
        );
        d.location = unplaced.diagnostic.and_then(|d| d.location.clone());
        d
    })?;
    let basis = song
        .loaded
        .source
        .source
        .authoring
        .as_ref()
        .expect("a loaded song is authored");
    if request.expected_revision != basis.revision {
        return Err(error("workflow.edit_stale",
            format!("The song's file changed since it was read (it is {} now, the edit expected {}); read the score again and repeat the edit.",
                basis.revision, request.expected_revision)));
    }
    let module = &basis.modules[&song.entry];
    let splices = splices(&song, &module.content, &request.edits)?;
    let text = apply(&module.content, &splices);
    let changed = splices
        .iter()
        .map(|s| s.pointer.clone())
        .collect::<BTreeSet<_>>();
    if text != module.content {
        compile_apart(&song, &text)?;
        write_song(&song, &module.digest, &text)?;
    }
    let written = load_workflow(&song.root, Path::new(&song.entry))?;
    let after = written
        .source
        .source
        .authoring
        .as_ref()
        .expect("a loaded song is authored");
    Ok(json!({
        "contract": EDIT_RECEIPT_CONTRACT,
        "runRef": run,
        "song": {"path": song.file, "entry": song.entry, "sourceRef": written.source.source.reference},
        "previousRevision": basis.revision,
        "revision": after.revision,
        "semanticDigest": written.source.source.digest,
        "edits": request.edits.len(),
        "changed": changed,
        "check": {"state": "admitted", "by": "factory workflow check"},
        "performed": written.source.source.digest == retained.source.digest,
        "performs": PERFORMS,
        "execution": "not-requested",
    }))
}
