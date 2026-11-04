//! O-I #843: a song's score, and the writes that reshape it (DLW manual §19.1 Notes, Tools; Rules: "A note's row is who does the
//! unit", "A song branches by allocation, never by a condition", "The tools reshape the score").
//!
//! Proves: `factory workflow score` reads the song a Run performs as one score — its units with their compositions, the rows of
//! the agents and agent sets the units name, its joins, its waves and each unit's leg — from the song's own file under its project
//! root, and says whether the Run still performs that edition; `factory workflow edit` reshapes the song's file in place (Allocate,
//! Connect, a unit's heads), compare-and-swap on the file's byte revision, every edit checked by the compiler before the file
//! changes, comments and the rest of the file kept; and what Factory cannot write it refuses in its own words, leaving the file
//! as it was: a stale revision, a field the song shares through a const, an edit the compiler refuses, a song that does not stand
//! under its project root.
use epilogos_factory::workflow_authoring::cli;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "support/executable.rs"]
mod executable;

const SONG: &str = "development-and-knowledge.workflow.ts";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ql-vak-workflows")
        .join(name)
}

fn sdk_example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflow-sdk/examples")
        .join(name)
}

fn run(args: &[&str]) -> Result<Value, epilogos_factory::workflow_authoring::Diagnostic> {
    cli::execute(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
}

/// A placed Factory project holding one song, commissioned from the project's root.
struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
    state: String,
    run: String,
}

fn participants(refs: &[&str]) -> Vec<Value> {
    refs.iter()
        .map(|reference| json!({"ref": reference, "description": "A voice the song names", "sourceOwner": "central",
            "sourceRef": "source:specimen", "sourceRevision": "specimen-r1"}))
        .collect()
}

fn project(
    song: &Path,
    name: &str,
    root_act: &str,
    refs: &[&str],
    scope: &str,
    inside: bool,
) -> Project {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let placed = epilogos_factory::project_setup::setup(&root, "score-specimen", None).unwrap();
    let state = placed["statePath"].as_str().unwrap().to_owned();
    let source_dir = if inside {
        root.clone()
    } else {
        dir.path().join("elsewhere")
    };
    fs::create_dir_all(&source_dir).unwrap();
    fs::copy(song, source_dir.join(name)).unwrap();
    let request = dir.path().join("commission.json");
    fs::write(&request, serde_json::to_vec_pretty(&json!({
        "contract": "factory.commission-request/v1",
        "requestRef": "commission-request:score-specimen",
        "projectKey": "score-specimen",
        "purpose": "Admit the song so its score can be read",
        "frontier": "Declared source; no execution has been requested",
        "runDestination": "score-specimen/first",
        "writeOwner": "factory",
        "commissionedAt": "2026-11-03T00:00:00Z",
        "participantRequirements": participants(refs),
        "rootAct": {"actRef": "act:score-specimen", "agentRef": root_act, "purpose": "Read the song's score",
            "scopeRefs": [scope], "standing": "commissioned-not-executed"}
    })).unwrap()).unwrap();
    let source_root = source_dir.to_string_lossy().into_owned();
    let receipt = run(&[
        "workflow",
        "commission",
        &state,
        &request.to_string_lossy(),
        name,
        "--root",
        &source_root,
    ])
    .unwrap();
    let run_ref = receipt["commission"]["commission"]["runRef"]
        .as_str()
        .unwrap()
        .to_owned();
    Project {
        root: if inside { root } else { source_dir },
        _dir: dir,
        state,
        run: run_ref,
    }
}

const ANIMA: &[&str] = &[
    "agent/anima-logos",
    "agent/anima-techne-helper",
    "agent/anima-eros",
    "agent/anima-mythos",
    "agent/anima-sophia",
    "agent/aletheia",
];

fn anima() -> Project {
    project(
        &fixture(SONG),
        SONG,
        "agent/anima-logos",
        ANIMA,
        "project:quaternal-logic",
        true,
    )
}

fn unit<'a>(score: &'a Value, key: &str) -> &'a Value {
    score["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|unit| unit["key"] == key)
        .unwrap_or_else(|| panic!("no unit {key}"))
}

fn score(p: &Project) -> Value {
    run(&["workflow", "score", &p.state, &p.run, "--json"]).unwrap()
}

fn edit(
    p: &Project,
    request: Value,
) -> Result<Value, epilogos_factory::workflow_authoring::Diagnostic> {
    let path = p.root.join("edit.json");
    fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
    let result = run(&[
        "workflow",
        "edit",
        &p.state,
        &p.run,
        &path.to_string_lossy(),
        "--json",
    ]);
    fs::remove_file(&path).unwrap();
    result
}

fn revision(score: &Value) -> String {
    score["song"]["file"]["byteRevision"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Nothing but the song (and Factory's own folder) stands in the project after an edit: no temporary file is left.
fn only_the_song(p: &Project, name: &str) {
    let mut names = fs::read_dir(&p.root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, vec![".factory".to_owned(), name.to_owned()]);
}

#[test]
fn the_score_is_the_song_its_run_performs() {
    let p = anima();
    let s = score(&p);
    assert_eq!(s["contract"], "factory.workflow-score/v1");
    assert_eq!(s["runRef"], p.run);
    assert_eq!(s["song"]["workflowKey"], "development-and-knowledge");
    assert_eq!(s["song"]["edition"], "file");
    assert_eq!(s["song"]["performed"], true);
    assert_eq!(s["song"]["file"]["entry"], SONG);
    // Every unit, in the song's order, its row the agent that does it, its heads its composition's.
    let keys = s["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| unit["key"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        keys[..4],
        [
            "logos-scope-plan",
            "techne-implement",
            "eros-verify",
            "mythos-debug"
        ]
    );
    let eros = unit(&s, "eros-verify");
    assert_eq!(eros["row"], "agent/anima-eros");
    assert_eq!(eros["dependencies"], json!(["techne-implement"]));
    assert_eq!(
        eros["heads"],
        json!({"CPF": "dialogical", "CT": "CT2", "CP": "4.2", "CF": "CF3", "CFP": "CFP2", "CS": "CS2", "direction": "returning"})
    );
    assert_eq!(
        eros["concern"],
        "Verify the implementation against the plan with the repository's real gates"
    );
    assert_eq!(
        eros["returns"],
        "Return the commands run, their results and every failure found"
    );
    assert!(eros["success"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| line.as_str().unwrap().starts_with("quote executed results")));
    assert_eq!(eros["stop"], "Stop when the branch cannot be built");
    assert_eq!(eros["leg"]["standing"], "not-started");
    assert_eq!(eros["edited"], false);
    // The rows are the agents and agent sets the units name, each once, with its kind.
    let rows = s["rows"].as_array().unwrap();
    assert!(rows.contains(&json!({"ref": "agent/anima-eros", "kind": "agent"})));
    assert!(rows.contains(&json!({"ref": "agent-set/anima", "kind": "agent-set"})));
    assert_eq!(
        rows.iter()
            .filter(|row| row["ref"] == "agent/anima-eros")
            .count(),
        1
    );
    // A Run nobody planned holds no wave, and the compiler admitted the song's file.
    assert_eq!(s["waves"], json!([]));
    assert_eq!(s["check"]["state"], "admitted");
    let checked = run(&[
        "workflow",
        "check",
        SONG,
        "--root",
        &p.root.to_string_lossy(),
    ])
    .unwrap();
    assert_eq!(
        revision(&s),
        checked["source"]["authoring"]["revision"].as_str().unwrap()
    );
}

#[test]
fn allocate_gives_a_unit_to_another_agent_through_the_songs_file() {
    let p = anima();
    let before = fs::read_to_string(p.root.join(SONG)).unwrap();
    let s = score(&p);
    let receipt = edit(&p, json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "allocate", "units": ["eros-verify"], "to": {"kind": "agent", "ref": "agent/anima-mythos"}}]})).unwrap();
    assert_eq!(receipt["contract"], "factory.workflow-edit-receipt/v1");
    assert_eq!(receipt["previousRevision"], revision(&s));
    assert_eq!(receipt["check"]["state"], "admitted");
    let after = fs::read_to_string(p.root.join(SONG)).unwrap();
    assert_ne!(before, after);
    // Only the unit's own words moved: its agent and its composition's actor. Comments and every other line stand.
    let changed = before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(changed, 2, "{after}");
    assert_eq!(before.lines().count(), after.lines().count());
    assert!(after.starts_with(
        "// Development and knowledge: ordinary software development carried by Anima's"
    ));
    let s = score(&p);
    let eros = unit(&s, "eros-verify");
    assert_eq!(eros["row"], "agent/anima-mythos");
    assert_eq!(eros["edited"], true);
    assert_eq!(unit(&s, "mythos-debug")["edited"], false);
    assert_eq!(s["song"]["performed"], false);
    assert_eq!(revision(&s), receipt["revision"].as_str().unwrap());
    only_the_song(&p, SONG);
}

#[test]
fn a_stale_revision_is_refused_and_the_file_stands() {
    let p = anima();
    let before = fs::read_to_string(p.root.join(SONG)).unwrap();
    let refused = edit(&p, json!({"contract": "factory.workflow-edit/v1", "expectedRevision": "blake3:0000",
        "edits": [{"op": "allocate", "units": ["eros-verify"], "to": {"kind": "agent", "ref": "agent/anima-mythos"}}]})).unwrap_err();
    assert_eq!(refused.code, "workflow.edit_stale");
    assert!(
        refused.message.contains("changed since it was read"),
        "{}",
        refused.message
    );
    assert_eq!(before, fs::read_to_string(p.root.join(SONG)).unwrap());
}

#[test]
fn a_field_the_song_shares_through_a_const_is_refused() {
    // Every unit names its set through one const (`agentSetRefs: anima`): giving one unit to another set would rewrite them all.
    let p = anima();
    let before = fs::read_to_string(p.root.join(SONG)).unwrap();
    let s = score(&p);
    let refused = edit(&p, json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "allocate", "units": ["eros-verify"], "to": {"kind": "agent-set", "ref": "agent-set/aletheia"}}]})).unwrap_err();
    assert_eq!(refused.code, "workflow.edit_shared");
    assert_eq!(refused.unit.as_deref(), Some("eros-verify"));
    assert!(refused.message.contains("shares"), "{}", refused.message);
    assert_eq!(before, fs::read_to_string(p.root.join(SONG)).unwrap());
    only_the_song(&p, SONG);
}

#[test]
fn an_edit_the_compiler_refuses_leaves_the_song_as_it_was() {
    // An undertaking names its authority: a dialogical unit made an undertaking without one is refused where the composition lowers.
    let p = anima();
    let before = fs::read_to_string(p.root.join(SONG)).unwrap();
    let s = score(&p);
    let refused = edit(&p, json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "heads", "unit": "eros-verify", "heads": {"CPF": "authorised-undertaking"}}]})).unwrap_err();
    assert_eq!(refused.code, "authoring.domain_lowering");
    assert_eq!(refused.unit.as_deref(), Some("eros-verify"));
    assert_eq!(
        refused.location.as_ref().map(|at| at.file.as_str()),
        Some(SONG)
    );
    assert_eq!(before, fs::read_to_string(p.root.join(SONG)).unwrap());
    only_the_song(&p, SONG);
    // A head the compiler admits is written in place.
    let receipt = edit(
        &p,
        json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "heads", "unit": "eros-verify", "heads": {"CS": "CS3", "CP": "4.3"}}]}),
    )
    .unwrap();
    assert_eq!(receipt["check"]["state"], "admitted");
    let read = score(&p);
    let heads = &unit(&read, "eros-verify")["heads"];
    assert_eq!(
        (heads["CS"].as_str(), heads["CP"].as_str()),
        (Some("CS3"), Some("4.3"))
    );
}

#[test]
fn connect_makes_a_unit_wait_on_another() {
    let p = project(
        &sdk_example("plural.workflow.ts"),
        "plural.workflow.ts",
        "agent:01ARZ3NDEKTSV4RRFFQ69G5FAX",
        &["agent:01ARZ3NDEKTSV4RRFFQ69G5FAX"],
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
        true,
    );
    let s = score(&p);
    assert_eq!(unit(&s, "integrate-return")["dependencies"], json!([]));
    assert_eq!(
        s["joins"],
        json!([{"key": "implementation-reviewed", "waitsFor": ["implement-compiler", "review-adversarially"],
        "releases": ["integrate-return"], "join": "all", "complete": false}])
    );
    let receipt = edit(
        &p,
        json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "connect", "unit": "integrate-return", "waitsOn": "inspect-source"}]}),
    )
    .unwrap();
    assert_eq!(receipt["check"]["state"], "admitted");
    assert_eq!(
        unit(&score(&p), "integrate-return")["dependencies"],
        json!(["inspect-source"])
    );
    // A unit that would wait on itself is refused by the compiler, in its words.
    let s = score(&p);
    let refused = edit(
        &p,
        json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "connect", "unit": "inspect-source", "waitsOn": "inspect-source"}]}),
    )
    .unwrap_err();
    assert!(refused.code.starts_with("authoring."), "{refused}");
    only_the_song(&p, "plural.workflow.ts");
}

#[test]
fn connect_writes_the_field_a_unit_leaves_out() {
    // integrate-return names no dependencies at all: Connect writes the field into the unit, in the song's own quoting.
    let plural = fs::read_to_string(sdk_example("plural.workflow.ts")).unwrap();
    let at = plural.find("\"key\": \"integrate-return\"").unwrap();
    let field = at + plural[at..].find("      \"dependencies\": [],\n").unwrap();
    let without = format!(
        "{}{}",
        &plural[..field],
        &plural[field + "      \"dependencies\": [],\n".len()..]
    );
    let dir = tempfile::tempdir().unwrap();
    let song = dir.path().join("plural.workflow.ts");
    fs::write(&song, &without).unwrap();
    let p = project(
        &song,
        "plural.workflow.ts",
        "agent:01ARZ3NDEKTSV4RRFFQ69G5FAX",
        &["agent:01ARZ3NDEKTSV4RRFFQ69G5FAX"],
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
        true,
    );
    let s = score(&p);
    edit(
        &p,
        json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "connect", "unit": "integrate-return", "waitsOn": "inspect-source"}]}),
    )
    .unwrap();
    assert_eq!(
        unit(&score(&p), "integrate-return")["dependencies"],
        json!(["inspect-source"])
    );
    let after = fs::read_to_string(p.root.join("plural.workflow.ts")).unwrap();
    let unit_text = &after[after.find("\"key\": \"integrate-return\"").unwrap()..];
    assert!(
        after[..after.find("\"key\": \"integrate-return\"").unwrap()]
            .ends_with("{\n      \"dependencies\": [\"inspect-source\"],\n      "),
        "{after}"
    );
    assert!(unit_text.starts_with("\"key\": \"integrate-return\","));
}

#[test]
fn a_song_outside_its_project_root_reads_the_edition_its_run_performs_and_takes_no_edit() {
    let p = project(
        &fixture(SONG),
        SONG,
        "agent/anima-logos",
        ANIMA,
        "project:quaternal-logic",
        false,
    );
    let s = score(&p);
    assert_eq!(s["song"]["edition"], "retained");
    assert!(
        s["song"]["why"]
            .as_str()
            .unwrap()
            .contains("does not stand under its project root"),
        "{}",
        s["song"]["why"]
    );
    assert_eq!(s["song"]["file"], Value::Null);
    assert_eq!(unit(&s, "eros-verify")["row"], "agent/anima-eros");
    let refused = edit(&p, json!({"contract": "factory.workflow-edit/v1", "expectedRevision": "blake3:0000",
        "edits": [{"op": "allocate", "units": ["eros-verify"], "to": {"kind": "agent", "ref": "agent/anima-mythos"}}]})).unwrap_err();
    assert_eq!(refused.code, "workflow.song_unplaced");
}

#[test]
fn help_names_the_score_and_its_edit() {
    let help = run(&["workflow", "help", "--json"]).unwrap();
    let commands = help["commands"].as_array().unwrap();
    assert!(
        commands.contains(&json!("workflow.score")) && commands.contains(&json!("workflow.edit"))
    );
    let contracts = help["contracts"].as_array().unwrap();
    for contract in [
        "factory.workflow-score/v1",
        "factory.workflow-edit/v1",
        "factory.workflow-edit-receipt/v1",
    ] {
        assert!(contracts.contains(&json!(contract)), "{contract}");
    }
    assert!(help["help"]
        .as_str()
        .unwrap()
        .contains("factory workflow score <state> <run-ref>"));
}

/// A forge Factory reads through its own port, faked: `gh api <endpoint>` answered from a table, with no network. An endpoint the
/// table does not hold answers as GitHub does, Not Found.
fn forge(dir: &Path, table: Value) -> PathBuf {
    let answers = dir.join("forge.json");
    fs::write(&answers, table.to_string()).unwrap();
    let gh = dir.join("gh");
    executable::write_executable(
        &gh,
        format!(
            "#!/usr/bin/python3\nimport json,sys\nt=json.load(open({:?}))\ne=sys.argv[-1]\n\
             if sys.argv[1:2]!=['api'] or e not in t:\n    sys.stderr.write('gh: Not Found (HTTP 404): '+e+'\\n')\n    sys.exit(1)\n\
             print(json.dumps(t[e]))\n",
            answers.display().to_string()
        ),
    );
    gh
}

fn ticket(repo: &str, number: u64, title: &str, state: &str) -> Value {
    json!({"number": number, "title": title, "state": state, "html_url": format!("https://github.com/{repo}/issues/{number}"),
        "repository_url": format!("https://api.github.com/repos/{repo}"), "labels": [{"name": "wayfinder:task"}], "assignees": []})
}

fn blocked_by(repo: &str, number: u64) -> String {
    format!("repos/{repo}/issues/{number}/dependencies/blocked_by?per_page=100")
}

const MAP_REPO: &str = "EpiLogos/O-I";
const SUBS: &str = "repos/EpiLogos/O-I/issues/706/sub_issues?per_page=100&page=1";

fn map_issue(labelled: bool) -> Value {
    let mut map = ticket(MAP_REPO, 706, "The Living World", "open");
    map["labels"] = if labelled {
        json!([{"name": "wayfinder:map"}])
    } else {
        json!([])
    };
    map
}

fn the_map() -> Value {
    json!({
        "repos/EpiLogos/O-I/issues/706": map_issue(true),
        SUBS: [ticket(MAP_REPO, 843, "Factory's Clip view", "open"), ticket(MAP_REPO, 844, "Songs", "open"),
            ticket(MAP_REPO, 845, "A resolved question", "closed"), ticket(MAP_REPO, 846, "The Intent panel", "open"),
            ticket("EpiLogos/ai-kit", 589, "Capture from a song", "open")],
        blocked_by(MAP_REPO, 843): [],
        blocked_by(MAP_REPO, 844): [ticket(MAP_REPO, 843, "Factory's Clip view", "open")],
        blocked_by(MAP_REPO, 846): [ticket(MAP_REPO, 845, "A resolved question", "closed"), ticket("EpiLogos/QL-MEF", 999, "The engine's word", "open")],
        blocked_by("EpiLogos/ai-kit", 589): [ticket(MAP_REPO, 844, "Songs", "open")],
    })
}

fn read_map(
    gh: &Path,
    number: &str,
) -> Result<Value, epilogos_factory::workflow_authoring::Diagnostic> {
    run(&[
        "workflow",
        "map",
        MAP_REPO,
        number,
        "--gh",
        &gh.to_string_lossy(),
        "--json",
    ])
}

/// O-I #844 (manual §19.1 Songs, "A project's map gives a development song its units: each ticket is a unit"): Factory reads the
/// project's map through the forge port it reads pull requests by. The map is a `wayfinder:map` issue; its open sub-issues are the
/// units, in any repository; blocked-by gives the order — a unit's wave is one past the latest of the open map units that block it,
/// a closed ticket blocks nothing, and an open blocker the map does not hold is named where its unit waits on it. Nothing is written.
#[test]
fn a_projects_map_gives_a_development_song_its_units_in_the_order_its_blocking_gives() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), the_map());
    let reading = read_map(&gh, "706").unwrap();
    assert_eq!(reading["contract"], "factory.project-map-reading/v1");
    assert_eq!(reading["map"]["ticket"], "github:EpiLogos/O-I:issue:706");
    assert_eq!(reading["map"]["title"], "The Living World");
    let units = reading["units"].as_array().unwrap();
    let keys: Vec<&str> = units
        .iter()
        .map(|unit| unit["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        ["issue-843", "issue-846", "issue-844", "epilogos-ai-kit-589"],
        "by wave, then by number"
    );
    let of = |key: &str| units.iter().find(|unit| unit["key"] == key).unwrap();
    assert_eq!(of("issue-843")["wave"], "wave-1");
    assert_eq!(of("issue-843")["ticket"], "github:EpiLogos/O-I:issue:843");
    assert_eq!(of("issue-843")["title"], "Factory's Clip view");
    assert_eq!(of("issue-844")["wave"], "wave-2");
    assert_eq!(of("issue-844")["blockedBy"], json!(["issue-843"]));
    assert_eq!(of("epilogos-ai-kit-589")["wave"], "wave-3");
    assert_eq!(
        of("epilogos-ai-kit-589")["ticket"],
        "github:EpiLogos/ai-kit:issue:589"
    );
    // A closed blocker blocks nothing; an open one outside the map is where the unit waits.
    assert_eq!(of("issue-846")["wave"], "wave-1");
    assert_eq!(of("issue-846")["blockedBy"], json!([]));
    assert_eq!(
        of("issue-846")["waitsOutside"],
        json!([{"ticket": "github:EpiLogos/QL-MEF:issue:999", "title": "The engine's word",
        "url": "https://github.com/EpiLogos/QL-MEF/issues/999"}])
    );
    assert_eq!(
        reading["waves"],
        json!([{"wave": "wave-1", "units": ["issue-843", "issue-846"]}, {"wave": "wave-2", "units": ["issue-844"]},
        {"wave": "wave-3", "units": ["epilogos-ai-kit-589"]}])
    );
    assert_eq!(
        reading["closed"],
        json!(["github:EpiLogos/O-I:issue:845"]),
        "a resolved ticket is no unit"
    );
    assert_eq!(reading["written"], false);
    let help = run(&["workflow", "help", "--json"]).unwrap();
    assert!(help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "workflow.map"));
    assert!(help["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "factory.project-map-reading/v1"));
}

#[test]
fn what_is_not_a_map_or_cannot_be_ordered_is_refused_in_factorys_words() {
    let dir = tempfile::tempdir().unwrap();
    // An issue without the label is no map.
    let mut table = the_map();
    table["repos/EpiLogos/O-I/issues/706"] = map_issue(false);
    let refused = read_map(&forge(dir.path(), table), "706").unwrap_err();
    assert_eq!(refused.code, "workflow.not_a_map");
    assert!(
        refused.message.contains("EpiLogos/O-I#706") && refused.message.contains("wayfinder:map"),
        "{}",
        refused.message
    );
    // Blocking that runs in a circle gives no order.
    let mut table = the_map();
    table[blocked_by(MAP_REPO, 843)] = json!([ticket(MAP_REPO, 844, "Songs", "open")]);
    let refused = read_map(&forge(dir.path(), table), "706").unwrap_err();
    assert_eq!(refused.code, "workflow.map_cycle");
    assert!(
        refused.message.contains("issue-843") && refused.message.contains("issue-844"),
        "{}",
        refused.message
    );
    // What the forge does not answer is said in its words.
    let mut table = the_map();
    table
        .as_object_mut()
        .unwrap()
        .remove(&blocked_by(MAP_REPO, 846));
    let refused = read_map(&forge(dir.path(), table), "706").unwrap_err();
    assert_eq!(refused.code, "workflow.map_unread");
    assert!(refused.message.contains("Not Found"), "{}", refused.message);
    // The port is an explicit absolute gh, as Factory's pull-request reads take it; the repository is exact.
    let refused = run(&["workflow", "map", MAP_REPO, "706", "--gh", "gh", "--json"]).unwrap_err();
    assert!(refused.message.contains("absolute"), "{}", refused.message);
    let gh = forge(dir.path(), the_map());
    let refused = run(&[
        "workflow",
        "map",
        "O-I",
        "706",
        "--gh",
        &gh.to_string_lossy(),
        "--json",
    ])
    .unwrap_err();
    assert!(
        refused.message.contains("owner/repository"),
        "{}",
        refused.message
    );
    assert!(run(&[
        "workflow",
        "map",
        MAP_REPO,
        "seven",
        "--gh",
        &gh.to_string_lossy(),
        "--json"
    ])
    .is_err());
}

/// O-I #844 Compose (lead ruling, r15): a proposal is admitted by Factory before the person accepts it, and nothing is written until
/// then — `factory workflow edit --check` compiles the edit as the write would, and answers without changing the song's file.
#[test]
fn an_edit_is_admitted_without_being_written_and_a_refusal_is_the_compilers() {
    let p = anima();
    let before = fs::read_to_string(p.root.join(SONG)).unwrap();
    let s = score(&p);
    let path = p.root.join("edit.json");
    let check = |request: Value| {
        fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
        let answer = run(&[
            "workflow",
            "edit",
            &p.state,
            &p.run,
            &path.to_string_lossy(),
            "--check",
            "--json",
        ]);
        fs::remove_file(&path).unwrap();
        answer
    };
    let admitted = check(
        json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "heads", "unit": "eros-verify", "heads": {"CS": "CS3"}}]}),
    )
    .unwrap();
    assert_eq!(admitted["contract"], "factory.workflow-edit-receipt/v1");
    assert_eq!(admitted["check"]["state"], "admitted");
    assert_eq!(admitted["written"], false);
    assert_eq!(
        admitted["revision"],
        revision(&s),
        "the file's revision stands"
    );
    assert_eq!(admitted["changed"].as_array().map(Vec::len), Some(1));
    assert_eq!(before, fs::read_to_string(p.root.join(SONG)).unwrap());
    let refused = check(json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "heads", "unit": "eros-verify", "heads": {"CPF": "authorised-undertaking"}}]}))
    .unwrap_err();
    assert_eq!(refused.code, "authoring.domain_lowering");
    assert_eq!(before, fs::read_to_string(p.root.join(SONG)).unwrap());
    only_the_song(&p, SONG);
    // A written edit says so.
    let written = edit(
        &p,
        json!({"contract": "factory.workflow-edit/v1", "expectedRevision": revision(&s),
        "edits": [{"op": "heads", "unit": "eros-verify", "heads": {"CS": "CS3"}}]}),
    )
    .unwrap();
    assert_eq!(written["written"], true);
}
