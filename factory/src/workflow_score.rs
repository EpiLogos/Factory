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
//!
//! A song's clip dropped on a track (O-I #844, `canon`) adds one unit to the song with the thread form canon: it names the clip
//! by its ref, takes the song's words, and the score draws the clip's song inside it (song_clip.rs). A clip from another
//! project is kept in this one as an import first, written only once the song admits the unit. A unit written whole (`unit`, in
//! the workflow SDK's words) adds a unit of the song's own: how Compose writes a first score, and how a map's tickets become units.
//!
//! An edit can be checked without being written (`--check`): the same compare-and-swap, splice and compile, answered with the
//! receipt it would give and `written: false`, so a proposal (Anima's Compose, O-I #844) is admitted before anyone accepts it.
//! A project's map gives a development song its units (`map`): the open sub-issues of a `wayfinder:map` issue, in the order
//! their blocking gives, read through the forge port and never written.
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
pub(crate) struct Song {
    root: PathBuf,
    file: PathBuf,
    entry: String,
    pub(crate) loaded: AuthoredWorkflow,
}

/// Why the score reads the retained edition instead of a file, and the compiler's word when the file does not compile.
pub(crate) struct Unplaced {
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

pub(crate) fn song(state_path: &Path, retained: &WorkflowSource) -> Result<Song, Unplaced> {
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

pub(crate) fn retained_source(
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
    // A unit that names a song's clip is that song, standing as one note: the clip's score drawn inside it (O-I #844).
    let here = crate::song_clip::project(state_path).ok();
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
        .zip(&source.units)
        .map(|(mut drawn, unit)| {
            if let Some(canon) = crate::song_clip::canon_of(
                here.as_ref().map(|p| p.root.as_path()),
                here.as_ref().map(|p| p.key.as_str()),
                unit.subject_ref.as_str(),
            ) {
                for (field, value) in canon {
                    drawn[field] = value;
                }
                // Each time the song ran again from this unit, a take of its own (`factory workflow perform`, song_take.rs).
                drawn["takes"] = json!(crate::song_take::takes_of(&state, run, &unit.key));
            }
            drawn
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
    /// A song's clip made one unit of this song, with the thread form canon (O-I #844).
    Canon {
        /// The clip's file: this project's (`.factory/songs/<name>.song.json`) or another project's, imported.
        clip: String,
        to: Allocation,
        /// The new unit's key; the clip's name when none is named, made unique in the song.
        #[serde(default)]
        key: Option<String>,
        #[serde(default, rename = "waitsOn")]
        waits_on: Vec<String>,
    },
    /// A unit written whole, in the workflow SDK's words (O-I #844: Compose writes a first score, and a project's map gives a
    /// development song its units). Its key is new to the song, it waits only on units the song holds or this request wrote
    /// before it, and the compiler admits or refuses it as the song's own.
    Unit {
        unit: Value,
    },
}

/// A canon edit made ready before the splice: the unit's key, its source text and the import it needs.
struct Canon {
    key: String,
    text: String,
    reference: String,
    import: Option<(PathBuf, Value)>,
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

impl Text<'_> {
    /// The builder call an array's element is written in (`unit({…})`), when it is one: where the call starts and its name. An
    /// element's location names the call or the brace it opens.
    fn builder_at(&self, at: usize) -> Option<(usize, &str)> {
        let word = |end: usize| {
            let start = self.text[..end]
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
                .map_or(0, |i| i + 1);
            (start < end).then(|| (start, &self.text[start..end]))
        };
        if self.text.as_bytes().get(at) == Some(&b'{') {
            let before = self.text[..at].trim_end();
            let call = before.strip_suffix('(')?.trim_end();
            return word(call.len());
        }
        let rest = &self.text[at..];
        let end = at + rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))?;
        self.text[end..]
            .trim_start()
            .starts_with('(')
            .then(|| word(end))
            .flatten()
    }

    /// The byte after the element at `at`: a literal, or the builder call it is written in.
    fn item_end(&self, at: usize) -> Option<usize> {
        let Some((call, _)) = self.builder_at(at) else {
            return literal_end(self.text, at);
        };
        let open = object_open(self.text, call)?;
        let close = literal_end(self.text, open)?;
        let rest = &self.text[close..];
        let space = rest.len() - rest.trim_start().len();
        rest[space..].starts_with(')').then_some(close + space + 1)
    }

    /// Add an item at the end of the array at `array`, written as its first item is (a builder call or a literal) and on its
    /// own line at that item's indentation.
    fn append(
        &self,
        unit: &str,
        array: &str,
        count: usize,
        item: &Value,
    ) -> Result<Splice, Diagnostic> {
        let field = array.trim_start_matches('/');
        let unreadable = |why: String| {
            refusal(
                "workflow.edit_unreadable",
                unit,
                field,
                why,
                self.locations.get(array),
            )
        };
        let start = self
            .writable(unit, array, field)?
            .ok_or_else(|| unreadable(format!("The song has no {field} list to add {unit} to.")))?;
        if self.text.as_bytes().get(start) != Some(&b'[') {
            return Err(unreadable(format!("The song's {field} are not a list written in its file, so {unit} cannot be added to them.")));
        }
        let end = literal_end(self.text, start).ok_or_else(|| {
            unreadable(format!(
                "The song's {field} are not a list Factory can read back."
            ))
        })?;
        let close = end - 1;
        let first = self
            .locations
            .get(&format!("{array}/0"))
            .map(|at| at.byte_offset);
        let builder = first.and_then(|at| self.builder_at(at));
        let indent = first
            .map(|at| {
                let at = builder.map_or(at, |(call, _)| call);
                let line = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
                let lead = &self.text[line..at];
                if lead.chars().all(char::is_whitespace) {
                    lead.to_owned()
                } else {
                    "  ".to_owned()
                }
            })
            .unwrap_or_else(|| "    ".to_owned());
        let body = serde_json::to_string_pretty(item)
            .map_err(|e| failure(e.to_string()))?
            .replace('\n', &format!("\n{indent}"));
        let written = match builder {
            Some((_, name)) => format!("{name}({body})"),
            None => body,
        };
        let after = match count.checked_sub(1) {
            None => start + 1,
            Some(last) => {
                let at = self
                    .locations
                    .get(&format!("{array}/{last}"))
                    .map(|at| at.byte_offset)
                    .ok_or_else(|| {
                        unreadable(format!("The song's last {field} item is not located."))
                    })?;
                self.item_end(at).ok_or_else(|| {
                    unreadable(format!(
                        "The song's last {field} item is not a literal Factory can read back."
                    ))
                })?
            }
        };
        let tail = &self.text[after..close];
        if has_comment(tail) {
            return Err(refusal("workflow.edit_by_hand", unit, field,
                format!("A comment stands after the song's last {field} item; Factory will not write around it. Add {unit} by hand."),
                self.locations.get(array)));
        }
        let comma = tail.trim_start().starts_with(',');
        let (at, text) = if count == 0 {
            (after, format!("\n{indent}{written}\n"))
        } else if comma {
            (
                after + tail.find(',').unwrap_or(0) + 1,
                format!("\n{indent}{written},"),
            )
        } else {
            (after, format!(",\n{indent}{written}"))
        };
        Ok(Splice {
            start: at,
            end: at,
            text,
            pointer: format!("{array}/{count}"),
        })
    }
}

/// The byte after a literal that starts at `start`: a string, an array or object (brackets matched outside strings and
/// comments), or a bare word or number.
pub(crate) fn literal_end(text: &str, start: usize) -> Option<usize> {
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

/// A whole unit's key: the object a unit edit writes names one.
fn unit_key(unit: &Value) -> Result<String, Diagnostic> {
    if !unit.is_object() {
        return Err(error(
            "workflow.edit_invalid",
            "A unit edit writes a whole unit: an object in the workflow SDK's words",
        ));
    }
    unit["key"]
        .as_str()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                "workflow.edit_invalid",
                "A unit edit writes a whole unit, and a unit names its key",
            )
        })
}

/// What an edit adds to the song: the units it writes whole and the clips it drops as canon units, in its order.
fn added_of(edits: &[Edit], canons: &[Canon]) -> Vec<Value> {
    let mut canons = canons.iter();
    edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::Unit { unit } => {
                Some(json!({"key": unit["key"], "subjectRef": unit["subjectRef"], "form": "unit"}))
            }
            Edit::Canon { .. } => canons
                .next()
                .map(|c| json!({"key": c.key, "subjectRef": c.reference, "form": "canon"})),
            _ => None,
        })
        .collect()
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

fn splices(
    song: &Song,
    text: &str,
    edits: &[Edit],
    canons: &[Canon],
) -> Result<Vec<Splice>, Diagnostic> {
    let source = &song.loaded.source;
    let basis = source
        .source
        .authoring
        .as_ref()
        .expect("a loaded song is authored");
    let writer = Text::new(&song.entry, text, &basis.locations);
    let mut out = Vec::new();
    let mut canons = canons.iter();
    let mut added = 0;
    // The units the song holds, and those this request writes as it goes: a new unit's key is new, and it waits on these.
    let mut keys = source
        .units
        .iter()
        .map(|u| u.key.clone())
        .collect::<BTreeSet<_>>();
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
            Edit::Unit { unit } => {
                let key = unit_key(unit)?;
                if !keys.insert(key.clone()) {
                    let mut d = error(
                        "workflow.edit_unit",
                        format!(
                            "{key} is already a unit of this song; name the new unit another key"
                        ),
                    );
                    d.unit = Some(key);
                    return Err(d);
                }
                for wait in unit["dependencies"].as_array().into_iter().flatten() {
                    let wait = wait.as_str().unwrap_or_default();
                    if wait == key || !keys.contains(wait) {
                        let mut d = error(
                            "workflow.edit_unit",
                            format!("{wait} is not a unit of this song, so {key} cannot wait on it: a new unit waits on the units the song holds, or on those this edit wrote before it"),
                        );
                        d.unit = Some(key);
                        d.field = Some("dependencies".into());
                        return Err(d);
                    }
                }
                let mut splice = writer.append(&key, "/units", source.units.len(), unit)?;
                splice.pointer = format!("/units/{}", source.units.len() + added);
                added += 1;
                out.push(splice);
            }
            Edit::Canon { .. } => {
                let canon = canons.next().expect("every canon edit is made ready");
                if !keys.insert(canon.key.clone()) {
                    let mut d = error(
                        "workflow.edit_conflict",
                        format!(
                            "{} is written twice in one edit; send the dropped song another key",
                            canon.key
                        ),
                    );
                    d.unit = Some(canon.key.clone());
                    return Err(d);
                }
                let unit: Value =
                    serde_json::from_str(&canon.text).map_err(|e| failure(e.to_string()))?;
                let mut splice = writer.append(&canon.key, "/units", source.units.len(), &unit)?;
                // Two canons in one request land at one place, in their order.
                splice.pointer = format!("/units/{}", source.units.len() + added);
                added += 1;
                out.push(splice);
            }
        }
    }
    // Stable: insertions at one place keep the order they were asked in.
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
    compile_modules_apart(basis, &song.entry, text)
}

/// Compile a song's modules apart from any project, its entry module's text replaced, at the same paths (a take's edition,
/// song_take.rs, is compiled so too).
pub(crate) fn compile_modules_apart(
    basis: &crate::workflow_authoring::AuthoredBasis,
    entry: &str,
    text: &str,
) -> Result<AuthoredWorkflow, Diagnostic> {
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
            let bytes = if path == entry {
                text
            } else {
                module.content.as_str()
            };
            fs::write(&target, bytes).map_err(|e| failure(e.to_string()))?;
        }
        load_workflow(&apart, Path::new(entry))
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

/// Each canon edit made ready: its clip read (and, from another project, its import prepared), its key made unique in the
/// song, and its unit written in the song's words.
fn canons_of(state_path: &Path, song: &Song, edits: &[Edit]) -> Result<Vec<Canon>, Diagnostic> {
    if !edits.iter().any(|e| matches!(e, Edit::Canon { .. })) {
        return Ok(vec![]);
    }
    let project = crate::song_clip::project(state_path)
        .map_err(|why| error("workflow.song_unplaced", why))?;
    let mut keys = song
        .loaded
        .source
        .units
        .iter()
        .map(|u| u.key.clone())
        .collect::<BTreeSet<_>>();
    let mut out = Vec::new();
    for edit in edits {
        let Edit::Canon {
            clip,
            to,
            key,
            waits_on,
        } = edit
        else {
            continue;
        };
        let dropped = crate::song_clip::dropped(&project.root, &project.key, Path::new(clip))?;
        if out.iter().any(|c: &Canon| {
            c.import.is_some()
                && dropped
                    .import
                    .as_ref()
                    .is_some_and(|(p, _)| c.import.as_ref().is_some_and(|(q, _)| p == q))
        }) {
            return Err(error(
                "workflow.edit_conflict",
                "One request drops the same clip twice; drop it once.",
            ));
        }
        for wait in waits_on {
            if !keys.contains(wait) {
                let mut d = error(
                    "workflow.edit_unit",
                    format!(
                        "{wait} is not a unit of this song, so the dropped song cannot wait on it"
                    ),
                );
                d.unit = Some(wait.clone());
                return Err(d);
            }
        }
        let base = key
            .clone()
            .unwrap_or_else(|| dropped.clip["name"].as_str().unwrap_or("song").to_owned());
        if key.is_some() && keys.contains(&base) {
            let mut d = error(
                "workflow.edit_unit",
                format!("{base} is already a unit of this song; name the dropped song another key"),
            );
            d.unit = Some(base);
            return Err(d);
        }
        let mut chosen = base.clone();
        let mut n = 2;
        while keys.contains(&chosen) {
            chosen = format!("{base}-{n}");
            n += 1;
        }
        keys.insert(chosen.clone());
        let unit =
            crate::song_clip::canon_unit(&dropped, &chosen, (&to.kind, &to.reference), waits_on)?;
        out.push(Canon {
            key: chosen,
            text: unit.to_string(),
            reference: dropped.reference,
            import: dropped.import,
        });
    }
    Ok(out)
}

/// `factory workflow edit <state> <run-ref> <edit.json|->`.
pub fn edit(state_path: &Path, run: &RunRef, request: Value) -> Result<Value, Diagnostic> {
    edit_song(state_path, run, request, true)
}

/// `factory workflow edit <state> <run-ref> <edit.json|-> --check`: the edit admitted or refused, the song's file untouched.
pub fn check_edit(state_path: &Path, run: &RunRef, request: Value) -> Result<Value, Diagnostic> {
    edit_song(state_path, run, request, false)
}

fn edit_song(
    state_path: &Path,
    run: &RunRef,
    request: Value,
    write: bool,
) -> Result<Value, Diagnostic> {
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
    let canons = canons_of(state_path, &song, &request.edits)?;
    let splices = splices(&song, &module.content, &request.edits, &canons)?;
    let text = apply(&module.content, &splices);
    let changed = splices
        .iter()
        .map(|s| s.pointer.clone())
        .collect::<BTreeSet<_>>();
    let mut imported = Vec::<&Path>::new();
    if !write {
        if text != module.content {
            compile_apart(&song, &text)?;
        }
        return Ok(json!({
            "contract": EDIT_RECEIPT_CONTRACT,
            "runRef": run,
            "song": {"path": song.file, "entry": song.entry, "sourceRef": song.loaded.source.source.reference},
            "previousRevision": basis.revision,
            "revision": basis.revision,
            "edits": request.edits.len(),
            "changed": changed,
            "added": added_of(&request.edits, &canons),
            "check": {"state": "admitted", "by": "factory workflow check"},
            "written": false,
            "execution": "not-requested",
        }));
    }
    if text != module.content {
        compile_apart(&song, &text)?;
        // A clip from another project is kept here once the song admits its unit, and taken out again if the song's file
        // cannot be written.
        for canon in &canons {
            if let Some((path, clip)) = &canon.import {
                if let Err(d) = crate::song_clip::write_import(path, clip) {
                    for path in &imported {
                        let _ = fs::remove_file(path);
                    }
                    return Err(d);
                }
                imported.push(path);
            }
        }
        if let Err(d) = write_song(&song, &module.digest, &text) {
            for path in &imported {
                let _ = fs::remove_file(path);
            }
            return Err(d);
        }
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
        "added": added_of(&request.edits, &canons),
        "imports": imported,
        "check": {"state": "admitted", "by": "factory workflow check"},
        "written": true,
        "performed": written.source.source.digest == retained.source.digest,
        "performs": PERFORMS,
        "execution": "not-requested",
    }))
}

pub const MAP_CONTRACT: &str = "factory.project-map-reading/v1";
const MAP_LABEL: &str = "wayfinder:map";
/// The most sub-issues a map is read to (ten pages of a hundred), and the wall budget of the whole read.
const MAP_PAGES: usize = 10;
const MAP_BUDGET: std::time::Duration = std::time::Duration::from_secs(120);

fn map_refusal(code: &str, message: impl Into<String>) -> Diagnostic {
    error(code, message)
}

/// One ticket as the forge reads it: its repository, number, title, state and page.
struct Ticket {
    repository: String,
    number: u64,
    title: String,
    open: bool,
    url: String,
}

impl Ticket {
    fn of(row: &Value, fallback: &str) -> Option<Ticket> {
        let number = row["number"].as_u64()?;
        let repository = row["repository_url"]
            .as_str()
            .and_then(|url| url.split("/repos/").nth(1))
            .filter(|repo| crate::sensing_sources::valid_github_scope(repo))
            .unwrap_or(fallback)
            .to_owned();
        Some(Ticket {
            url: row["html_url"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("https://github.com/{repository}/issues/{number}")),
            title: row["title"].as_str().unwrap_or("").to_owned(),
            open: row["state"] == "open",
            repository,
            number,
        })
    }
    fn reference(&self) -> String {
        format!("github:{}:issue:{}", self.repository, self.number)
    }
    /// The unit's key: `issue-<n>` for the map's own repository, the repository's name before it for another.
    fn key(&self, home: &str) -> String {
        if self.repository == home {
            return format!("issue-{}", self.number);
        }
        let name: String = self
            .repository
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        format!("{name}-{}", self.number)
    }
}

/// A development song's units from the project's map (O-I #844; DLW manual §19.1 Songs, "A project's map gives a development song
/// its units: each ticket is a unit"). The map is an issue labelled `wayfinder:map` (the Wayfinder skill's map); its open
/// sub-issues are the units, in whichever repository they stand; blocked-by gives their order: a unit's wave is one past the
/// latest wave of the open units of the map that block it. A closed ticket blocks nothing and is no unit; an open blocker the map
/// does not hold is named where its unit waits on it. Read through the forge port Factory reads pull requests by
/// (attempt_delivery.rs: an explicit absolute `gh`, `gh api`, each read bounded) and never written.
pub fn map(gh: &str, repository: &str, number: u64) -> Result<Value, Diagnostic> {
    if !crate::sensing_sources::valid_github_scope(repository) {
        return Err(map_refusal(
            "workflow.map_invalid",
            format!(
                "{repository} names no repository: a map is read from an exact owner/repository"
            ),
        ));
    }
    let deadline = std::time::Instant::now() + MAP_BUDGET;
    let read = |endpoint: String| {
        crate::attempt_delivery::gh(gh, endpoint.clone(), deadline).map_err(|why| {
            map_refusal(
                "workflow.map_unread",
                format!("The forge did not answer {endpoint}: {why}"),
            )
        })
    };
    let rows = |value: Value, endpoint: &str| -> Result<Vec<Value>, Diagnostic> {
        match value {
            Value::Array(rows) => Ok(rows),
            _ => Err(map_refusal(
                "workflow.map_unread",
                format!("The forge answered {endpoint} with no list of issues"),
            )),
        }
    };
    let issue = read(format!("repos/{repository}/issues/{number}"))?;
    let map = Ticket::of(&issue, repository).ok_or_else(|| {
        map_refusal(
            "workflow.map_unread",
            format!("The forge answered {repository}#{number} without its number"),
        )
    })?;
    let labelled = issue["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|label| label["name"] == MAP_LABEL || label == MAP_LABEL);
    if issue.get("pull_request").is_some() || !labelled {
        return Err(map_refusal(
            "workflow.not_a_map",
            format!(
                "{repository}#{number} ({}) carries no {MAP_LABEL} label: it is not a map, so it gives no song its units",
                map.title
            ),
        ));
    }
    let mut children = Vec::new();
    for page in 1..=MAP_PAGES {
        let endpoint =
            format!("repos/{repository}/issues/{number}/sub_issues?per_page=100&page={page}");
        let page_rows = rows(read(endpoint.clone())?, &endpoint)?;
        let full = page_rows.len() == 100;
        children.extend(
            page_rows
                .iter()
                .filter_map(|row| Ticket::of(row, repository)),
        );
        if !full {
            break;
        }
        if page == MAP_PAGES {
            return Err(map_refusal(
                "workflow.map_too_large",
                format!("{repository}#{number} holds more than {} sub-issues: Factory reads a map to that many", MAP_PAGES * 100),
            ));
        }
    }
    let (open, closed): (Vec<Ticket>, Vec<Ticket>) =
        children.into_iter().partition(|ticket| ticket.open);
    let keys: BTreeMap<String, String> = open
        .iter()
        .map(|ticket| (ticket.reference(), ticket.key(repository)))
        .collect();
    let mut blocked: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut outside: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for ticket in &open {
        let key = ticket.key(repository);
        let endpoint = format!(
            "repos/{}/issues/{}/dependencies/blocked_by?per_page=100",
            ticket.repository, ticket.number
        );
        let mut on = Vec::new();
        let mut waits = Vec::new();
        for blocker in rows(read(endpoint.clone())?, &endpoint)?
            .iter()
            .filter_map(|row| Ticket::of(row, &ticket.repository))
            .filter(|blocker| blocker.open)
        {
            match keys.get(&blocker.reference()) {
                Some(unit) => on.push(unit.clone()),
                None => waits.push(json!({"ticket": blocker.reference(), "title": blocker.title, "url": blocker.url})),
            }
        }
        on.sort();
        on.dedup();
        blocked.insert(key.clone(), on);
        outside.insert(key, waits);
    }
    // The waves: a unit stands one past the latest of the units that block it (Kahn's layering); blocking in a circle gives none.
    let mut wave: BTreeMap<String, usize> = BTreeMap::new();
    while wave.len() < blocked.len() {
        let ready: Vec<(String, usize)> = blocked
            .iter()
            .filter(|(key, _)| !wave.contains_key(*key))
            .filter_map(|(key, on)| {
                on.iter()
                    .map(|unit| wave.get(unit).copied())
                    .collect::<Option<Vec<_>>>()
                    .map(|waves| (key.clone(), waves.into_iter().max().map_or(1, |w| w + 1)))
            })
            .collect();
        if ready.is_empty() {
            let circle: Vec<&str> = blocked
                .keys()
                .filter(|key| !wave.contains_key(*key))
                .map(String::as_str)
                .collect();
            return Err(map_refusal(
                "workflow.map_cycle",
                format!(
                    "The blocking on {repository}#{number} runs in a circle among {}: a song takes no order from it",
                    circle.join(", ")
                ),
            ));
        }
        wave.extend(ready);
    }
    let mut ordered: Vec<&Ticket> = open.iter().collect();
    ordered.sort_by_key(|ticket| {
        (
            wave[&ticket.key(repository)],
            ticket.repository != repository,
            ticket.number,
        )
    });
    let units: Vec<Value> = ordered
        .iter()
        .map(|ticket| {
            let key = ticket.key(repository);
            json!({"key": key, "ticket": ticket.reference(), "title": ticket.title, "url": ticket.url,
                "wave": format!("wave-{}", wave[&key]), "blockedBy": blocked[&key], "waitsOutside": outside[&key]})
        })
        .collect();
    let mut waves: Vec<Value> = Vec::new();
    for unit in &units {
        match waves.last_mut() {
            Some(last) if last["wave"] == unit["wave"] => last["units"]
                .as_array_mut()
                .expect("units")
                .push(unit["key"].clone()),
            _ => waves.push(json!({"wave": unit["wave"], "units": [unit["key"]]})),
        }
    }
    Ok(json!({
        "contract": MAP_CONTRACT,
        "map": {"ticket": map.reference(), "title": map.title, "url": map.url, "label": MAP_LABEL},
        "units": units,
        "waves": waves,
        "closed": closed.iter().map(Ticket::reference).collect::<Vec<_>>(),
        "written": false,
    }))
}
