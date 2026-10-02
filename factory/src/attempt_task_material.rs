//! Native prepared-Run material joins are read on the actual Task owner hop.
//! Receipts and observations are sequential previews; AIKit's locked admission
//! and Workcell scope validation still execute before the worker effect.
use crate::attempt_runtime::{FactoryAttemptReading, FactoryAttemptRecord};
use crate::core::run::WorkflowUnitRef;
use crate::native_aikit_route::{material_output, AikitOwnerTransport};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Correlations from the actual retained attempt. These fields make no model,
/// reservation, dispatch or completion claim of their own.
pub(super) struct MaterialAttemptBasis<'a> {
    pub workflow_unit_ref: &'a WorkflowUnitRef,
    pub task_ref: &'a str,
    pub agent_ref: &'a str,
    pub agency_ref: &'a str,
    pub world_binding_ref: &'a str,
    pub agent_session_ref: &'a str,
    pub session_space_ref: &'a str,
}

impl<'a> From<&'a FactoryAttemptRecord> for MaterialAttemptBasis<'a> {
    fn from(attempt: &'a FactoryAttemptRecord) -> Self {
        Self {
            workflow_unit_ref: &attempt.workflow_unit_ref,
            task_ref: &attempt.task_ref,
            agent_ref: &attempt.disposition.participant.agent_ref,
            agency_ref: &attempt.disposition.participant.agency_ref,
            world_binding_ref: &attempt.disposition.participant.world_binding_ref,
            agent_session_ref: &attempt.disposition.body.agent_session_ref,
            session_space_ref: &attempt.disposition.body.session_space_ref,
        }
    }
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Native prepared material omitted {field}"))
}

fn call(
    binary: &Path,
    transport: Option<&AikitOwnerTransport>,
    arguments: &[String],
    deadline: Instant,
) -> Result<Value, String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or("Native prepared material read budget exhausted")?;
    let output = material_output(binary, transport, arguments, remaining)?;
    let bounded =
        |bytes: &[u8]| String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    if !output.status.success() {
        return Err(format!(
            "Native Workcell material read refused; exit {:?}; stderr {}; response {}",
            output.status.code(),
            bounded(&output.stderr),
            bounded(&output.stdout)
        ));
    }
    let response: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Native Workcell material read is not JSON: {e}"))?;
    if response["ok"] != true {
        return Err(format!(
            "Native Workcell material read did not succeed: {}",
            bounded(&output.stdout)
        ));
    }
    Ok(response)
}

pub(super) fn inspect(
    current: &FactoryAttemptReading,
    attempt: &MaterialAttemptBasis<'_>,
    task: &Value,
    transport: Option<&AikitOwnerTransport>,
    deadline: Instant,
) -> Result<Option<Value>, String> {
    if !current.source_current {
        return Err(
            "WorkflowSource is no longer current; re-resolve before material admission".into(),
        );
    }
    let run_ref = current.run_ref.to_string();
    let prepared = &task["prepared_run"];
    if prepared.is_null() {
        return Ok(None);
    }
    let executable = Path::new(text(prepared, "executable")?);
    let state_root = text(prepared, "state_root")?;
    if !executable.is_absolute() || !Path::new(state_root).is_absolute() {
        return Err("Prepared native material requires absolute owner/state paths".into());
    }
    let scope = &prepared["scope"];
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Native material admission clock unavailable: {e}"))?
        .as_millis();
    if task["requirements"]["expires_at_unix_ms"]
        .as_u64()
        .is_none_or(|expiry| u128::from(expiry) <= now)
    {
        return Err(
            "Native Task boundary lease is absent or expired; recover the same preparation".into(),
        );
    }
    let slug = text(scope, "run_slug")?;
    let show_args = vec![
        "--state-root".into(),
        state_root.into(),
        "--json".into(),
        "run".into(),
        "show".into(),
        "--run".into(),
        slug.into(),
    ];
    let show = call(executable, transport, &show_args, deadline)?;
    let run = &show["run"];
    if scope["schema"] != "workcell.prepared-run-scope/v1"
        || run["schema"] != "workcell.run/v1"
        || run["canonical_run_ref"] != run_ref
        || run["run_slug"] != slug
        || show["run_revision"] != scope["run_revision"]
        || run["demand_digest"] != scope["demand_digest"]
        || !matches!(
            run["execution_status"].as_str(),
            Some("running" | "blocked")
        )
        || run["agency"] != scope["agency"]
        || run["operative"] != scope["operative"]
        || run["boundary_digest"] != task["inspection"]["requirements_digest"]
        || scope["prepared_write_boundary"]["requirements"] != task["requirements"]
        || scope["prepared_write_boundary"]["requirements_digest"]
            != task["inspection"]["requirements_digest"]
        || scope["prepared_write_boundary"] != task["inspection"]
        || scope["worktree_path"] != task["request"]["cwd"]
    {
        return Err("Native Workcell Run/scope changed or belongs to another undertaking; re-resolve before dispatch".into());
    }
    let receipt = text(run, "world_receipt")?;
    let reading = call(
        executable,
        transport,
        &[
            "--state-root".into(),
            state_root.into(),
            "--receipt".into(),
            receipt.into(),
            "--json".into(),
            "inspect".into(),
        ],
        deadline,
    )?;
    let world = &reading["receipt_world"];
    if reading["contract"] != "workcell.material-reading/v1"
        || reading["observation"]["status"] != "supplied"
        || world["version"] != "workcell.material-world/v1"
        || world["world_ref"] != run["world_ref"]
        || world["subjects"]["factory_run"] != run_ref
        || world["subjects"]["workflow_source"] != current.workflow_source_ref
        || world["subjects"]["workflow_source_revision"] != current.workflow_source_revision
        || world["subjects"]["workflow_source_digest"] != current.workflow_source_digest
        || world["subjects"]["workflow_unit"] != json!(attempt.workflow_unit_ref)
        || world["subjects"]["central_task"] != attempt.task_ref
        || world["subjects"]["agent"] != attempt.agent_ref
        || world["subjects"]["agency"] != attempt.agency_ref
        || world["subjects"]["world_binding"] != attempt.world_binding_ref
        || world["subjects"]["agent_session"] != attempt.agent_session_ref
        || world["subjects"]["session_space"] != attempt.session_space_ref
        || transport.is_some_and(|route| world["workcell_ref"] != route.declared_workcell())
    {
        return Err("Native material does not retain this exact Run/unit/Task/Agency/session/Workcell basis".into());
    }
    // A supplied observation is transport, not health. Join the selected source
    // binding to its fresh physical observation and the actual boundary object.
    let selected = text(scope, "workspace_material_ref")?;
    let (binding, observed) = selected_source(selected, &scope["worktree_path"], world, &reading)?;
    let object = task["inspection"]["objects"]
        .as_array()
        .and_then(|objects| {
            let selected: Vec<_> = objects
                .iter()
                .filter(|object| object["path"] == scope["worktree_path"])
                .collect();
            (selected.len() == 1).then(|| selected[0])
        })
        .ok_or("Native boundary omitted a unique physical source object")?;
    let identity = text(object, "identity")?;
    let stored_identity = binding["properties"]["object_identity"].as_str();
    let object_matches = match stored_identity {
        Some(stored) => stored == identity && observed["detail"]["object_identity"] == stored,
        // The native Git workspace contract exposes path/commit/health rather
        // than an inode. Keep its actual prepared boundary object and native
        // launch revalidation; never invent a fresh object observation.
        None => {
            binding["port"] == "workspace"
                && binding["provenance"]["provider_kind"] == "git-worktree"
                && observed["detail"]["object_identity"].is_null()
        }
    };
    if !object_matches {
        return Err("Native source material is unavailable or its current physical object differs from the prepared Task boundary".into());
    }
    // Inspect is sequential, and its storage observation is not a lease. The
    // same native Run must still retain the admitted scope after those reads.
    if call(executable, transport, &show_args, deadline)? != show {
        return Err(
            "Native material Run changed during inspection; keep current work unresolved".into(),
        );
    }
    Ok(Some(
        json!({"preparedRun":prepared,"runReading":show,"materialReading":reading,
        "standing":"native sequential material preview; locked task/scope admission still required"}),
    ))
}

/// Join actual provider observations to the selected binding. Supplied JSON is
/// not evidence that a required native resource is available.
pub(super) fn selected_source<'a>(
    selected: &str,
    path: &Value,
    world: &'a Value,
    reading: &'a Value,
) -> Result<(&'a Value, &'a Value), String> {
    let bindings = world["binding_graph"]["bindings"]
        .as_array()
        .ok_or("Native material omitted its binding graph")?;
    let candidates: Vec<_> = bindings
        .iter()
        .filter(|binding| binding["material_ref"] == selected)
        .collect();
    if candidates.len() != 1 {
        return Err("Native prepared source material has no unique actual binding".into());
    }
    let binding = candidates[0];
    let logical = text(binding, "logical_ref")?;
    let observations = reading["observation"]["reading"]["observations"]
        .as_array()
        .ok_or("Native material omitted fresh observations")?;
    let current: Vec<_> = observations
        .iter()
        .filter(|observation| observation["logical_ref"] == logical)
        .collect();
    if current.len() != 1 {
        return Err("Native prepared source material has no unique fresh observation".into());
    }
    let observed = current[0];
    if binding["necessity"] != "required"
        || binding["presence"] != "present"
        || binding["properties"]["path"] != *path
        || observed["state"] != "healthy"
        || observed["detail"]["material_ref"] != selected
        || observed["detail"]["path"] != *path
    {
        return Err("Native selected source material is unavailable or changed".into());
    }
    Ok((binding, observed))
}
