//! A song made into one clip, and the clip used whole in other work (O-I #844; DLW manual §19.1 Songs, §10.3 Sets, files and
//! media: "A render is made from something else: it names what it was made from, it can be remade, and it is never edited in
//! place").
//!
//! Export Song… renders a song — its whole score, or the rows (the tracks) and the units (the range) chosen — into one clip
//! (`factory.song-clip/v1`), kept in the project that made it at `.factory/songs/<name>.song.json`. The clip names the set it was
//! made from (the song's file under the project root, its source and its byte revision), carries the rendered score, and is
//! written once: a second export of the same name is refused, and Remake makes it again from the set as it now stands. `songs`
//! lists a project's clips, each with whether its set has moved since it was rendered.
//!
//! Dropped on a track, a clip is one unit of the receiving song with the thread form canon (`factory workflow edit`'s `canon`
//! op, workflow_score.rs): the receiving project keeps its own copy of a clip from another project (an import, which keeps where it
//! came from), and the unit names it by its ref, so the score draws the clip's song inside the unit. Nothing here executes.
use crate::workflow::WorkflowUnitSource;
use crate::workflow_authoring::{error, load_workflow, AuthoredWorkflow, Diagnostic};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const CLIP_CONTRACT: &str = "factory.song-clip/v1";
pub const CLIPS_CONTRACT: &str = "factory.song-clips/v1";
/// A clip's ref: `factory-song:<project key>/<name>`, the project that keeps it and its name there.
pub const SONG_REF_PREFIX: &str = "factory-song:";
const SONGS_DIR: &str = "songs";
const SUFFIX: &str = ".song.json";
/// How deep a clip draws the clips inside its own units (canon within canon).
const DEPTH: usize = 4;
const NEVER_IN_PLACE: &str = "A render is never edited in place: Remake makes it again from the set it names, and to change the song you change it there.";

fn failure(code: &str, message: impl Into<String>) -> Diagnostic {
    error(code, message)
}

/// The project a developmental state is placed in: its root, its key and its ref.
pub(crate) struct Project {
    pub root: PathBuf,
    pub key: String,
    pub reference: String,
}

pub(crate) fn project(state_path: &Path) -> Result<Project, String> {
    let state = state_path
        .canonicalize()
        .map_err(|e| format!("The developmental state cannot be read ({e})."))?;
    let root = state
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == ".factory"))
        .and_then(Path::parent)
        .ok_or_else(|| "The developmental state is not a placed project's (.factory/development-state.json), so it keeps no songs.".to_owned())?;
    let located = crate::project_setup::locate(root)
        .map_err(|e| format!("The project's placement cannot be read ({e})."))?;
    if located["statePath"].as_str() != state.to_str() {
        return Err("The project's placement names another developmental state.".into());
    }
    Ok(Project {
        root: root.to_path_buf(),
        key: located["projectKey"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        reference: located["projectRef"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    })
}

fn placed(state_path: &Path) -> Result<Project, Diagnostic> {
    project(state_path).map_err(|why| failure("workflow.song_unplaced", why))
}

fn songs_dir(root: &Path) -> PathBuf {
    root.join(".factory").join(SONGS_DIR)
}

/// A clip's name: what it is kept under and named by.
fn valid_name(name: &str) -> Result<(), Diagnostic> {
    let ok = !name.is_empty()
        && name.len() <= 96
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !name.contains("..");
    if ok {
        Ok(())
    } else {
        Err(failure("workflow.song_name", format!("{name:?} cannot name a clip: use letters, digits, '-', '_' or '.', starting with a letter or digit, at most 96 characters.")))
    }
}

fn clip_path(root: &Path, name: &str) -> PathBuf {
    songs_dir(root).join(format!("{name}{SUFFIX}"))
}

pub fn clip_ref(project_key: &str, name: &str) -> String {
    format!("{SONG_REF_PREFIX}{project_key}/{name}")
}

/// `factory-song:<project key>/<name>` → its project key and name.
pub fn parse_ref(reference: &str) -> Option<(&str, &str)> {
    let rest = reference.strip_prefix(SONG_REF_PREFIX)?;
    let (key, name) = rest.rsplit_once('/')?;
    (!key.is_empty() && !name.is_empty()).then_some((key, name))
}

/// The row a unit is drawn on: its first agent, else its first agent set, else its first agency (as the score draws it).
fn row_of(unit: &WorkflowUnitSource) -> Option<(&str, &'static str)> {
    let r = &unit.agent_requirements;
    r.agent_refs
        .first()
        .map(|x| (x.as_str(), "agent"))
        .or_else(|| r.agent_set_refs.first().map(|x| (x.as_str(), "agent-set")))
        .or_else(|| r.agency_refs.first().map(|x| (x.as_str(), "agency")))
}

fn heads(unit: &WorkflowUnitSource) -> Value {
    unit.composition.as_ref().map_or(Value::Null, |c| {
        json!({"CPF": c.participation, "CT": c.content, "CP": c.position, "CF": c.frame, "CFP": c.thread,
            "CS": c.sequence, "direction": c.direction})
    })
}

/// What is chosen to render: the rows (the tracks) and the units (the range); none chosen is the whole song.
#[derive(Debug, Default, Clone)]
pub struct Selection {
    pub rows: Option<Vec<String>>,
    pub units: Option<Vec<String>>,
}

/// The song's score as the clip carries it: the chosen units, each on its row with what it waits on inside the selection, its
/// composition's heads and its own words; the rows they stand on; the joins among them; and the ties the selection cut.
fn render_score(
    root: &Path,
    project_key: &str,
    loaded: &AuthoredWorkflow,
    selection: &Selection,
    depth: usize,
) -> Result<Value, Diagnostic> {
    let source = &loaded.source;
    let rows_named = source
        .units
        .iter()
        .filter_map(|u| row_of(u).map(|(r, _)| r.to_owned()))
        .collect::<BTreeSet<_>>();
    if let Some(rows) = &selection.rows {
        if rows.is_empty() {
            return Err(failure(
                "workflow.song_selection",
                "The selection names no row: choose at least one track, or none for the whole song.",
            ));
        }
        for row in rows {
            if !rows_named.contains(row) {
                return Err(failure(
                    "workflow.song_selection",
                    format!(
                        "{row} is no row of the song {}: no unit of it stands on that track.",
                        source.workflow_key
                    ),
                ));
            }
        }
    }
    if let Some(units) = &selection.units {
        if units.is_empty() {
            return Err(failure(
                "workflow.song_selection",
                "The selection names no unit: choose a range, or none for the whole song.",
            ));
        }
        for key in units {
            if !source.units.iter().any(|u| &u.key == key) {
                return Err(failure(
                    "workflow.song_selection",
                    format!("{key} is no unit of the song {}.", source.workflow_key),
                ));
            }
        }
    }
    let chosen = source
        .units
        .iter()
        .filter(|u| {
            selection
                .rows
                .as_ref()
                .is_none_or(|rows| row_of(u).is_some_and(|(r, _)| rows.iter().any(|x| x == r)))
                && selection
                    .units
                    .as_ref()
                    .is_none_or(|units| units.iter().any(|k| k == &u.key))
        })
        .collect::<Vec<_>>();
    if chosen.is_empty() {
        return Err(failure(
            "workflow.song_selection",
            format!(
                "The rows and range chosen hold no unit of the song {}: nothing would be rendered.",
                source.workflow_key
            ),
        ));
    }
    let keys = chosen
        .iter()
        .map(|u| u.key.as_str())
        .collect::<BTreeSet<_>>();
    let mut rows = Vec::<Value>::new();
    let mut seen = BTreeSet::new();
    let mut cut = Vec::<Value>::new();
    let units = chosen
        .iter()
        .map(|unit| {
            let row = row_of(unit);
            if let Some((r, kind)) = row {
                if seen.insert(r.to_owned()) {
                    rows.push(json!({"ref": r, "kind": kind}));
                }
            }
            for dep in &unit.dependencies {
                if !keys.contains(dep.as_str()) {
                    cut.push(json!({"unit": unit.key, "waitsOn": dep}));
                }
            }
            let mut drawn = json!({
                "key": unit.key,
                "row": row.map(|(r, _)| r),
                "rowKind": row.map(|(_, k)| k),
                "dependencies": unit.dependencies.iter().filter(|d| keys.contains(d.as_str())).collect::<Vec<_>>(),
                "heads": heads(unit),
                "concern": unit.developmental_concern,
                "requiredDifference": unit.required_difference,
                "returns": unit.return_contract,
                "returnAddress": unit.return_address,
                "subjectRef": unit.subject_ref,
                "praxisRefs": unit.praxis_refs,
                "capabilityRefs": unit.capability_refs,
                "success": unit.verification_obligations,
                "may": unit.permitted_effects,
                "stop": unit.stop_conditions,
                "escalate": unit.escalation_conditions,
            });
            // A unit that is itself a song's clip carries that clip's score, canon within canon, as deep as DEPTH.
            if let Some((key, name)) = parse_ref(unit.subject_ref.as_str()) {
                if key == project_key && depth > 0 {
                    match read_clip(root, name) {
                        Ok(inner) => {
                            drawn["song"] = inner["score"].clone();
                            drawn["songClip"] = inner["clipRef"].clone();
                        }
                        Err(d) => drawn["songWhy"] = json!(d.message),
                    }
                }
            }
            drawn
        })
        .collect::<Vec<_>>();
    let joins = source
        .barriers
        .iter()
        .filter_map(|b| {
            let waits = b
                .waits_for
                .iter()
                .filter(|k| keys.contains(k.as_str()))
                .collect::<Vec<_>>();
            let releases = b
                .releases
                .iter()
                .filter(|k| keys.contains(k.as_str()))
                .collect::<Vec<_>>();
            (!waits.is_empty() && !releases.is_empty())
                .then(|| json!({"key": b.key, "waitsFor": waits, "releases": releases}))
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"workflowKey": source.workflow_key, "rows": rows, "units": units, "joins": joins, "cut": cut}),
    )
}

/// The set a clip names: the song's file under the project root, its source and the revision it was rendered at.
pub(crate) fn set_of(entry: &str, loaded: &AuthoredWorkflow) -> Value {
    let source = &loaded.source.source;
    let basis = source
        .authoring
        .as_ref()
        .expect("a loaded song is authored");
    json!({"entry": entry, "sourceRef": source.reference, "sourceRevision": source.revision,
        "workflowKey": loaded.source.workflow_key, "byteRevision": basis.revision, "semanticDigest": source.digest})
}

fn entry_of(root: &Path, entry: &str) -> Result<String, Diagnostic> {
    let path = Path::new(entry);
    let relative = if path.is_absolute() {
        let canonical = path.canonicalize().map_err(|e| {
            failure(
                "workflow.song_unplaced",
                format!("The song's file {entry} cannot be read ({e})."),
            )
        })?;
        let root = root
            .canonicalize()
            .map_err(|e| failure("workflow.song_unplaced", e.to_string()))?;
        canonical
            .strip_prefix(&root)
            .map_err(|_| failure("workflow.song_unplaced", format!("{entry} does not stand under the project root {}: a song is exported from the project that holds it.", root.display())))?
            .to_string_lossy()
            .into_owned()
    } else {
        entry.to_owned()
    };
    if !root.join(&relative).is_file() {
        return Err(failure(
            "workflow.song_unplaced",
            format!("No song's file stands at {relative} under the project root."),
        ));
    }
    Ok(relative)
}

fn write_new(path: &Path, clip: &Value) -> Result<(), Diagnostic> {
    let dir = path.parent().expect("a clip's path has a directory");
    fs::create_dir_all(dir).map_err(|e| failure("workflow.song_write", e.to_string()))?;
    let bytes = serde_json::to_vec_pretty(clip)
        .map_err(|e| failure("workflow.song_write", e.to_string()))?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(failure(
                "workflow.song_rendered",
                format!(
                    "A clip named {} already stands in this project's songs. {NEVER_IN_PLACE}",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .trim_end_matches(SUFFIX)
                ),
            ))
        }
        Err(e) => return Err(failure("workflow.song_write", e.to_string())),
    };
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|e| {
            let _ = fs::remove_file(path);
            failure("workflow.song_write", e.to_string())
        })
}

/// Replace a clip in one rename (Remake): the only way a clip's bytes change.
fn write_over(path: &Path, clip: &Value) -> Result<(), Diagnostic> {
    let bytes = serde_json::to_vec_pretty(clip)
        .map_err(|e| failure("workflow.song_write", e.to_string()))?;
    let temp = path.with_extension(format!("remake-{}", std::process::id()));
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    written.map_err(|e| {
        let _ = fs::remove_file(&temp);
        failure("workflow.song_write", e.to_string())
    })
}

fn listed(values: &Option<Vec<String>>) -> Value {
    values.as_ref().map_or(Value::Null, |v| json!(v))
}

/// `factory workflow export <state> <song.workflow.ts> [--name <name>] [--rows <ref,…>] [--units <key,…>]`.
pub fn export(
    state_path: &Path,
    entry: &str,
    name: Option<&str>,
    selection: Selection,
) -> Result<Value, Diagnostic> {
    let project = placed(state_path)?;
    let entry = entry_of(&project.root, entry)?;
    let loaded = load_workflow(&project.root, Path::new(&entry))?;
    let name = name.map_or_else(|| loaded.source.workflow_key.clone(), str::to_owned);
    valid_name(&name)?;
    let score = render_score(&project.root, &project.key, &loaded, &selection, DEPTH)?;
    let path = clip_path(&project.root, &name);
    let clip = json!({
        "contract": CLIP_CONTRACT,
        "clipRef": clip_ref(&project.key, &name),
        "name": name,
        "project": {"projectKey": project.key, "projectRef": project.reference},
        "set": set_of(&entry, &loaded),
        "selection": {"rows": listed(&selection.rows), "units": listed(&selection.units)},
        "score": score,
        "render": {"by": "factory workflow export", "renders": 1, "previousByteRevision": null},
        "importedFrom": null,
        "execution": "not-requested",
    });
    write_new(&path, &clip)?;
    Ok(json!({"contract": CLIP_CONTRACT, "path": path, "clip": clip, "execution": "not-requested"}))
}

/// One clip of this project, as written.
pub(crate) fn read_clip(root: &Path, name: &str) -> Result<Value, Diagnostic> {
    valid_name(name)?;
    let path = clip_path(root, name);
    read_clip_at(&path)
}

pub(crate) fn read_clip_at(path: &Path) -> Result<Value, Diagnostic> {
    let meta = fs::symlink_metadata(path).map_err(|_| {
        failure(
            "workflow.song_missing",
            format!("No clip stands at {}.", path.display()),
        )
    })?;
    if !meta.is_file() || meta.len() > 2 * 1024 * 1024 {
        return Err(failure(
            "workflow.song_missing",
            format!(
                "{} is not a clip Factory reads (a regular file of at most 2 MiB).",
                path.display()
            ),
        ));
    }
    let clip: Value = serde_json::from_slice(
        &fs::read(path).map_err(|e| failure("workflow.song_missing", e.to_string()))?,
    )
    .map_err(|e| {
        failure(
            "workflow.song_invalid",
            format!("{} is not a song's clip ({e}).", path.display()),
        )
    })?;
    if clip["contract"] != CLIP_CONTRACT {
        return Err(failure(
            "workflow.song_invalid",
            format!("{} is not a {CLIP_CONTRACT} document.", path.display()),
        ));
    }
    Ok(clip)
}

fn selection_of(clip: &Value) -> Selection {
    let list = |v: &Value| {
        v.as_array().map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
    };
    Selection {
        rows: list(&clip["selection"]["rows"]),
        units: list(&clip["selection"]["units"]),
    }
}

/// The set a clip names, as it now stands: loaded, or why it cannot be.
pub(crate) fn set_now(root: &Path, clip: &Value) -> Result<AuthoredWorkflow, (String, String)> {
    let entry = clip["set"]["entry"].as_str().unwrap_or_default();
    if entry.is_empty() || !root.join(entry).is_file() {
        return Err((
            "missing".into(),
            format!("The set the clip names ({entry}) no longer stands under the project root."),
        ));
    }
    let loaded = load_workflow(root, Path::new(entry)).map_err(|d| {
        (
            "does-not-compile".to_owned(),
            format!(
                "The set the clip names ({entry}) does not compile now: {}",
                d.message
            ),
        )
    })?;
    if loaded.source.source.reference.to_string()
        != clip["set"]["sourceRef"].as_str().unwrap_or_default()
    {
        return Err(("another-song".into(), format!("The file at {entry} is another song now ({}), not the set the clip was made from ({}).",
            loaded.source.source.reference, clip["set"]["sourceRef"].as_str().unwrap_or_default())));
    }
    Ok(loaded)
}

/// `factory workflow remake <state> <name>`: the clip made again from its set as it now stands, with the same rows and range.
pub fn remake(state_path: &Path, name: &str) -> Result<Value, Diagnostic> {
    let project = placed(state_path)?;
    let before = read_clip(&project.root, name)?;
    if let Some(from) = before["importedFrom"]["clipRef"].as_str() {
        return Err(failure("workflow.song_import", format!(
            "{name} is an import of {from}, and an import keeps where it came from: Remake it in the project that made it, then drop it here again.")));
    }
    let loaded = set_now(&project.root, &before)
        .map_err(|(_, why)| failure("workflow.song_set", format!("{why} Nothing is remade.")))?;
    let entry = before["set"]["entry"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let selection = selection_of(&before);
    let score = render_score(&project.root, &project.key, &loaded, &selection, DEPTH)
        .map_err(|d| failure(&d.code, format!("{} Nothing is remade.", d.message)))?;
    let renders = before["render"]["renders"].as_u64().unwrap_or(1) + 1;
    let mut clip = before.clone();
    clip["set"] = set_of(&entry, &loaded);
    clip["score"] = score;
    clip["render"] = json!({"by": "factory workflow remake", "renders": renders, "previousByteRevision": before["set"]["byteRevision"]});
    let path = clip_path(&project.root, name);
    write_over(&path, &clip)?;
    Ok(
        json!({"contract": CLIP_CONTRACT, "path": path, "clip": clip, "moved": before["set"]["byteRevision"] != clip["set"]["byteRevision"],
        "execution": "not-requested"}),
    )
}

/// `factory workflow songs <state>`: the project's clips, each with whether its set has moved since it was rendered.
pub fn songs(state_path: &Path) -> Result<Value, Diagnostic> {
    let project = placed(state_path)?;
    let dir = songs_dir(&project.root);
    let mut names = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(SUFFIX))
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>(),
        Err(_) => vec![],
    };
    names.sort();
    let songs = names
        .iter()
        .map(|name| {
            let path = clip_path(&project.root, name);
            match read_clip(&project.root, name) {
                Err(d) => json!({"name": name, "path": path, "standing": "unreadable", "why": d.message}),
                Ok(clip) => {
                    let (standing, why) = if clip["importedFrom"].is_object() {
                        ("imported", format!("An import of {}: it keeps where it came from.",
                            clip["importedFrom"]["clipRef"].as_str().unwrap_or_default()))
                    } else {
                        match set_now(&project.root, &clip) {
                            Err((standing, why)) => (if standing == "missing" { "missing" } else if standing == "another-song" { "another-song" } else { "does-not-compile" }, why),
                            Ok(loaded) => {
                                let now = set_of(clip["set"]["entry"].as_str().unwrap_or_default(), &loaded);
                                if now["byteRevision"] == clip["set"]["byteRevision"] {
                                    ("current", "The set stands as it was when the clip was rendered.".to_owned())
                                } else {
                                    ("moved", format!("The set has moved since the clip was rendered (it is {} now, the clip was made at {}): Remake makes it again from the set as it now stands.",
                                        now["byteRevision"].as_str().unwrap_or_default(), clip["set"]["byteRevision"].as_str().unwrap_or_default()))
                                }
                            }
                        }
                    };
                    json!({"name": name, "path": path, "clipRef": clip["clipRef"], "set": clip["set"], "selection": clip["selection"],
                        "units": clip["score"]["units"].as_array().map_or(0, Vec::len), "rows": clip["score"]["rows"].as_array().map_or(0, Vec::len),
                        "importedFrom": clip["importedFrom"], "render": clip["render"], "standing": standing, "moved": standing == "moved", "why": why,
                        "clip": clip})
                }
            }
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"contract": CLIPS_CONTRACT, "projectKey": project.key, "projectRef": project.reference, "projectRoot": project.root,
        "songs": songs, "execution": "not-requested"}),
    )
}

/// A clip dropped into a song of this project: this project's own clip as it stands, or another project's, imported — a copy kept
/// in this project's songs under the receiving ref, which keeps where it came from. The import is returned unwritten, so the drop
/// writes it only once the receiving song admits the unit.
pub(crate) struct Dropped {
    pub clip: Value,
    pub reference: String,
    pub import: Option<(PathBuf, Value)>,
}

pub(crate) fn dropped(root: &Path, project_key: &str, path: &Path) -> Result<Dropped, Diagnostic> {
    let clip = read_clip_at(path)?;
    let name = clip["name"].as_str().unwrap_or_default().to_owned();
    valid_name(&name)?;
    let here = clip_path(root, &name);
    let same = |a: &Path, b: &Path| {
        a.canonicalize()
            .ok()
            .is_some_and(|x| b.canonicalize().ok().is_some_and(|y| x == y))
    };
    if same(path, &here) {
        let reference = clip["clipRef"].as_str().unwrap_or_default().to_owned();
        return Ok(Dropped {
            clip,
            reference,
            import: None,
        });
    }
    let reference = clip_ref(project_key, &name);
    if here.exists() {
        let kept = read_clip_at(&here)?;
        let from = kept["importedFrom"]["clipRef"].as_str().unwrap_or_default();
        if from == clip["clipRef"].as_str().unwrap_or_default()
            && kept["set"] == clip["set"]
            && kept["score"] == clip["score"]
        {
            // The same render was imported before: the drop uses it.
            return Ok(Dropped {
                clip: kept,
                reference,
                import: None,
            });
        }
        return Err(failure("workflow.song_rendered", format!(
            "This project already keeps a clip named {name}, which is not this render of {}. {NEVER_IN_PLACE}",
            clip["clipRef"].as_str().unwrap_or_default())));
    }
    let mut import = clip.clone();
    import["clipRef"] = json!(reference);
    import["importedFrom"] =
        json!({"clipRef": clip["clipRef"], "path": path, "project": clip["project"]});
    Ok(Dropped {
        clip: import.clone(),
        reference,
        import: Some((here, import)),
    })
}

pub(crate) fn write_import(path: &Path, clip: &Value) -> Result<(), Diagnostic> {
    write_new(path, clip)
}

/// The receiving song's unit for a dropped clip: one unit with the thread form canon, named by the clip's ref. Its words are the
/// song's own — what its last units must make true and return, everything its units carry and may do — and it stops when the song
/// returns. Allocated to the track it was dropped on.
pub(crate) fn canon_unit(
    dropped: &Dropped,
    key: &str,
    to: (&str, &str),
    waits_on: &[String],
) -> Result<Value, Diagnostic> {
    let clip = &dropped.clip;
    let score = &clip["score"];
    let units = score["units"].as_array().cloned().unwrap_or_default();
    if units.is_empty() {
        return Err(failure(
            "workflow.song_invalid",
            "The clip holds no unit, so it is no song to drop.",
        ));
    }
    let waited = units
        .iter()
        .flat_map(|u| u["dependencies"].as_array().cloned().unwrap_or_default())
        .chain(
            score["joins"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .flat_map(|j| j["waitsFor"].as_array().cloned().unwrap_or_default()),
        )
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let last = units
        .iter()
        .filter(|u| !waited.contains(u["key"].as_str().unwrap_or_default()))
        .collect::<Vec<_>>();
    let words = |field: &str, from: &[&Value]| {
        let mut seen = BTreeSet::new();
        from.iter()
            .filter_map(|u| u[field].as_str())
            .filter(|w| seen.insert(w.to_owned()))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let every = units.iter().collect::<Vec<_>>();
    let union = |field: &str| {
        units
            .iter()
            .flat_map(|u| u[field].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let addresses = last
        .iter()
        .filter_map(|u| u["returnAddress"].as_str())
        .collect::<BTreeSet<_>>();
    let song = score["workflowKey"].as_str().unwrap_or_default();
    let name = clip["name"].as_str().unwrap_or_default();
    let (agents, sets) = match to.0 {
        "agent" => (vec![to.1], vec![]),
        "agent-set" => (vec![], vec![to.1]),
        other => {
            return Err(failure(
                "workflow.edit_invalid",
                format!("A dropped song is given to an agent or an agent set, not {other:?}"),
            ))
        }
    };
    Ok(json!({
        "key": key,
        "developmentalConcern": format!("Perform the song {song}, made into the clip {name}"),
        "requiredDifference": words("requiredDifference", &last),
        "returnContract": words("returns", &last),
        "subjectRef": dropped.reference,
        "basisRevision": clip["set"]["byteRevision"],
        "agentRequirements": {"agentRefs": agents, "agentSetRefs": sets, "agencyRefs": []},
        "praxisRefs": union("praxisRefs"),
        "capabilityRefs": union("capabilityRefs"),
        "dependencies": waits_on,
        "permittedEffects": union("may"),
        "verificationObligations": union("success"),
        // Where the song's last units return; where they return to several places, the first of them (a canon unit has one).
        "returnAddress": addresses.into_iter().next().unwrap_or_default(),
        "stopConditions": format!("Stop when the song {song} returns"),
        "escalationConditions": words("escalate", &every),
    }))
}

/// What the score reads of a unit that names a clip (`factory-song:` as its subject): the clip's score drawn inside it, the clip's
/// ref, and the thread form canon — or why the clip is not read. None for any other unit.
pub(crate) fn canon_of(
    root: Option<&Path>,
    project_key: Option<&str>,
    subject: &str,
) -> Option<BTreeMap<&'static str, Value>> {
    let (key, name) = parse_ref(subject)?;
    let mut out = BTreeMap::new();
    out.insert("form", json!("canon"));
    out.insert("songClip", json!(subject));
    match (root, project_key) {
        (Some(root), Some(here)) if here == key => match read_clip(root, name) {
            Ok(clip) => {
                out.insert("song", clip["score"].clone());
            }
            Err(d) => {
                out.insert("songWhy", json!(d.message));
            }
        },
        (Some(_), Some(_)) => {
            out.insert("songWhy", json!(format!("{subject} is another project's clip: drop it on a track here and this project keeps its own copy.")));
        }
        _ => {
            out.insert(
                "songWhy",
                json!("The song is not placed in a project, so no clip it names is read."),
            );
        }
    }
    Some(out)
}
