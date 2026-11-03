//! O-I #844: a song made into one clip, and the clip used whole in other work (DLW manual §19.1 Songs: "Export Song… … writes the
//! song, or the selected tracks and range, as one clip. The clip is a render: it names its set, Remake makes it again, and it is
//! never edited in place"; "Drop the clip on a track, in this project or another, and it becomes a task with the thread form canon";
//! §10.3: "A render is made from something else: it names what it was made from, it can be remade, and it is never edited in place.
//! … An import keeps where it came from").
//!
//! Proves, through Factory's public CLI on placed scratch projects: `factory workflow export` renders a song — whole, or the rows
//! and units chosen — once into one clip the project keeps, naming the set it was made from at the byte revision it was rendered at;
//! a second render of the same name is refused; `factory workflow songs` lists the project's clips and says when a clip's set has
//! moved; `factory workflow remake` makes the clip again from its set as it now stands. `factory workflow edit`'s `canon` op makes a
//! clip — this project's or another's — one unit of a receiving song, allocated to the track it was dropped on, in the song's own
//! words: the receiving project keeps an import of another project's clip, which keeps where it came from and is never remade
//! there; the score draws the clip's song inside the unit; a canon the song cannot admit leaves the song and the project as they
//! were.
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

/// A placed project holding the song at its root.
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

/// A placed project whose song a Run performs (the receiving song of a drop).
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

fn score(p: &Performing) -> Value {
    run(&["workflow", "score", &p.placed.state, &p.run, "--json"]).unwrap()
}

fn edit(
    p: &Performing,
    request: Value,
) -> Result<Value, epilogos_factory::workflow_authoring::Diagnostic> {
    let path = p.placed.root.join("..").join("edit.json");
    fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
    let result = run(&[
        "workflow",
        "edit",
        &p.placed.state,
        &p.run,
        &path.to_string_lossy(),
        "--json",
    ]);
    fs::remove_file(&path).unwrap();
    result
}

fn canon(revision: &str, clip: &Path, to: Value, waits_on: &[&str]) -> Value {
    json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision,
        "edits": [{"op": "canon", "clip": clip.to_string_lossy(), "to": to, "waitsOn": waits_on}]})
}

fn unit<'a>(score: &'a Value, key: &str) -> &'a Value {
    score["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["key"] == key)
        .unwrap_or_else(|| panic!("no unit {key}"))
}

fn songs_dir(p: &Placed) -> PathBuf {
    p.root.join(".factory/songs")
}

fn kept(p: &Placed) -> Vec<String> {
    let mut names = fs::read_dir(songs_dir(p))
        .map(|d| {
            d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn export_renders_the_song_once_and_remake_makes_it_again_from_its_set() {
    let p = placed("song-maker");
    let made = run(&["workflow", "export", &p.state, SONG, "--json"]).unwrap();
    assert_eq!(made["contract"], "factory.song-clip/v1");
    assert_eq!(made["execution"], "not-requested");
    let clip = &made["clip"];
    assert_eq!(
        clip["clipRef"],
        "factory-song:song-maker/development-and-knowledge"
    );
    assert_eq!(clip["name"], "development-and-knowledge");
    assert_eq!(clip["project"]["projectKey"], "song-maker");
    // The clip names its set: the song's file under the project root, its source and the byte revision it was rendered at.
    assert_eq!(clip["set"]["entry"], SONG);
    assert_eq!(clip["set"]["workflowKey"], "development-and-knowledge");
    assert!(clip["set"]["byteRevision"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));
    assert_eq!(clip["render"]["renders"], 1);
    assert_eq!(clip["importedFrom"], Value::Null);
    let units = clip["score"]["units"].as_array().unwrap();
    assert_eq!(units[0]["key"], "logos-scope-plan");
    assert_eq!(units[0]["row"], "agent/anima-logos");
    assert_eq!(units[0]["heads"]["CFP"], "CFP2");
    assert!(
        units
            .iter()
            .any(|u| !u["dependencies"].as_array().unwrap().is_empty()),
        "the units keep what they wait on"
    );
    assert_eq!(kept(&p), ["development-and-knowledge.song.json"]);
    let on_disk: Value = serde_json::from_slice(
        &fs::read(songs_dir(&p).join("development-and-knowledge.song.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(&on_disk, clip);

    // Written once: a second render of the same name is refused, and the clip stands as it was.
    let again = run(&["workflow", "export", &p.state, SONG, "--json"]).unwrap_err();
    assert_eq!(again.code, "workflow.song_rendered");
    assert!(
        again.message.contains("never edited in place"),
        "{}",
        again.message
    );

    // The project's songs: the clip, its set as it was.
    let listed = run(&["workflow", "songs", &p.state, "--json"]).unwrap();
    assert_eq!(listed["contract"], "factory.song-clips/v1");
    assert_eq!(listed["projectKey"], "song-maker");
    assert_eq!(listed["songs"][0]["standing"], "current");
    assert_eq!(listed["songs"][0]["units"], units.len());

    // The set moves: the clip says so, and Remake makes it again from the set as it now stands.
    let file = p.root.join(SONG);
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        text.replacen(
            "Scope the change and write the plan it will be built from",
            "Scope the change and plan it",
            1,
        ),
    )
    .unwrap();
    let moved = run(&["workflow", "songs", &p.state, "--json"]).unwrap();
    assert_eq!(moved["songs"][0]["standing"], "moved");
    assert_eq!(moved["songs"][0]["moved"], true);
    let remade = run(&[
        "workflow",
        "remake",
        &p.state,
        "development-and-knowledge",
        "--json",
    ])
    .unwrap();
    assert_eq!(remade["moved"], true);
    assert_eq!(remade["clip"]["render"]["renders"], 2);
    assert_eq!(
        remade["clip"]["render"]["previousByteRevision"],
        clip["set"]["byteRevision"]
    );
    assert_eq!(
        remade["clip"]["score"]["units"][0]["concern"],
        "Scope the change and plan it"
    );
    assert_eq!(
        remade["clip"]["clipRef"], clip["clipRef"],
        "the clip keeps its name and ref"
    );
    assert_eq!(
        run(&["workflow", "songs", &p.state, "--json"]).unwrap()["songs"][0]["standing"],
        "current"
    );
    assert_eq!(
        kept(&p),
        ["development-and-knowledge.song.json"],
        "no temporary file is left"
    );

    // The selected tracks and range: only those units, the ties the selection cut named.
    let part = run(&[
        "workflow",
        "export",
        &p.state,
        SONG,
        "--name",
        "eros-part",
        "--rows",
        "agent/anima-eros,agent/anima-mythos",
        "--json",
    ])
    .unwrap();
    let rows = part["clip"]["score"]["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["row"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(
        !rows.is_empty()
            && rows
                .iter()
                .all(|r| r == "agent/anima-eros" || r == "agent/anima-mythos"),
        "{rows:?}"
    );
    assert!(
        !part["clip"]["score"]["cut"].as_array().unwrap().is_empty(),
        "a tie to a unit left out is named, not kept"
    );
    assert_eq!(
        part["clip"]["selection"]["rows"],
        json!(["agent/anima-eros", "agent/anima-mythos"])
    );
    let none = run(&[
        "workflow",
        "export",
        &p.state,
        SONG,
        "--name",
        "nobody",
        "--rows",
        "agent/nobody",
        "--json",
    ])
    .unwrap_err();
    assert_eq!(none.code, "workflow.song_selection");
    assert!(none.message.contains("agent/nobody"), "{}", none.message);
    let bad = run(&[
        "workflow", "export", &p.state, SONG, "--name", "../out", "--json",
    ])
    .unwrap_err();
    assert_eq!(bad.code, "workflow.song_name");
    assert_eq!(
        kept(&p),
        ["development-and-knowledge.song.json", "eros-part.song.json"]
    );
}

#[test]
fn a_clip_dropped_on_another_projects_song_is_one_canon_unit_its_score_draws_whole() {
    let maker = placed("song-maker");
    let made = run(&[
        "workflow",
        "export",
        &maker.state,
        SONG,
        "--name",
        "release",
        "--json",
    ])
    .unwrap();
    let clip_path = PathBuf::from(made["path"].as_str().unwrap());
    let taker = performing("song-taker");
    let before = score(&taker);
    let revision = before["song"]["file"]["byteRevision"]
        .as_str()
        .unwrap()
        .to_owned();
    let file_before = fs::read_to_string(taker.placed.root.join(SONG)).unwrap();

    let receipt = edit(
        &taker,
        canon(
            &revision,
            &clip_path,
            json!({"kind": "agent", "ref": "agent/anima-eros"}),
            &["eros-verify"],
        ),
    )
    .unwrap();
    assert_eq!(receipt["contract"], "factory.workflow-edit-receipt/v1");
    assert_eq!(
        receipt["added"],
        json!([{"key": "release", "subjectRef": "factory-song:song-taker/release", "form": "canon"}])
    );
    let import = songs_dir(&taker.placed).join("release.song.json");
    let imports = receipt["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 1, "{receipt}");
    assert_eq!(
        PathBuf::from(imports[0].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        import.canonicalize().unwrap()
    );
    assert_ne!(receipt["revision"], revision);
    // The song's file gains one unit in its own form; the rest of the file stands.
    let file_after = fs::read_to_string(taker.placed.root.join(SONG)).unwrap();
    assert!(
        file_after.starts_with(&file_before[..file_before.find("units: [").unwrap()]),
        "the head of the file stands"
    );
    assert!(
        file_after.contains("unit({\n")
            && file_after.contains("\"subjectRef\": \"factory-song:song-taker/release\""),
        "{file_after}"
    );
    assert!(
        run(&[
            "workflow",
            "check",
            &taker.placed.root.join(SONG).to_string_lossy(),
            "--json"
        ])
        .is_ok(),
        "the song still compiles"
    );

    // The score draws it as one note on the track it was dropped on, the clip's song inside it.
    let after = score(&taker);
    let release = unit(&after, "release");
    assert_eq!(release["form"], "canon");
    assert_eq!(release["songClip"], "factory-song:song-taker/release");
    assert_eq!(release["row"], "agent/anima-eros");
    assert_eq!(release["dependencies"], json!(["eros-verify"]));
    assert_eq!(release["song"]["workflowKey"], "development-and-knowledge");
    assert_eq!(
        release["song"]["units"].as_array().unwrap().len(),
        made["clip"]["score"]["units"].as_array().unwrap().len()
    );
    assert!(
        release["concern"]
            .as_str()
            .unwrap()
            .contains("development-and-knowledge"),
        "{}",
        release["concern"]
    );
    assert_eq!(
        after["song"]["performed"], false,
        "the Run keeps the edition it was commissioned with"
    );
    assert!(
        unit(&after, "logos-scope-plan").get("form").is_none(),
        "only a unit that names a clip is a canon"
    );

    // The import keeps where it came from, and is never remade here.
    let listed = run(&["workflow", "songs", &taker.placed.state, "--json"]).unwrap();
    assert_eq!(listed["songs"][0]["name"], "release");
    assert_eq!(listed["songs"][0]["standing"], "imported");
    assert_eq!(
        listed["songs"][0]["importedFrom"]["clipRef"],
        "factory-song:song-maker/release"
    );
    let remake = run(&[
        "workflow",
        "remake",
        &taker.placed.state,
        "release",
        "--json",
    ])
    .unwrap_err();
    assert_eq!(remake.code, "workflow.song_import");

    // Dropped again, the same render is used, under a key of its own.
    let again = edit(
        &taker,
        canon(
            after["song"]["file"]["byteRevision"].as_str().unwrap(),
            &clip_path,
            json!({"kind": "agent-set", "ref": "agent-set/anima"}),
            &[],
        ),
    )
    .unwrap();
    assert_eq!(again["added"][0]["key"], "release-2");
    assert_eq!(again["imports"], json!([]));
    assert_eq!(unit(&score(&taker), "release-2")["row"], "agent-set/anima");

    // The receiving song made a clip of its own carries the dropped song inside its unit, canon within canon.
    let nested = run(&[
        "workflow",
        "export",
        &taker.placed.state,
        SONG,
        "--name",
        "with-release",
        "--json",
    ])
    .unwrap();
    let inner = nested["clip"]["score"]["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["key"] == "release")
        .unwrap();
    assert_eq!(inner["songClip"], "factory-song:song-taker/release");
    assert_eq!(inner["song"]["workflowKey"], "development-and-knowledge");
}

#[test]
fn a_canon_the_song_cannot_admit_leaves_the_song_and_the_project_as_they_were() {
    let maker = placed("song-maker");
    let made = run(&[
        "workflow",
        "export",
        &maker.state,
        SONG,
        "--name",
        "release",
        "--json",
    ])
    .unwrap();
    let clip_path = PathBuf::from(made["path"].as_str().unwrap());
    let taker = performing("song-taker");
    let revision = score(&taker)["song"]["file"]["byteRevision"]
        .as_str()
        .unwrap()
        .to_owned();
    let file = fs::read(taker.placed.root.join(SONG)).unwrap();
    let refused = |request: Value| {
        let d = edit(&taker, request).unwrap_err();
        assert_eq!(
            fs::read(taker.placed.root.join(SONG)).unwrap(),
            file,
            "the song stands"
        );
        assert!(kept(&taker.placed).is_empty(), "no import is kept");
        d
    };
    let agency = refused(canon(
        &revision,
        &clip_path,
        json!({"kind": "agency", "ref": "agency/x"}),
        &[],
    ));
    assert_eq!(agency.code, "workflow.edit_invalid");
    let stale = refused(canon(
        "blake3:0",
        &clip_path,
        json!({"kind": "agent", "ref": "agent/anima-eros"}),
        &[],
    ));
    assert_eq!(stale.code, "workflow.edit_stale");
    let unknown = refused(canon(
        &revision,
        &clip_path,
        json!({"kind": "agent", "ref": "agent/anima-eros"}),
        &["nobody"],
    ));
    assert_eq!(unknown.code, "workflow.edit_unit");
    let missing = refused(canon(
        &revision,
        &maker.root.join("none.song.json"),
        json!({"kind": "agent", "ref": "agent/anima-eros"}),
        &[],
    ));
    assert_eq!(missing.code, "workflow.song_missing");
    let not_a_clip = maker.root.join("not-a-clip.json");
    fs::write(&not_a_clip, br#"{"contract":"factory.workflow-score/v1"}"#).unwrap();
    let invalid = refused(canon(
        &revision,
        &not_a_clip,
        json!({"kind": "agent", "ref": "agent/anima-eros"}),
        &[],
    ));
    assert_eq!(invalid.code, "workflow.song_invalid");
    let taken = refused(
        json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision,
        "edits": [{"op": "canon", "clip": clip_path.to_string_lossy(), "to": {"kind": "agent", "ref": "agent/anima-eros"}, "key": "eros-verify"}]}),
    );
    assert_eq!(taken.code, "workflow.edit_unit");
}

#[test]
fn help_names_the_songs_verbs_and_their_contracts() {
    let help = run(&["workflow", "help", "--json"]).unwrap();
    for command in ["workflow.export", "workflow.remake", "workflow.songs"] {
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c == command),
            "{command}"
        );
    }
    for contract in ["factory.song-clip/v1", "factory.song-clips/v1"] {
        assert!(
            help["contracts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c == contract),
            "{contract}"
        );
    }
    let words = help["help"].as_str().unwrap();
    assert!(
        words.contains("factory workflow export <state> <song.workflow.ts>")
            && words.contains("canon"),
        "{words}"
    );
}
