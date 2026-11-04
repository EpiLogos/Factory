//! A song run again, each run a take (O-I #844; DLW manual §19.1 Songs: "It comes round again by Loop, by Repeat, or by a routine's
//! schedule. The schedule is the routine's, and the song keeps none"; §19.1 The Launch panel: "each run a take of its own").
//!
//! `factory workflow perform <state> <run-ref> <unit>` starts a Run of the song a canon unit names: the unit a clip became when it
//! was dropped on a track (song_clip.rs `canon_unit`), in the song the Run performs, as the score reads it. The take performs the set
//! the clip names as it was rendered — read in this project, or for an import in the project that made it — and is refused when that
//! set has moved, no longer compiles or is another song now (a render is never edited in place: Remake makes it again). A clip that
//! renders only chosen rows or units is part of a song, and a Run performs a whole one.
//!
//! A song's edition runs once in a project's developmental state (a workflow source identity is attached once, its units' identities
//! once), so each take performs an edition of its own: the set's own modules, byte for byte, but for the source's identity — a ref
//! Factory mints for the take — and `successorOf`, which names the set it was made from. Nothing else of the song changes.
//!
//! The take is commissioned as the Run the canon unit stands in was — the same project, participants and root act — under a request
//! of its own, `<that Run's request>:take/<unit>/<n>`, so a canon unit's takes are the commissions its requests name, numbered in
//! order (`takes_of`, drawn on the unit in the score). Commission stamps the exact basis; nothing executes here: Factory's attempt
//! Actions start, fork and dispatch the take's units as they do any Run's.
use crate::commission::FactoryCommissionRequest;
use crate::core::run::RunRef;
use crate::developmental_read::FactoryDevelopmentalState;
use crate::workflow::WorkflowSource;
use crate::workflow_authoring::{error, AuthoredWorkflow, Diagnostic};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const TAKE_CONTRACT: &str = "factory.song-take/v1";
const NOTHING: &str = "Nothing is performed.";

fn refused(message: impl Into<String>) -> Diagnostic {
    error("workflow.song_take", message)
}

/// Where a canon unit's takes are commissioned: the request of the Run it stands in, the unit, then the take's number.
fn take_prefix(run_request: &str, unit: &str) -> String {
    format!("{run_request}:take/{unit}/")
}

/// A canon unit's takes: each Run commissioned from it by `perform`, numbered in order.
pub(crate) fn takes_of(state: &FactoryDevelopmentalState, run: &RunRef, unit: &str) -> Vec<Value> {
    let Some(of) = state.commissions.iter().find(|c| &c.run_ref == run) else {
        return vec![];
    };
    let prefix = take_prefix(&of.request.request_ref, unit);
    let mut takes = state
        .commissions
        .iter()
        .filter_map(|c| {
            let n = c
                .request
                .request_ref
                .strip_prefix(&prefix)?
                .parse::<u64>()
                .ok()?;
            Some((n, c.run_ref.to_string()))
        })
        .collect::<Vec<_>>();
    takes.sort();
    takes
        .into_iter()
        .map(|(take, run_ref)| json!({"take": take, "runRef": run_ref}))
        .collect()
}

/// The project that made an imported clip: the root its `.factory/songs/<name>.song.json` stands under.
fn origin_root(path: &Path, name: &str) -> Result<PathBuf, Diagnostic> {
    let gone = || {
        error(
            "workflow.song_set",
            format!(
                "{name} is an import of a clip that no longer stands at {}, so the set it names is not read. {NOTHING}",
                path.display()
            ),
        )
    };
    let canonical = path.canonicalize().map_err(|_| gone())?;
    let songs = canonical
        .parent()
        .filter(|d| d.file_name().is_some_and(|n| n == "songs"))
        .ok_or_else(gone)?;
    let factory = songs
        .parent()
        .filter(|d| d.file_name().is_some_and(|n| n == ".factory"))
        .ok_or_else(gone)?;
    factory.parent().map(Path::to_path_buf).ok_or_else(gone)
}

/// The take's edition: the set's entry module with its source's `ref` replaced by the take's own and `successorOf` naming the set
/// (in place of any predecessor the set names), compiled apart from the project from the same modules.
fn take_edition(set: &AuthoredWorkflow, request_ref: &str) -> Result<AuthoredWorkflow, Diagnostic> {
    let source = &set.source.source;
    let basis = source
        .authoring
        .as_ref()
        .expect("a loaded song is authored");
    let unwritable = || {
        refused(format!(
            "The song's source ref is not written in its own file ({}), so Factory cannot make a take of it. {NOTHING}",
            basis.entry
        ))
    };
    let text = &basis
        .modules
        .get(&basis.entry)
        .ok_or_else(unwritable)?
        .content;
    let at = basis
        .locations
        .get("/source/ref")
        .filter(|at| at.file == basis.entry)
        .ok_or_else(unwritable)?
        .byte_offset;
    let end = crate::workflow_score::literal_end(text, at).ok_or_else(unwritable)?;
    let mut hasher = blake3::Hasher::new();
    for part in [
        source.reference.to_string().as_str(),
        &source.revision,
        &source.digest,
        request_ref,
    ] {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    let mut id = [0u8; 16];
    id.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    let take_ref = format!("workflow-source:{}", ulid::Ulid::from_bytes(id));
    let named =
        json!({"ref": source.reference, "revision": source.revision, "digest": source.digest})
            .to_string();
    let mut edited = text.clone();
    match basis
        .locations
        .get("/source/successorOf")
        .filter(|at| at.file == basis.entry)
    {
        // The set names a predecessor of its own: the take names the set instead (spliced from the end, so `at` holds).
        Some(prior) => {
            let prior_end = crate::workflow_score::literal_end(text, prior.byte_offset)
                .ok_or_else(unwritable)?;
            let (first, second) = if prior.byte_offset > at {
                (
                    (prior.byte_offset, prior_end, named.clone()),
                    (at, end, json!(take_ref).to_string()),
                )
            } else {
                (
                    (at, end, json!(take_ref).to_string()),
                    (prior.byte_offset, prior_end, named.clone()),
                )
            };
            edited.replace_range(first.0..first.1, &first.2);
            edited.replace_range(second.0..second.1, &second.2);
        }
        None => edited.replace_range(
            at..end,
            &format!("{}, successorOf: {named}", json!(take_ref)),
        ),
    }
    crate::workflow_score::compile_modules_apart(basis, &basis.entry, &edited)
}

/// A take ready to commission: the request, the song's source, and what the receipt names of where it came from.
pub struct Take {
    pub request: FactoryCommissionRequest,
    pub source: WorkflowSource,
    pub take: u64,
    pub clip_ref: Value,
    pub clip: String,
    pub set: Value,
}

/// The take of `unit` in the song the Run performs: its clip read, its set checked as rendered, and its commission composed from
/// the Run's own. Refused, before anything is written, in Factory's words.
pub fn prepare(state_path: &Path, run: &RunRef, unit: &str) -> Result<Take, Diagnostic> {
    let (state, _, retained) = crate::workflow_score::retained_source(state_path, run)?;
    // The song as the score reads it: its file while it is the same song, else the edition the Run performs.
    let song = crate::workflow_score::song(state_path, &retained).ok();
    let source = song.as_ref().map_or(&retained, |s| &s.loaded.source);
    let chosen = source.units.iter().find(|u| u.key == unit).ok_or_else(|| {
        refused(format!(
            "{unit} is no unit of the song this Run performs. {NOTHING}"
        ))
    })?;
    let subject = chosen.subject_ref.as_str();
    let Some((key, name)) = crate::song_clip::parse_ref(subject) else {
        return Err(refused(format!(
            "{unit} is a task, not a song: only a unit with the thread form canon (a song's clip dropped on a track) runs a song again. {NOTHING}"
        )));
    };
    let project =
        crate::song_clip::project(state_path).map_err(|why| refused(format!("{why} {NOTHING}")))?;
    if key != project.key {
        return Err(refused(format!(
            "{subject} is another project's clip, which this project does not keep: drop it on a track here and this project keeps its own copy. {NOTHING}"
        )));
    }
    let clip = crate::song_clip::read_clip(&project.root, name)?;
    let song_key = clip["set"]["workflowKey"]
        .as_str()
        .unwrap_or(name)
        .to_owned();
    if !clip["selection"]["rows"].is_null() || !clip["selection"]["units"].is_null() {
        return Err(refused(format!(
            "{name} renders chosen rows or units of the song {song_key}, not the whole song, and a Run performs a whole song: export the whole song as a clip and drop that to run it again. {NOTHING}"
        )));
    }
    let imported = clip["importedFrom"]["path"].as_str();
    let (root, remake) = match imported {
        Some(path) => (
            origin_root(Path::new(path), name)?,
            "Remake it in the project that made it, then drop it here again.",
        ),
        None => (
            project.root.clone(),
            "Remake makes it again from the set as it now stands.",
        ),
    };
    let loaded = crate::song_clip::set_now(&root, &clip)
        .map_err(|(_, why)| error("workflow.song_set", format!("{why} {NOTHING}")))?;
    let entry = clip["set"]["entry"].as_str().unwrap_or_default();
    let now = crate::song_clip::set_of(entry, &loaded);
    if now["byteRevision"] != clip["set"]["byteRevision"] {
        return Err(error("workflow.song_set", format!(
            "The set {name} names has moved since it was rendered (it is {} now, the clip was made at {}), and a take performs the song as the clip renders it. {remake} {NOTHING}",
            now["byteRevision"].as_str().unwrap_or_default(), clip["set"]["byteRevision"].as_str().unwrap_or_default())));
    }
    let of = state
        .commissions
        .iter()
        .find(|c| &c.run_ref == run)
        .ok_or_else(|| {
            refused(format!(
                "{run} was not commissioned by Factory, so there is no commission for the song's takes to follow. {NOTHING}"
            ))
        })?;
    let take = takes_of(&state, run, unit)
        .last()
        .and_then(|t| t["take"].as_u64())
        .map_or(1, |n| n + 1);
    let mut request = of.request.clone();
    request.request_ref = format!("{}{take}", take_prefix(&of.request.request_ref, unit));
    let edition = take_edition(&loaded, &request.request_ref)?;
    request.purpose =
        format!("Perform the song {song_key}, made into the clip {name}: take {take} of {unit}");
    request.frontier = format!(
        "The song as the clip {name} renders it ({} at {}); no execution has been requested",
        clip["set"]["sourceRef"].as_str().unwrap_or_default(),
        clip["set"]["byteRevision"].as_str().unwrap_or_default()
    );
    let moment: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    request.commissioned_at = moment.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    request.root_act.act_ref = format!("{}:take/{unit}/{take}", of.request.root_act.act_ref);
    request.root_act.purpose = format!("Perform the song {song_key} again");
    Ok(Take {
        request,
        source: edition.source,
        take,
        clip_ref: clip["clipRef"].clone(),
        clip: name.to_owned(),
        set: clip["set"].clone(),
    })
}
