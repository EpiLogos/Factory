//! Native sensing operations extend `factory telemetry`. Classification and
//! commissioning have separate policy/Actuation gates; digest/history are pure
//! reads and cannot approve, recognise, close, or deploy anything.

use crate::cli::CliError;
use crate::developmental_read::FactoryDevelopmentalState;
use crate::project_development_store::{
    read_developmental_state, transact_developmental_state, ProjectDevelopmentStoreError,
};
use crate::sensing::*;
use crate::sensing_sources::{now_ms, owner_json};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn err(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
fn store_err(error: impl std::fmt::Display) -> ProjectDevelopmentStoreError {
    ProjectDevelopmentStoreError::Native(error.to_string())
}
fn digest(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}
fn read_json(path: &Path) -> Result<Value, CliError> {
    serde_json::from_slice(&std::fs::read(path).map_err(err)?).map_err(err)
}
fn string(value: &Value, key: &str) -> Result<String, CliError> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| err(format!("{key} is required")))
}
fn strings(value: &Value, key: &str) -> Result<Vec<String>, CliError> {
    value[key]
        .as_array()
        .ok_or_else(|| err(format!("{key} must be an array")))?
        .iter()
        .map(|s| {
            s.as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| err(format!("{key} contains an empty/non-string ref")))
        })
        .collect()
}

#[derive(Clone)]
struct Arguments {
    previous: Option<(Interval, Value)>,
    path: PathBuf,
    policy: Option<PathBuf>,
    request: Option<PathBuf>,
    subject: Option<String>,
    window: Interval,
    window_basis: Value,
    limit: usize,
}
fn parse(args: &[String]) -> Result<Arguments, CliError> {
    let path = PathBuf::from(
        args.first()
            .ok_or_else(|| err("missing developmental state path"))?,
    );
    let (civil, remaining) = crate::telemetry_time::CivilWindow::from_args(&args[1..], &path)?;
    let mut out = Arguments {
        previous: None,
        path,
        policy: None,
        request: None,
        subject: None,
        window: Interval {
            since_unix_ms: now_ms() - 86_400_000,
            until_unix_ms: now_ms(),
        },
        window_basis: json!({"basis":"last 24 hours","interval":"[since,until)"}),
        limit: 100,
    };
    if let Some(civil) = civil.as_ref() {
        out.window = Interval {
            since_unix_ms: civil.start_ms,
            until_unix_ms: civil.end_ms,
        };
        out.window_basis = civil.as_json();
    }
    let mut iter = remaining.iter();
    let mut seen = BTreeSet::new();
    while let Some(arg) = iter.next() {
        if arg.starts_with("--") {
            if !seen.insert(arg) {
                return Err(err(format!("{arg} repeated")));
            }
            if arg == "--compare-previous" {
                let prior = civil
                    .as_ref()
                    .ok_or_else(|| err("--compare-previous requires an explicit civil Day window"))?
                    .previous(&out.path)?;
                out.previous = Some((
                    Interval {
                        since_unix_ms: prior.start_ms,
                        until_unix_ms: prior.end_ms,
                    },
                    prior.as_json(),
                ));
                continue;
            }
            let value = iter
                .next()
                .ok_or_else(|| err(format!("{arg} requires a value")))?;
            match arg.as_str() {
                "--policy" => out.policy = Some(value.into()),
                "--request" => out.request = Some(value.into()),
                "--limit" => out.limit = value.parse().map_err(err)?,
                "--since" | "--until" => {
                    if civil.is_some() {
                        return Err(err(
                            "civil Day and explicit timestamp windows cannot be mixed",
                        ));
                    }
                    let time = chrono::DateTime::parse_from_rfc3339(value)
                        .map_err(err)?
                        .timestamp_millis();
                    if arg == "--since" {
                        out.window.since_unix_ms = time;
                    } else {
                        out.window.until_unix_ms = time;
                    }
                }
                _ => return Err(err(format!("unknown sensing option {arg}"))),
            }
        } else if out.subject.replace(arg.clone()).is_some() {
            return Err(err("only one signal ref is accepted"));
        }
    }
    if out.window.until_unix_ms <= out.window.since_unix_ms
        || out.window.until_unix_ms - out.window.since_unix_ms > 366 * 86_400_000i64
    {
        return Err(err("window must be positive and no longer than 366 days"));
    }
    if out.limit == 0 || out.limit > 1000 {
        return Err(err("--limit must be 1..1000; truncation is disclosed"));
    }
    Ok(out)
}

fn policy_path(args: &Arguments) -> Result<PathBuf, CliError> {
    if let Some(path) = &args.policy {
        return Ok(path.clone());
    }
    let state = std::fs::canonicalize(&args.path).map_err(err)?;
    for base in state.ancestors() {
        let path = base.join("ProjectCentral/user/factory-policy.json");
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(err(
        "no Project Factory policy found; pass --policy <ProjectCentral/user/factory-policy.json>",
    ))
}

fn load_policy(
    args: &Arguments,
    state: &FactoryDevelopmentalState,
) -> Result<(Policy, String, String), CliError> {
    let path = policy_path(args)?;
    let bytes = std::fs::read(&path).map_err(err)?;
    let policy:Policy=serde_json::from_slice(&bytes).map_err(|e|err(format!("{}: native policy is JSON; preserve and explicitly convert any Builder YAML choices: {e}",path.display())))?;
    policy.validate().map_err(err)?;
    check_scope(state, &policy.project_world_ref)?;
    Ok((policy, path.display().to_string(), digest(&bytes)))
}

fn world(state: &FactoryDevelopmentalState) -> String {
    state
        .sensing
        .project_world_ref
        .clone()
        .unwrap_or_else(|| grounded_world(state))
}
fn grounded_world(state: &FactoryDevelopmentalState) -> String {
    state
        .central_project_links
        .get(state.build.project().reference())
        .map(|link| {
            let reference = &link.central_project_ref;
            if reference == "control:root" || reference.starts_with("project:") {
                reference.clone()
            } else {
                format!("project:{reference}")
            }
        })
        .unwrap_or_else(|| state.project_ref().to_string())
}
fn check_scope(state: &FactoryDevelopmentalState, requested: &str) -> Result<(), CliError> {
    let expected = grounded_world(state);
    if requested != expected {
        return Err(err(format!("scope refusal: provider belongs to {expected}, policy requests {requested}; admit the Central Project link through its native owner first")));
    }
    if state
        .sensing
        .project_world_ref
        .as_deref()
        .is_some_and(|bound| bound != requested)
    {
        return Err(err(
            "sensing state is already bound to another ProjectWorld",
        ));
    }
    Ok(())
}

pub fn execute(args: &[String], as_json: bool) -> Result<String, CliError> {
    let operation = args
        .first()
        .ok_or_else(|| err("missing sensing operation"))?;
    let args = parse(&args[1..])?;
    if args.previous.is_some() && operation != "lookback" {
        return Err(err("--compare-previous is supported for lookback"));
    }
    let document = match operation.as_str() {
        "collect" => collect(&args)?,
        "classify" => classify(&args)?,
        "commission" => commission(&args)?,
        "return" => record_return(&args)?,
        "policy" => {
            let state = read_developmental_state(&args.path).map_err(err)?;
            let (policy, path, revision) = load_policy(&args, &state)?;
            json!({"schema":POLICY_SCHEMA,"source_ref":path,"revision":revision,"policy":policy,"authority":"policy is not a grant"})
        }
        "field" => field(&args)?,
        "signals" | "signal" | "digest" | "lookback" | "day" => reading(operation, &args)?,
        _ => return Err(err(format!("unknown sensing operation {operation}"))),
    };
    if as_json {
        serde_json::to_string_pretty(&document).map_err(err)
    } else {
        Ok(render(&document))
    }
}

fn collect(args: &Arguments) -> Result<Value, CliError> {
    let state = read_developmental_state(&args.path).map_err(err)?;
    let (policy, policy_ref, policy_revision) = load_policy(args, &state)?;
    let flow = policy
        .workflows
        .get("collect")
        .filter(|f| f.enabled)
        .ok_or_else(|| err("collection is disabled or unconfigured in Project policy"))?;
    let mut coverage = vec![];
    let mut observations = vec![];
    for id in &flow.sources {
        let source = policy
            .sources
            .iter()
            .find(|s| &s.id == id)
            .ok_or_else(|| err("undefined source"))?;
        let (read, mut items) = crate::sensing_sources::read(source, &args.window, &state);
        coverage.push(read);
        observations.append(&mut items);
    }
    if flow.sources.is_empty() {
        return Err(err(
            "collection has no configured sources; this is not an empty-source observation",
        ));
    }
    let recorded = now_ms();
    // Observation time is a receipt fact, not source identity. Replaying an
    // identical complete read must not invent another collection or work.
    let mut identity = serde_json::to_value(&observations).map_err(err)?;
    for observation in identity.as_array_mut().into_iter().flatten() {
        observation
            .as_object_mut()
            .map(|o| o.remove("observed_at_unix_ms"));
    }
    let request_digest=digest(serde_json::to_string(&json!({"policy":policy_revision,"window":args.window,"coverage":coverage,"observations":identity})).map_err(err)?.as_bytes());
    let collection_ref = format!(
        "factory:collection:{}",
        request_digest.trim_start_matches("blake3:")
    );
    let receipt = transact_developmental_state(&args.path, |current| {
        check_scope(current, &policy.project_world_ref).map_err(store_err)?;
        // Policy is source, not cached authority: a concurrent edit invalidates
        // the entire intake before any observed relation is published.
        if digest(&std::fs::read(&policy_ref)?) != policy_revision {
            return Err(store_err(
                "policy changed during collection; reread and retry",
            ));
        }
        if let Some(previous) = current
            .sensing
            .collections
            .iter()
            .find(|r| r.collection_ref == collection_ref)
        {
            return Ok(previous.clone());
        }
        current.sensing.project_world_ref = Some(policy.project_world_ref.clone());
        let refs = observations
            .into_iter()
            .map(|item| retain(&mut current.sensing, &policy.project_world_ref, item))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let receipt = Collection {
            collection_ref,
            policy_ref,
            policy_revision,
            recorded_at_unix_ms: recorded,
            coverage,
            signal_refs: refs,
        };
        current.sensing.collections.push(receipt.clone());
        current.sensing.revision += 1;
        Ok(receipt)
    })
    .map_err(err)?;
    Ok(
        json!({"schema":COLLECTION_SCHEMA,"project_world_ref":policy.project_world_ref,"collection":receipt,"work_created":false,"basis":"collection observes; classification and authorised commissioning are separate operations"}),
    )
}

struct Authority {
    actor: String,
    reference: String,
    agent: String,
}
fn authority(request: &Value, world: &str, action: &str) -> Result<Authority, CliError> {
    let resolution = &request["authority_request"];
    let binding = &resolution["differentiated_binding"];
    if binding["world_ref"] != world {
        return Err(err("authority world does not match signal ProjectWorld"));
    }
    let expected_bound = format!("bound:factory-sensing:{world}");
    let bounds = strings(resolution, "requested_bounds_refs")?;
    if !bounds.contains(&expected_bound) {
        return Err(err(format!(
            "authority must explicitly carry {expected_bound}"
        )));
    }
    let action_bound = format!(
        "{expected_bound}:{}",
        action.trim_start_matches("factory:action/")
    );
    if !bounds.contains(&action_bound) {
        return Err(err(format!(
            "authority must carry the separately granted action bound {action_bound}"
        )));
    }
    let allowed = strings(&resolution["delegated_autonomy"], "allowed_action_refs")?;
    let denied = strings(&resolution["delegated_autonomy"], "denied_action_refs")?;
    if !allowed.iter().any(|a| a == action) || denied.iter().any(|a| a == action) {
        return Err(err(format!("native authority does not allow {action}")));
    }
    let result = owner_json(
        "actuation",
        &[
            "authority".into(),
            "resolve".into(),
            "-".into(),
            "--json".into(),
        ],
        Some(resolution),
    )
    .map_err(err)?;
    if result["standing"] != "admitted" {
        return Err(err(format!(
            "Actuation refused sensing mutation: {}",
            result["refusal"]
        )));
    }
    Ok(Authority {
        actor: string(resolution, "requester_ref")?,
        reference: string(resolution, "authority_source_ref")?,
        agent: string(binding, "agent_ref")?,
    })
}

fn mutation_request(args: &Arguments) -> Result<Value, CliError> {
    read_json(
        args.request
            .as_deref()
            .ok_or_else(|| err("--request <JSON file> required"))?,
    )
}
fn expected(
    request: &Value,
    state: &FactoryDevelopmentalState,
) -> Result<(), ProjectDevelopmentStoreError> {
    if request["expected_revision"].as_u64() != Some(state.sensing.revision) {
        return Err(store_err(format!(
            "stale sensing revision: current {}; reread before mutating",
            state.sensing.revision
        )));
    }
    Ok(())
}

fn classify(args: &Arguments) -> Result<Value, CliError> {
    let request = mutation_request(args)?;
    let initial = read_developmental_state(&args.path).map_err(err)?;
    let auth = authority(
        &request,
        &world(&initial),
        "factory:action/telemetry.classify",
    )?;
    let signal_ref = string(&request, "signal_ref")?;
    let classification: Classification =
        serde_json::from_value(request["classification"].clone()).map_err(err)?;
    let evidence = strings(&request, "evidence_refs")?;
    let reason = string(&request, "reason")?;
    if classification == Classification::VerifiedDefect && evidence.is_empty() {
        return Err(err(
            "a verified-defect classification needs reproduction evidence refs",
        ));
    }
    if request.get("boundary_ref").is_some_and(|v| !v.is_null()) && evidence.is_empty() {
        return Err(err("grouping a shared boundary requires evidence refs"));
    }
    if classification == Classification::HumanDecision
        && request["decision_needed"]
            .as_str()
            .is_none_or(|s| s.trim().is_empty())
    {
        return Err(err("human-decision requires the exact decision_needed"));
    }
    transact_developmental_state(&args.path,|state|{
        expected(&request,state)?;
        let signal=state.sensing.signals.get_mut(&signal_ref).ok_or_else(||store_err("signal not found in this Project"))?;
        if request["source_revision"]!=signal.observation.source_revision {return Err(store_err("source changed: classification is bound to a stale source revision"));}
        signal.decisions.push(Decision{classification,source_revision:signal.observation.source_revision.clone(),evidence_refs:evidence,reason,decision_needed:request["decision_needed"].as_str().map(str::to_owned),boundary_ref:request["boundary_ref"].as_str().map(str::to_owned),actor_ref:auth.actor,authority_ref:auth.reference,recorded_at_unix_ms:now_ms()});
        state.sensing.revision+=1;Ok(json!({"schema":"factory.signal-decision-receipt/v1","revision":state.sensing.revision,"signal":signal,"work_created":false}))
    }).map_err(err)
}

fn commission(args: &Arguments) -> Result<Value, CliError> {
    let request = mutation_request(args)?;
    let initial = read_developmental_state(&args.path).map_err(err)?;
    let (policy, policy_ref, policy_revision) = load_policy(args, &initial)?;
    let implement = &policy
        .workflows
        .get("collect")
        .ok_or_else(|| err("collect policy missing"))?
        .implement;
    if implement.mode != "criteria" || implement.allow.is_empty() {
        return Err(err("implementation held by Project policy; criteria and an actual authority resolution are both required"));
    }
    let fulfilled = request["criteria_evidence"].as_object().ok_or_else(|| {
        err("criteria_evidence must map each configured criterion to nonempty evidence refs")
    })?;
    if implement
        .allow
        .iter()
        .chain(implement.require.iter())
        .any(|criterion| {
            fulfilled
                .get(criterion)
                .and_then(Value::as_array)
                .is_none_or(|refs| {
                    refs.is_empty()
                        || refs
                            .iter()
                            .any(|r| r.as_str().is_none_or(|s| s.trim().is_empty()))
                })
        })
    {
        return Err(err(
            "not every configured implementation criterion has explicit evidence refs",
        ));
    }
    let stopped = strings(&request, "stop_conditions_present")?;
    if stopped.iter().any(|stop| implement.stop.contains(stop)) {
        return Err(err(
            "configured human stop condition holds; use the read-only digest",
        ));
    }
    let auth = authority(
        &request,
        &policy.project_world_ref,
        "factory:action/telemetry.commission",
    )?;
    let reference = string(&request, "signal_ref")?;
    let position = string(&request, "position_ref")?;
    let initial_signal = initial
        .sensing
        .signals
        .get(&reference)
        .ok_or_else(|| err("signal not found"))?;
    // Revalidate the actual provider before creating work. Retrying an already
    // admitted commission only repairs its child NOW join and creates no work.
    if initial_signal.work.is_none() {
        crate::sensing_sources::verify_current(&initial_signal.observation, &initial, &policy)
            .map_err(err)?;
    }
    let checked_source_revision = initial_signal.observation.source_revision.clone();
    let position_read = owner_json(
        "ctrl",
        &[
            "--json".into(),
            "action".into(),
            "run".into(),
            "central.position.read".into(),
            json!({"position_ref":position}).to_string(),
        ],
        None,
    )
    .map_err(err)?;
    if position_read["ok"] != true {
        return Err(err("Central could not resolve the target World Position"));
    }
    let position_world = position_read["data"]["record"]["enclosing_world_ref"]
        .as_str()
        .unwrap_or("");
    if position_world != policy.project_world_ref && position_world != "control:root" {
        return Err(err(
            "target Position is not in this ProjectWorld or its root ancestry",
        ));
    }
    let result=transact_developmental_state(&args.path,|state|{
        if digest(&std::fs::read(&policy_ref)?)!=policy_revision {return Err(store_err("policy changed; re-evaluate implementation gates"));}
        let signal=state.sensing.signals.get(&reference).ok_or_else(||store_err("signal not found"))?.clone();
        if let Some(work)=signal.work {return Ok(json!({"schema":"factory.signal-work-receipt/v1","result":"already-applied","work":work}));}
        expected(&request,state)?;
        if signal.observation.source_revision != checked_source_revision {return Err(store_err("signal changed during provider revalidation; reread before commissioning"));}
        if signal.observation.provider_ref == "factory" {crate::sensing_sources::verify_current(&signal.observation,state,&policy).map_err(store_err)?;}
        if signal.classification()!=Classification::VerifiedDefect {return Err(store_err("only a currently verified defect may enter autonomous work; classify/reproduce first"));}
        let commission_request:crate::commission::FactoryCommissionRequest=serde_json::from_value(request["commission"].clone())?;
        if commission_request.root_act.agent_ref!=auth.agent || !commission_request.root_act.scope_refs.contains(&reference) {return Err(store_err("Commission must retain the resolved Agent and original signal in its bounded scope"));}
        let admitted=state.admit_commission(commission_request).map_err(store_err)?;
        let custody=crate::work_custody::assign_in(state,crate::work_custody::AssignRequest{
            position_ref:position.clone(),work_ref:reference.clone(),reason:format!("Verified source-qualified defect: {}",signal.observation.source_ref),run_ref:Some(admitted.commission.run_ref.clone()),journey_ref:Some(admitted.commission.journey_ref.clone()),..Default::default()
        },now_ms()).map_err(store_err)?;
        let work=WorkRelation{custody_ref:custody.custody.custody_ref,work_ref:reference.clone(),run_ref:Some(admitted.commission.run_ref.to_string()),position_ref:position,now_ref:None,authority_ref:auth.reference,created_at_unix_ms:now_ms()};
        state.sensing.signals.get_mut(&reference).ok_or_else(||store_err("signal vanished"))?.work=Some(work.clone());state.sensing.revision+=1;
        Ok(json!({"schema":"factory.signal-work-receipt/v1","result":"applied","work":work,"commission":admitted}))
    }).map_err(err)?;
    // Child NOW is an idempotent cross-owner join. A failure cannot erase
    // committed custody or cause a replacement Run on retry.
    attach_now(args, &policy.project_world_ref, &reference, result)
}

fn attach_now(
    args: &Arguments,
    project: &str,
    signal_ref: &str,
    mut result: Value,
) -> Result<Value, CliError> {
    let state = read_developmental_state(&args.path).map_err(err)?;
    if state
        .sensing
        .signals
        .get(signal_ref)
        .and_then(|s| s.work.as_ref())
        .and_then(|w| w.now_ref.as_ref())
        .is_some()
    {
        return Ok(result);
    }
    let invoke = |action: &str, input: Value| {
        owner_json(
            "ctrl",
            &[
                "--json".into(),
                "action".into(),
                "run".into(),
                action.into(),
                input.to_string(),
            ],
            None,
        )
    };
    let reading = (|| -> Result<Value, String> {
        let here = invoke("central.world.here", json!({}))?;
        let cells = here["data"]["workcells"]
            .as_array()
            .ok_or("Central did not resolve this Workcell")?;
        let current = cells
            .iter()
            .filter(|c| c["role"] == "current")
            .collect::<Vec<_>>();
        if current.len() != 1 {
            return Err(
                "Workcell identity is absent or ambiguous; no child NOW was guessed".into(),
            );
        }
        let workcell = current[0]["ref"].as_str().ok_or("Workcell ref absent")?;
        let root = invoke(
            "central.now.workcell-root",
            json!({"workcell_ref":workcell}),
        )?;
        if root["ok"] != true {
            return Err(format!("Workcell root NOW unavailable: {}", root["error"]));
        }
        let parent = root["data"]["now_ref"]
            .as_str()
            .ok_or("Workcell root NOW ref absent")?;
        let scope = if project == "control:root" {
            json!({})
        } else {
            json!({"project":project.trim_start_matches("project:")})
        };
        let policy = invoke("central.work.policy", scope)?;
        let revision = policy["data"]["revision"]
            .as_str()
            .ok_or("Central returned no current placement policy revision")?;
        let mut input = json!({"task_ref":signal_ref,"purpose":"Investigate and repair a source-qualified Factory signal","expected_policy_revision":revision,"source_refs":[signal_ref],"parent_now_ref":parent,"workcell_ref":workcell});
        if project != "control:root" {
            input["project"] = json!(project.trim_start_matches("project:"));
        }
        let allocated = invoke("central.now.allocate", input)?;
        if allocated["ok"] != true {
            return Err(format!(
                "Central child NOW allocation refused: {}",
                allocated["error"]
            ));
        }
        Ok(allocated)
    })();
    match reading {
        Ok(allocated) => {
            let now_ref = allocated["data"]["now_ref"]
                .as_str()
                .ok_or_else(|| err("Central allocation lacks NOW ref"))?
                .to_owned();
            transact_developmental_state(&args.path, |state| {
                let work = state
                    .sensing
                    .signals
                    .get_mut(signal_ref)
                    .and_then(|s| s.work.as_mut())
                    .ok_or_else(|| store_err("work no longer exists"))?;
                if work.now_ref.is_none() {
                    work.now_ref = Some(now_ref.clone());
                    state.sensing.revision += 1;
                }
                Ok(())
            })
            .map_err(err)?;
            result["now_ref"] = json!(now_ref);
        }
        Err(error) => {
            result["now_state"] = json!("unavailable");
            result["now_reason"] = json!(error);
            result["next"] = json!(
                "Retry the same commission request; the committed Run and custody are retained."
            );
        }
    }
    Ok(result)
}

fn record_return(args: &Arguments) -> Result<Value, CliError> {
    let request = mutation_request(args)?;
    let state = read_developmental_state(&args.path).map_err(err)?;
    let auth = authority(&request, &world(&state), "factory:action/telemetry.return")?;
    let reference = string(&request, "signal_ref")?;
    let outcome = string(&request, "outcome")?;
    if ![
        "implemented",
        "verified",
        "merged",
        "installed",
        "live-resolved",
        "unresolved",
    ]
    .contains(&outcome.as_str())
    {
        return Err(err(
            "unsupported Return outcome; delivery stages stay separate",
        ));
    }
    if outcome == "live-resolved"
        && [
            "installed_revision",
            "running_revision",
            "live_evidence_ref",
        ]
        .iter()
        .any(|key| request[key].as_str().is_none_or(|s| s.is_empty()))
    {
        return Err(err("live resolution needs installed/running revisions and live evidence; green checks/merge are insufficient"));
    }
    let evidence = strings(&request, "evidence_refs")?;
    if evidence.is_empty() {
        return Err(err("Return requires exact verification evidence refs"));
    }
    let merge_basis = if outcome == "merged" {
        Some(read_merge_basis(args, &state, &request)?)
    } else {
        None
    };
    let returned = SignalReturn {
        return_ref: string(&request, "return_ref")?,
        source_revision: string(&request, "source_revision")?,
        change_revision: string(&request, "change_revision")?,
        evidence_refs: evidence,
        installed_revision: request["installed_revision"].as_str().map(str::to_owned),
        running_revision: request["running_revision"].as_str().map(str::to_owned),
        live_evidence_ref: request["live_evidence_ref"].as_str().map(str::to_owned),
        outcome,
        recorded_at_unix_ms: now_ms(),
        actor_ref: auth.actor,
        authority_ref: auth.reference,
        attempt_ref: request["attempt_ref"].as_str().map(str::to_owned),
        verification_ref: request["verification_ref"].as_str().map(str::to_owned),
        merge_basis,
    };
    transact_developmental_state(&args.path,|state|{
        if let Some(previous)=state.sensing.signals.get(&reference).and_then(|s|s.returns.iter().find(|r|r.return_ref==returned.return_ref && r.outcome==returned.outcome)){
            let mut replay=returned.clone();replay.recorded_at_unix_ms=previous.recorded_at_unix_ms;
            if let (Some(basis),Some(prior))=(&mut replay.merge_basis,&previous.merge_basis){basis["observed_at_unix_ms"]=prior["observed_at_unix_ms"].clone();}
            if previous!=&replay{return Err(store_err("Return identity reused with different evidence"));}
            return Ok(json!({"schema":"factory.signal-return-receipt/v1","result":"already-applied","returned":previous}));
        }
        expected(&request,state)?;
        verify_return_basis(state,&reference,&returned)?;
        let signal=state.sensing.signals.get_mut(&reference).ok_or_else(||store_err("signal not found"))?;
        if signal.work.is_none(){return Err(store_err("Return has no originating Factory work relation"));}
        if returned.source_revision!=signal.observation.source_revision{return Err(store_err("Return source basis is stale; reread and independently verify"));}
        signal.returns.push(returned.clone());state.sensing.revision+=1;
        Ok(json!({"schema":"factory.signal-return-receipt/v1","result":"applied","signal_ref":reference,"returned":returned,"human_recognition":false}))
    }).map_err(err)
}

fn read_merge_basis(
    args: &Arguments,
    state: &FactoryDevelopmentalState,
    request: &Value,
) -> Result<Value, CliError> {
    let (policy, _, _) = load_policy(args, state)?;
    let reference = string(request, "pull_request_ref")?;
    let parts = reference
        .strip_prefix("https://github.com/")
        .ok_or_else(|| err("merged outcome requires an exact GitHub pull request URL"))?
        .split('/')
        .collect::<Vec<_>>();
    if parts.len() != 4
        || parts[2] != "pull"
        || parts[3].parse::<u64>().ok().filter(|n| *n > 0).is_none()
    {
        return Err(err("invalid GitHub pull request URL"));
    }
    let repository = format!("{}/{}", parts[0], parts[1]);
    if !policy
        .repositories
        .iter()
        .any(|r| r.provider == "github" && r.remote == repository)
    {
        return Err(err("pull request repository is outside Project policy"));
    }
    let native = owner_json(
        "gh",
        &[
            "api".into(),
            format!("repos/{repository}/pulls/{}", parts[3]),
        ],
        None,
    )
    .map_err(err)?;
    let head = string(request, "expected_pr_head")?;
    let revision = string(request, "change_revision")?;
    if native["html_url"] != reference
        || native["merged"] != true
        || native["head"]["sha"] != head
        || native["merge_commit_sha"] != revision
    {
        return Err(err("live pull request is unmerged, has another head, or merged another revision; prior check/review evidence cannot establish this outcome"));
    }
    Ok(
        json!({"source_ref":reference,"head_revision":head,"merge_revision":revision,"merged_at":native["merged_at"],"observed_at_unix_ms":now_ms(),"standing":"provider-reported"}),
    )
}

/// Re-read owner-admitted Attempt/Return/verification evidence in the same
/// transaction. Strings in the mutation request alone never resolve a signal.
fn verify_return_basis(
    state: &FactoryDevelopmentalState,
    reference: &str,
    returned: &SignalReturn,
) -> Result<(), ProjectDevelopmentStoreError> {
    let signal = state
        .sensing
        .signals
        .get(reference)
        .ok_or_else(|| store_err("signal not found"))?;
    let run = signal
        .work
        .as_ref()
        .and_then(|w| w.run_ref.as_ref())
        .ok_or_else(|| store_err("Return has no originating Factory Run"))?;
    let attempts = state
        .attempt_states
        .iter()
        .find(|(r, _)| r.to_string() == *run)
        .map(|(_, a)| a)
        .ok_or_else(|| store_err("originating Run has no native Attempt evidence"))?;
    let attempt = returned
        .attempt_ref
        .as_ref()
        .and_then(|r| attempts.attempts().get(r))
        .ok_or_else(|| store_err("attempt_ref must resolve inside the originating Run"))?;
    let readable = attempt
        .readable_return
        .as_ref()
        .filter(|r| r.return_ref == returned.return_ref)
        .ok_or_else(|| {
            store_err("return_ref must resolve to the native Attempt readable Return")
        })?;
    if !readable.evidence_refs.contains(reference)
        || !readable
            .evidence_refs
            .contains(&signal.observation.source_ref)
    {
        return Err(store_err(
            "native Return must preserve signal and original source refs",
        ));
    }
    if returned
        .evidence_refs
        .iter()
        .any(|r| !readable.evidence_refs.contains(r))
    {
        return Err(store_err(
            "Return evidence was not retained by the native Attempt",
        ));
    }
    if returned.outcome == "unresolved" {
        return Ok(());
    }
    // A later failed, unknown, or differently based receipt invalidates a
    // prior success for a new Return. The selected receipt must be current.
    let verification = attempt
        .verifications
        .last()
        .filter(|v| returned.verification_ref.as_deref() == Some(v.verification_ref.as_str()))
        .ok_or_else(|| {
            store_err("verification_ref must name the current owner-admitted receipt")
        })?;
    if verification.outcome != crate::attempt_runtime::VerificationOutcome::Passed
        || verification.source_revision != returned.change_revision
    {
        return Err(store_err(
            "verification is failed/unknown or bound to another changed revision",
        ));
    }
    if returned.outcome == "merged" {
        let merged = format!("merge:{}", returned.change_revision);
        if !verification.evidence_refs.contains(&merged) {
            return Err(store_err(
                "merged outcome requires the exact merge revision in native verification",
            ));
        }
    }
    if returned.outcome == "installed" {
        let revision = returned
            .installed_revision
            .as_deref()
            .filter(|r| !r.is_empty())
            .ok_or_else(|| store_err("installed outcome requires installed_revision"))?;
        let installed = format!("installed:{revision}");
        if !verification.evidence_refs.contains(&installed) {
            return Err(store_err(
                "installed outcome requires the exact installed revision in native verification",
            ));
        }
    }
    if returned.outcome == "live-resolved" {
        let live = returned
            .live_evidence_ref
            .as_ref()
            .ok_or_else(|| store_err("live evidence absent"))?;
        let installed = format!(
            "installed:{}",
            returned.installed_revision.as_deref().unwrap_or("")
        );
        let running = format!(
            "running:{}",
            returned.running_revision.as_deref().unwrap_or("")
        );
        if [live, &installed, &running]
            .iter()
            .any(|r| !verification.evidence_refs.contains(*r))
        {
            return Err(store_err("live resolution requires native verification of live evidence and exact installed/running revisions"));
        }
        if !readable.evidence_refs.contains(live) {
            return Err(store_err(
                "native Return has not retained the live verification evidence",
            ));
        }
    }
    Ok(())
}

fn field(args: &Arguments) -> Result<Value, CliError> {
    // Timestamp the beginning of the owner snapshot, not completion of its
    // slower occupancy reads. A delayed old snapshot must not look newer.
    let observed_at_unix_ms = now_ms();
    let state = read_developmental_state(&args.path).map_err(err)?;
    let policy_basis = policy_path(args)
        .ok()
        .filter(|p| p.is_file())
        .map(|_| load_policy(args, &state))
        .transpose()?;
    let project = world(&state);
    let all = state
        .sensing
        .signals
        .values()
        .filter(|s| s.disposition() != "live-resolved")
        .collect::<Vec<_>>();
    let signals = all
        .iter()
        .take(args.limit)
        .map(|signal| signal_summary(signal, &state))
        .collect::<Vec<_>>();
    let source_count = state
        .sensing
        .collections
        .last()
        .map(|c| c.coverage.len())
        .unwrap_or(0);
    let coverage = state
        .sensing
        .collections
        .last()
        .map(|c| c.coverage.iter().take(100).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut absences = vec![];
    if state.sensing.collections.is_empty() {
        absences.push(json!({"facet":"collection","state":"not-attempted","reason":"No source collection has been retained."}));
    }
    let mut positions = BTreeMap::new();
    let position_refs = state
        .work_custody
        .iter()
        .map(|c| c.position_ref.clone())
        .collect::<BTreeSet<_>>();
    let owner_truncated = source_count > 100
        || position_refs.len() > 30
        || state.work_custody.len() > 100
        || state
            .attempt_states
            .values()
            .map(|s| s.attempts().len())
            .sum::<usize>()
            > 100;
    if owner_truncated {
        absences.push(json!({"facet":"owner_basis","state":"truncated","reason":"Bounded projection limits: 30 Positions, 100 custody refs, 100 Attempt refs. Full evidence remains at each owner."}));
    }
    for position in position_refs.into_iter().take(30) {
        let work = crate::current_work::derive_current_work(&state, &position);
        let work_basis = json!({"outcome":work.outcome,"node_ref":work.current.as_ref().map(|w|&w.node_ref),"candidates":work.candidates.len(),"source_ref":format!("factory:current-work:{position}")});
        match owner_json(
            "actuation",
            &[
                "occupancy".into(),
                "read".into(),
                "--position".into(),
                position.clone(),
                "--json".into(),
            ],
            None,
        ) {
            Ok(value) => {
                positions.insert(position,json!({"revision":digest(value.to_string().as_bytes()),"state":value["state"],"generation_ref":value["current"]["generation_ref"],"current_work":work_basis}));
            }
            Err(error) => {
                absences.push(json!({"facet":"occupancy","source_ref":position,"state":"unavailable","reason":error}));
                positions.insert(
                    position,
                    json!({"state":"unavailable","current_work":work_basis}),
                );
            }
        }
    }
    let encoded = serde_json::to_vec(&state).map_err(err)?;
    let revision = digest(&encoded);
    let owner_basis = json!({"factory_state_ref":format!("factory:developmental-state:{}",state.project_ref()),"factory_state_revision":revision,"policy":policy_basis.as_ref().map(|(_,path,rev)|json!({"source_ref":path,"source_revision":rev})),"build_revision":state.build.revision().get(),"positions":positions,"custody_refs":state.work_custody.iter().take(100).map(|c|c.custody_ref.clone()).collect::<Vec<_>>(),"attempt_refs":state.attempt_states.values().flat_map(|s|s.attempts().keys().cloned()).take(100).collect::<Vec<_>>()});
    let cursor = digest(
        json!({"state":revision,"owners":owner_basis})
            .to_string()
            .as_bytes(),
    );
    Ok(
        json!({"schema":FIELD_SCHEMA,"project_world_ref":project,"source_revision":revision,"source_sequence":state.sensing.revision,"observed_at_unix_ms":observed_at_unix_ms,"signals":signals,"coverage":coverage,"owner_basis":owner_basis,"absences":absences,"cursor":cursor,"counts":{"signals":all.len(),"custody":state.work_custody.len(),"affected_people":null,"affected_sessions":null},"truncated":all.len()>args.limit || owner_truncated}),
    )
}

fn signal_summary(signal: &Signal, state: &FactoryDevelopmentalState) -> Value {
    let work = signal.work.as_ref();
    let custody = work.and_then(|w| current_custody(state, &w.custody_ref));
    json!({"signal_ref":signal.signal_ref,"source_refs":[signal.observation.source_ref],"relation_refs":signal.observation.relation_refs,"source_revision":signal.observation.source_revision,"summary":signal.observation.summary,"dimension":signal.observation.dimension,"standing":signal.observation.standing,"classification":signal.classification(),"disposition":signal.disposition(),"decision_needed":signal.decision().and_then(|d|d.decision_needed.as_ref()),"work_ref":work.map(|w|&w.work_ref),"run_ref":work.and_then(|w|w.run_ref.as_ref()),"custody_ref":custody.map(|c|&c.custody_ref),"origin_custody_ref":work.map(|w|&w.custody_ref),"custody_state":custody.map(|c|c.state),"custody_state_basis":"current owner state","position_ref":custody.map(|c|&c.position_ref),"now_ref":work.and_then(|w|w.now_ref.as_ref()),"return_ref":signal.returns.last().map(|r|&r.return_ref),"occurred_at_unix_ms":signal.observation.occurred_at_unix_ms,"updated_at_unix_ms":signal.decisions.last().map(|d|d.recorded_at_unix_ms).unwrap_or(signal.observation.observed_at_unix_ms)})
}

fn current_custody<'a>(
    state: &'a FactoryDevelopmentalState,
    initial: &str,
) -> Option<&'a crate::work_custody::FactoryWorkCustody> {
    let mut reference = initial;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(reference.to_owned()) {
            return None;
        }
        let current = state
            .work_custody
            .iter()
            .find(|c| c.custody_ref == reference)?;
        if current.state == crate::work_custody::CustodyState::HandedOff {
            reference = &current.handed_off_to.as_ref()?.custody_ref;
        } else {
            return Some(current);
        }
    }
}

fn reading(operation: &str, args: &Arguments) -> Result<Value, CliError> {
    let state = read_developmental_state(&args.path).map_err(err)?;
    if args.policy.is_some() {
        load_policy(args, &state)?;
    }
    if operation == "signal" {
        let reference = args
            .subject
            .as_ref()
            .ok_or_else(|| err("signal ref is required"))?;
        let signal = state
            .sensing
            .signals
            .get(reference)
            .ok_or_else(|| err("signal not found in this ProjectWorld"))?;
        return Ok(
            json!({"schema":"factory.signal-reading/v1","project_world_ref":world(&state),"revision":state.sensing.revision,"signal":signal,"summary":signal_summary(signal,&state)}),
        );
    }
    let historical = state
        .sensing
        .signals
        .values()
        .filter_map(|signal| signal_at(signal, args.window.until_unix_ms, operation == "day"))
        .collect::<Vec<_>>();
    let in_window = |s: &Signal| {
        args.window.contains(
            s.observation
                .occurred_at_unix_ms
                .unwrap_or(s.observation.observed_at_unix_ms),
        ) || s
            .decisions
            .iter()
            .any(|d| args.window.contains(d.recorded_at_unix_ms))
            || s.work
                .as_ref()
                .is_some_and(|work| args.window.contains(work.created_at_unix_ms))
            || s.returns
                .iter()
                .any(|r| args.window.contains(r.recorded_at_unix_ms))
    };
    // Human decisions remain current until resolved; a daily digest must not
    // make an unanswered question vanish when its occurrence ages out.
    let candidates = if operation == "signals" || operation == "digest" {
        state.sensing.signals.values().collect::<Vec<_>>()
    } else {
        historical
            .iter()
            .filter(|s| in_window(s))
            .collect::<Vec<_>>()
    };
    let selected = candidates
        .iter()
        .copied()
        .filter(|s| {
            operation != "digest"
                || (s.disposition() != "live-resolved"
                    && s.classification() == Classification::HumanDecision)
        })
        .collect::<Vec<_>>();
    let boundary = |s: &Signal| {
        s.decision()
            .and_then(|d| d.boundary_ref.clone())
            .unwrap_or_else(|| s.signal_ref.clone())
    };
    let mut groups: BTreeMap<String, Vec<&Signal>> = BTreeMap::new();
    for signal in &selected {
        groups.entry(boundary(signal)).or_default().push(signal);
    }
    let patterns=groups.into_iter().map(|(key,signals)|{
        // Prior fixes may predate the requested window. Use explicit source
        // boundary relations across history rather than only selected rows.
        let related=historical.iter().filter(|s|boundary(s)==key).collect::<Vec<_>>();
        let prior_fixes=related.iter().flat_map(|s|s.returns.iter()).filter(|r|r.outcome=="live-resolved").collect::<Vec<_>>();
        let recurrence=signals.iter().any(|signal| signal.observation.occurred_at_unix_ms.is_some_and(|t|prior_fixes.iter().any(|fix|t>fix.recorded_at_unix_ms)));
        json!({"boundary_ref":key,"signal_refs":signals.iter().map(|s|&s.signal_ref).collect::<Vec<_>>(),"source_refs":signals.iter().map(|s|&s.observation.source_ref).collect::<Vec<_>>(),"records":signals.len(),"affected_users":null,"affected_sessions":null,"identity_basis":"source record identity; no person/session denominator supplied","recurrence_after_live_fix":recurrence,"prior_fix_refs":prior_fixes.iter().map(|r|&r.return_ref).collect::<Vec<_>>(),"prior_work_refs":related.iter().filter_map(|s|s.work.as_ref().map(|w|&w.work_ref)).collect::<Vec<_>>(),"grouping_basis":"explicit evidence-qualified boundary relation; never text similarity"})
    }).collect::<Vec<_>>();
    let coverage = state
        .sensing
        .collections
        .iter()
        .flat_map(|collection| {
            collection.coverage.iter().map(move |receipt| {
                let mut observed = receipt.clone();
                observed.query_basis["requested_until_unix_ms"] =
                    json!(receipt.window.until_unix_ms);
                observed.window.until_unix_ms = observed
                    .window
                    .until_unix_ms
                    .min(collection.recorded_at_unix_ms);
                observed
            })
        })
        .filter(|c| {
            c.window.since_unix_ms < args.window.until_unix_ms
                && c.window.until_unix_ms > args.window.since_unix_ms
        })
        .collect::<Vec<_>>();
    let carried = if operation == "day" {
        historical
            .iter()
            .filter(|s| s.work.is_some() && s.disposition() != "live-resolved")
            .map(|s| s.signal_ref.clone())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    let active_policy = policy_path(args)
        .ok()
        .filter(|p| p.is_file())
        .map(|_| load_policy(args, &state))
        .transpose()?;
    let expected_sources = active_policy.as_ref().and_then(|(p, _, _)| {
        p.workflows.get("collect").map(|f| {
            f.sources
                .iter()
                .filter_map(|id| p.sources.iter().find(|s| &s.id == id))
                .collect::<Vec<_>>()
        })
    });
    let missing_sources = expected_sources
        .as_ref()
        .map(|sources| {
            sources
                .iter()
                .filter(|source| {
                    !coverage
                        .iter()
                        .any(|c| c.source_ref == source.source_ref && c.scope == source.scope)
                })
                .map(|s| s.source_ref.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let complete = missing_sources.is_empty()
        && coverage_complete(&coverage.iter().collect::<Vec<_>>(), &args.window);
    let comparison = if let Some((window, basis)) = &args.previous {
        let mut previous = args.clone();
        previous.previous = None;
        previous.window = window.clone();
        previous.window_basis = basis.clone();
        let reading = reading(operation, &previous)?;
        Some(
            json!({"basis":"equal number of preceding civil Days","window":reading["window"],"civil_basis":reading["civil_basis"],"counts":reading["counts"],"patterns":reading["patterns"],"source_coverage_complete":reading["source_coverage_complete"]}),
        )
    } else {
        None
    };
    Ok(
        json!({"schema":format!("factory.telemetry-{operation}/v1"),"project_world_ref":world(&state),"revision":state.sensing.revision,"read_only":true,"window":args.window,"civil_basis":args.window_basis,"signals":selected.iter().take(args.limit).map(|s|signal_summary(s,&state)).collect::<Vec<_>>(),"patterns":patterns,"coverage":coverage,"counts":{"records":selected.len(),"affected_users":null,"affected_sessions":null},"carried_signal_refs":carried,"truncated":selected.len()>args.limit,"source_coverage_complete":complete,"coverage_basis":if expected_sources.is_some(){"current policy source set"}else{"retained collection source sets; current policy unavailable"},"missing_source_refs":missing_sources,"comparison":comparison,"historical_basis":if operation=="day"{"observed by end of Day; native admission times; custody status explicitly current"}else{"retrospective source occurrence times as observed now; native admission times; custody status explicitly current"},"human_day_prose_changed":false}),
    )
}

fn signal_at(signal: &Signal, until: i64, observed_as_of: bool) -> Option<Signal> {
    let observation = signal
        .prior_observations
        .iter()
        .chain(std::iter::once(&signal.observation))
        .filter(|o| {
            if observed_as_of {
                o.observed_at_unix_ms < until
            } else {
                o.occurred_at_unix_ms.unwrap_or(o.observed_at_unix_ms) < until
            }
        })
        .max_by_key(|o| o.observed_at_unix_ms)?;
    let mut reading = signal.clone();
    reading.observation = observation.clone();
    reading.decisions.retain(|d| d.recorded_at_unix_ms < until);
    reading.returns.retain(|r| r.recorded_at_unix_ms < until);
    if reading
        .work
        .as_ref()
        .is_some_and(|w| w.created_at_unix_ms >= until)
    {
        reading.work = None;
    }
    Some(reading)
}

fn coverage_complete(coverage: &[&Coverage], window: &Interval) -> bool {
    let mut sources: BTreeMap<(&str, &str), Vec<(i64, i64)>> = BTreeMap::new();
    for receipt in coverage {
        let intervals = sources
            .entry((&receipt.source_ref, &receipt.scope))
            .or_default();
        if matches!(
            receipt.state,
            CoverageState::Complete | CoverageState::Empty
        ) {
            intervals.push((receipt.window.since_unix_ms, receipt.window.until_unix_ms));
        }
    }
    !sources.is_empty()
        && sources.values_mut().all(|intervals| {
            intervals.sort_unstable();
            let mut covered = window.since_unix_ms;
            for (start, end) in intervals {
                if *start > covered {
                    break;
                }
                covered = covered.max(*end);
            }
            covered >= window.until_unix_ms
        })
}

fn render(document: &Value) -> String {
    let mut out = format!(
        "{}\nWorld: {}\n",
        document["schema"].as_str().unwrap_or("Factory sensing"),
        document["project_world_ref"]
            .as_str()
            .unwrap_or("see owner receipt")
    );
    if let Some(signals) = document["signals"].as_array() {
        for signal in signals {
            out.push_str(&format!(
                "\n{} — {}\n  {}\n",
                signal["classification"].as_str().unwrap_or("signal"),
                signal["summary"].as_str().unwrap_or(""),
                signal["signal_ref"].as_str().unwrap_or("")
            ));
        }
    }
    if let Some(coverage) = document["collection"]["coverage"]
        .as_array()
        .or(document["coverage"].as_array())
    {
        for source in coverage {
            out.push_str(&format!(
                "Source {}: {} ({} records)\n",
                source["source_ref"].as_str().unwrap_or(""),
                source["state"].as_str().unwrap_or("unknown"),
                source["records"]
            ));
        }
    }
    if document["truncated"] == true {
        out.push_str("Partial reading: display limit reached.\n");
    }
    out
}
