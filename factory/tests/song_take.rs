//! O-I #844's open line: a song runs again, each run a take (DLW manual §19.1 Songs: "It comes round again by Loop, by Repeat, or
//! by a routine's schedule. The schedule is the routine's, and the song keeps none"; §19.1 The Launch panel: "each run a take of its
//! own").
//!
//! Proves, through Factory's public CLI on placed scratch projects: `factory workflow perform <state> <run-ref> <unit>` starts a Run
//! of the song a canon unit names — the clip's set as it was rendered, read where the clip was made — commissioned as the Run the
//! canon unit stands in was, under a request of its own; each perform is one take, numbered, and the score lists a canon unit's
//! takes. A unit that is no song, a clip that renders only chosen rows or units, and a set that has moved since the clip was
//! rendered are refused in Factory's words, and nothing is commissioned.
use epilogos_factory::developmental_read::FactoryDevelopmentalFileProvider;
use epilogos_factory::workflow_authoring::cli;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const SONG: &str = "development-and-knowledge.workflow.ts";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ql-vak-workflows")
        .join(name)
}

fn run(args: &[&str]) -> Result<Value, epilogos_factory::workflow_authoring::Diagnostic> {
    cli::execute(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
}

struct Placed {
    _dir: tempfile::TempDir,
    root: PathBuf,
    state: String,
}

fn placed(key: &str) -> Placed {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(key);
    fs::create_dir(&root).unwrap();
    let setup = epilogos_factory::project_setup::setup(&root, key, None).unwrap();
    fs::copy(fixture(SONG), root.join(SONG)).unwrap();
    Placed {
        state: setup["statePath"].as_str().unwrap().to_owned(),
        root,
        _dir: dir,
    }
}

/// A placed project whose song a Run performs: the song a clip is dropped into.
struct Performing {
    placed: Placed,
    run: String,
}

fn performing(key: &str) -> Performing {
    let placed = placed(key);
    let request = placed.root.join("..").join("commission.json");
    let participants = [
        "agent/anima-logos",
        "agent/anima-techne-helper",
        "agent/anima-eros",
        "agent/anima-mythos",
        "agent/anima-sophia",
        "agent/aletheia",
    ]
    .iter()
    .map(|r| {
        json!({"ref": r, "description": "A voice the song names", "sourceOwner": "central",
        "sourceRef": "source:specimen", "sourceRevision": "specimen-r1"})
    })
    .collect::<Vec<_>>();
    fs::write(&request, serde_json::to_vec_pretty(&json!({
        "contract": "factory.commission-request/v1", "requestRef": format!("commission-request:{key}"), "projectKey": key,
        "purpose": "Admit the song that receives a dropped clip", "frontier": "Declared source; no execution has been requested",
        "runDestination": format!("{key}/first"), "writeOwner": "factory", "commissionedAt": "2026-11-03T00:00:00Z",
        "participantRequirements": participants,
        "rootAct": {"actRef": format!("act:{key}"), "agentRef": "agent/anima-logos", "purpose": "Perform the song",
            "scopeRefs": ["project:quaternal-logic"], "standing": "commissioned-not-executed"}
    })).unwrap()).unwrap();
    let root = placed.root.to_string_lossy().into_owned();
    let receipt = run(&[
        "workflow",
        "commission",
        &placed.state,
        &request.to_string_lossy(),
        SONG,
        "--root",
        &root,
    ])
    .unwrap();
    let run_ref = receipt["commission"]["commission"]["runRef"]
        .as_str()
        .unwrap()
        .to_owned();
    Performing {
        placed,
        run: run_ref,
    }
}

fn score(state: &str, run_ref: &str) -> Value {
    run(&["workflow", "score", state, run_ref, "--json"]).unwrap()
}

fn unit<'a>(score: &'a Value, key: &str) -> &'a Value {
    score["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["key"] == key)
        .unwrap_or_else(|| panic!("no unit {key}"))
}

/// The clip dropped on the track of `to`, as the shell's drop writes it.
fn drop_clip(p: &Performing, clip: &Path, to: &str, waits_on: &[&str]) -> String {
    let revision = score(&p.placed.state, &p.run)["song"]["file"]["byteRevision"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = p.placed.root.join("..").join("edit.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision,
            "edits": [{"op": "canon", "clip": clip.to_string_lossy(), "to": {"kind": "agent", "ref": to}, "waitsOn": waits_on}]}))
        .unwrap(),
    )
    .unwrap();
    let receipt = run(&[
        "workflow",
        "edit",
        &p.placed.state,
        &p.run,
        &path.to_string_lossy(),
        "--json",
    ])
    .unwrap();
    fs::remove_file(&path).unwrap();
    receipt["added"][0]["key"].as_str().unwrap().to_owned()
}

fn export(p: &Placed, name: &str, rows: Option<&str>) -> PathBuf {
    let mut args = vec!["workflow", "export", &p.state, SONG, "--name", name];
    if let Some(rows) = rows {
        args.extend(["--rows", rows]);
    }
    args.push("--json");
    PathBuf::from(run(&args).unwrap()["path"].as_str().unwrap())
}

fn perform(
    p: &Performing,
    unit: &str,
) -> Result<Value, epilogos_factory::workflow_authoring::Diagnostic> {
    run(&[
        "workflow",
        "perform",
        &p.placed.state,
        &p.run,
        unit,
        "--json",
    ])
}

fn commissions(state: &str) -> usize {
    FactoryDevelopmentalFileProvider::open(state)
        .unwrap()
        .state()
        .commissions
        .len()
}

#[test]
fn a_canon_unit_performs_its_song_again_each_run_a_take_commissioned_as_its_run_was() {
    let maker = placed("song-maker");
    let clip = export(&maker, "release", None);
    let made: Value = serde_json::from_slice(&fs::read(&clip).unwrap()).unwrap();
    let taker = performing("song-taker");
    let key = drop_clip(&taker, &clip, "agent/anima-eros", &["eros-verify"]);
    assert_eq!(key, "release");
    assert_eq!(commissions(&taker.placed.state), 1);

    let first = perform(&taker, "release").unwrap();
    assert_eq!(first["contract"], "factory.song-take/v1");
    assert_eq!(first["take"], 1);
    assert_eq!(first["unit"], "release");
    assert_eq!(first["of"], taker.run.as_str());
    assert_eq!(first["clipRef"], "factory-song:song-taker/release");
    assert_eq!(first["execution"], "not-requested");
    assert_eq!(first["nextCommand"], "factory attempt action");
    // The take performs the set the clip names, as it was rendered: the maker's song at the byte revision of the render, in an
    // edition of its own (a song's edition runs once in a project), which names the set it was made from.
    assert_eq!(first["set"], made["set"]);
    assert_ne!(first["source"]["ref"], made["set"]["sourceRef"]);
    assert_eq!(
        first["source"]["authoring"]["successorOf"],
        json!({"ref": made["set"]["sourceRef"], "revision": made["set"]["sourceRevision"], "digest": made["set"]["semanticDigest"]})
    );
    assert_eq!(first["source"]["authoring"]["entry"], SONG);
    let take_run = first["commission"]["commission"]["runRef"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(take_run, taker.run, "a take is a Run of its own");
    // Commissioned as the Run the canon unit stands in was: the same project, participants and root act's agent, under a request of
    // its own that names the take.
    let request = &first["commission"]["commission"]["request"];
    assert_eq!(request["projectKey"], "song-taker");
    assert_eq!(
        request["requestRef"],
        "commission-request:song-taker:take/release/1"
    );
    assert_eq!(request["rootAct"]["agentRef"], "agent/anima-logos");
    assert_eq!(
        request["participantRequirements"].as_array().unwrap().len(),
        6
    );
    assert!(
        request["purpose"]
            .as_str()
            .unwrap()
            .contains("development-and-knowledge"),
        "{request}"
    );
    assert_eq!(commissions(&taker.placed.state), 2);
    // The take's Run holds the song's attempts, ready for Factory's attempt Actions.
    assert_eq!(
        first["attempts"]["runRef"], take_run,
        "{}",
        first["attempts"]
    );

    // The take's Run performs the song whole: its score is the clip's song, unit for unit.
    let performed = score(&taker.placed.state, &take_run);
    assert_eq!(
        performed["song"]["workflowKey"],
        "development-and-knowledge"
    );
    let keys = |units: &Value| {
        units
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["key"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&performed["units"]), keys(&made["score"]["units"]));

    // Again: another take, another Run.
    let second = perform(&taker, "release").unwrap();
    assert_eq!(second["take"], 2);
    let second_run = second["commission"]["commission"]["runRef"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(second_run, take_run);
    assert_ne!(
        second["source"]["ref"], first["source"]["ref"],
        "each take an edition of its own"
    );
    assert_eq!(
        second["source"]["authoring"]["successorOf"],
        first["source"]["authoring"]["successorOf"]
    );
    assert_eq!(commissions(&taker.placed.state), 3);

    // The score lists the canon unit's takes, in order; a unit that is no song has none.
    let read = score(&taker.placed.state, &taker.run);
    assert_eq!(
        unit(&read, "release")["takes"],
        json!([{"take": 1, "runRef": take_run}, {"take": 2, "runRef": second_run}])
    );
    assert!(unit(&read, "logos-scope-plan").get("takes").is_none());
}

#[test]
fn a_unit_that_is_no_whole_song_or_whose_set_has_moved_is_refused_and_nothing_is_commissioned() {
    let maker = placed("song-maker");
    let whole = export(&maker, "release", None);
    let part = export(&maker, "eros-part", Some("agent/anima-eros"));
    let taker = performing("song-taker");
    drop_clip(&taker, &whole, "agent/anima-eros", &["eros-verify"]);
    drop_clip(&taker, &part, "agent/anima-mythos", &[]);
    let before = commissions(&taker.placed.state);

    // A task, not a song.
    let task = perform(&taker, "logos-scope-plan").unwrap_err();
    assert_eq!(task.code, "workflow.song_take");
    assert!(task.message.contains("not a song"), "{}", task.message);
    // No unit of the song.
    let none = perform(&taker, "no-such-unit").unwrap_err();
    assert_eq!(none.code, "workflow.song_take");
    assert!(none.message.contains("no-such-unit"), "{}", none.message);
    // A clip of chosen rows is part of a song: a Run performs a whole one.
    let partial = perform(&taker, "eros-part").unwrap_err();
    assert_eq!(partial.code, "workflow.song_take");
    assert!(
        partial.message.contains("chosen rows or units"),
        "{}",
        partial.message
    );
    // The set moved since the clip was rendered: the take would not be what the clip renders.
    let file = maker.root.join(SONG);
    let mut text = fs::read_to_string(&file).unwrap();
    text.push_str("\n// moved after the render\n");
    fs::write(&file, text).unwrap();
    let moved = perform(&taker, "release").unwrap_err();
    assert_eq!(moved.code, "workflow.song_set");
    assert!(
        moved.message.contains("has moved") && moved.message.contains("Nothing is performed"),
        "{}",
        moved.message
    );
    assert_eq!(commissions(&taker.placed.state), before);
}

/// The maker's song, its source naming a predecessor through `prior` — a const in the song's own file, or one imported from
/// another module of the set.
fn with_prior(maker: &Placed, imported: bool) {
    let file = maker.root.join(SONG);
    let prior = "{ ref: \"workflow-source:01M399B6B0R8CJY3Y2RTG5EPWT\", revision: \"development-and-knowledge-v0\", digest: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\" }";
    let mut text = fs::read_to_string(&file).unwrap().replacen(
        "    revision: \"development-and-knowledge-v1\",\n",
        "    revision: \"development-and-knowledge-v1\",\n    successorOf: prior,\n",
        1,
    );
    if imported {
        fs::write(
            maker.root.join("prior.ts"),
            format!("export const prior = {prior};\n"),
        )
        .unwrap();
        text = text.replacen("import { defineWorkflow, unit } from \"@epilogos/factory-workflow\";", "import { defineWorkflow, unit } from \"@epilogos/factory-workflow\";\nimport { prior } from \"./prior.ts\";", 1);
    } else {
        text = text.replacen(
            "export default defineWorkflow({",
            &format!("const prior = {prior};\n\nexport default defineWorkflow({{"),
            1,
        );
    }
    assert!(text.contains("successorOf: prior"));
    fs::write(&file, text).unwrap();
}

#[test]
fn two_performs_of_one_canon_unit_at_the_same_moment_never_share_a_take() {
    // Both are prepared from the same state, so both would be take 1: the take is numbered under the developmental lock, and
    // the one that comes second is refused in Factory's words, never commissioned as a second take 1.
    let maker = placed("song-twin-maker");
    let whole = export(&maker, "release", None);
    let taker = performing("song-twin-taker");
    let canon = drop_clip(&taker, &whole, "agent/anima-eros", &["eros-verify"]);
    let run: epilogos_factory::core::run::RunRef = taker.run.parse().unwrap();
    let state = Path::new(&taker.placed.state);
    let before = commissions(&taker.placed.state);
    let first = epilogos_factory::song_take::prepare(state, &run, &canon).unwrap();
    let second = epilogos_factory::song_take::prepare(state, &run, &canon).unwrap();
    assert_eq!(
        (first.take, second.take),
        (1, 1),
        "prepared together, both read take 1"
    );
    let made = epilogos_factory::song_take::commission(state, first).unwrap();
    assert_eq!(made["take"], 1);
    let refused = epilogos_factory::song_take::commission(state, second).unwrap_err();
    assert_eq!(refused.code, "workflow.song_take");
    assert!(
        refused
            .message
            .contains("take 1 of release was started by another perform")
            && refused.message.ends_with("Nothing is performed."),
        "{}",
        refused.message
    );
    assert_eq!(
        commissions(&taker.placed.state),
        before + 1,
        "one take 1, never two"
    );
    assert_eq!(
        perform(&taker, &canon).unwrap()["take"],
        2,
        "the next perform takes the next number"
    );
}

#[test]
fn a_set_that_names_its_predecessor_through_a_const_gets_a_take_that_names_the_set_it_was_made_from(
) {
    let maker = placed("song-const-maker");
    with_prior(&maker, false);
    let whole = export(&maker, "release", None);
    let taker = performing("song-const-taker");
    let canon = drop_clip(&taker, &whole, "agent/anima-eros", &["eros-verify"]);
    let take = perform(&taker, &canon).unwrap();
    let set = &take["set"];
    assert_eq!(
        take["source"]["authoring"]["successorOf"],
        json!({"ref": set["sourceRef"], "revision": set["sourceRevision"], "digest": set["semanticDigest"]}),
        "the take names the set it was made from, not the set's own predecessor"
    );
    assert_ne!(take["source"]["ref"], set["sourceRef"]);
}

#[test]
fn a_set_whose_predecessor_is_written_in_another_module_is_refused_saying_why_and_nothing_is_commissioned(
) {
    let maker = placed("song-import-maker");
    with_prior(&maker, true);
    let whole = export(&maker, "release", None);
    let taker = performing("song-import-taker");
    let canon = drop_clip(&taker, &whole, "agent/anima-eros", &["eros-verify"]);
    let before = commissions(&taker.placed.state);
    let refused = perform(&taker, &canon).unwrap_err();
    assert_eq!(refused.code, "workflow.song_take", "{}", refused.message);
    assert!(
        refused.message.contains("successorOf")
            && refused.message.contains("prior.ts")
            && refused.message.ends_with("Nothing is performed."),
        "{}",
        refused.message
    );
    assert_eq!(
        commissions(&taker.placed.state),
        before,
        "nothing is commissioned"
    );
}

#[test]
fn help_names_perform_and_its_contract() {
    let help = run(&["workflow", "help", "--json"]).unwrap();
    assert!(help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "workflow.perform"));
    assert!(help["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "factory.song-take/v1"));
    assert!(help["help"]
        .as_str()
        .unwrap()
        .contains("factory workflow perform <state> <run-ref> <unit>"));
}
