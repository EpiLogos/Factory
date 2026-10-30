//! A Run's team steps read their joins from Central's agent sets as the Run starts (O-I #684, DLW manual §19.15).
//!
//! Central keeps how a set's members' returns combine (`central.agent-set/v1` `join`); the Agentset rack reads and saves
//! it there. Factory reads it through the host's own Central binding (`FACTORY_NATIVE_CENTRAL_BINARY`,
//! `FACTORY_NATIVE_CENTRAL_ROOT`), with `central.agent-set.list` at root scope, the scope the rack saves at, and keeps
//! it with the Run (`ExecutableOrchestration::read_step_joins`). A host that binds no Central reads no join, and its
//! team steps keep the barrier's own rule, All.
use crate::orchestration::{BarrierJoin, SetJoinReading, StepJoinSource};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const LIST: &str = "central.agent-set.list";

#[derive(Debug, Clone)]
pub struct CentralStepJoins {
    binary: PathBuf,
    root: PathBuf,
}

impl CentralStepJoins {
    /// The host's Central binding. `Ok(None)` when the host binds none; an error when it binds one that is not usable,
    /// which refuses the start rather than starting unread.
    pub fn configured() -> Result<Option<Self>, String> {
        if std::env::var_os("FACTORY_NATIVE_CENTRAL_BINARY").is_none()
            && std::env::var_os("FACTORY_NATIVE_CENTRAL_ROOT").is_none()
        {
            return Ok(None);
        }
        let endpoint = crate::attempt_native_receiving::configured_endpoint()?;
        Ok(Some(Self {
            binary: endpoint.binary,
            root: endpoint.root,
        }))
    }

    pub fn at(binary: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            root: root.into(),
        }
    }
}

impl StepJoinSource for CentralStepJoins {
    fn read_set_join(&self, agent_set_ref: &str) -> Result<Option<SetJoinReading>, String> {
        let mut command = Command::new(&self.binary);
        command
            .arg("--json")
            .arg("--root")
            .arg(&self.root)
            .args(["action", "run", LIST])
            .arg(json!({"scope": "root"}).to_string());
        let output = crate::native_process::output(&mut command, Duration::from_secs(30)).map_err(
            |error| {
                format!(
                    "Central could not be run at {}: {error}",
                    self.binary.display()
                )
            },
        )?;
        set_join_from_list(&output.stdout, agent_set_ref)
    }
}

/// The join a set names, from Central's `central.agent-set.list` answer. `Ok(None)` when Central keeps no set of that ref.
pub fn set_join_from_list(
    stdout: &[u8],
    agent_set_ref: &str,
) -> Result<Option<SetJoinReading>, String> {
    // A workflow names a set with Factory's qualification (`agent-set:<ref>`); Central keeps it under its own ref.
    let central_ref = agent_set_ref
        .strip_prefix("agent-set:")
        .unwrap_or(agent_set_ref);
    let answer: Value = serde_json::from_slice(stdout)
        .map_err(|error| format!("Central's answer to {LIST} is not JSON: {error}"))?;
    if answer["ok"] != true || answer["status"] != "success" || answer["action"] != LIST {
        return Err(format!(
            "Central did not answer {LIST}: {}",
            answer["error"]["message"]
                .as_str()
                .unwrap_or("it gave no reason")
        ));
    }
    let records = answer["data"]["records"]
        .as_array()
        .ok_or_else(|| format!("Central's answer to {LIST} holds no records"))?;
    let Some(found) = records.iter().find(|record| record["ref"] == central_ref) else {
        return Ok(None);
    };
    let revision = found["revision"]
        .as_str()
        .filter(|revision| !revision.trim().is_empty())
        .ok_or_else(|| format!("Central's set {agent_set_ref} names no revision"))?;
    let join = match found["record"].get("join") {
        None | Some(Value::Null) => None,
        Some(join) => Some(
            serde_json::from_value::<BarrierJoin>(join.clone()).map_err(|_| {
                format!("Central's set {agent_set_ref} names a join Factory does not know: {join}")
            })?,
        ),
    };
    Ok(Some(SetJoinReading {
        agent_set_ref: agent_set_ref.into(),
        revision: revision.into(),
        join,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sets_join_is_read_from_centrals_list_and_an_absent_set_reads_none() {
        let list = json!({"ok": true, "status": "success", "action": LIST, "data": {"records": [
            {"kind": "agent-set", "ref": "anima", "revision": "r4", "source_path": "x",
             "record": {"ref": "anima", "revision": "r4", "join": "first", "members": []}},
            {"kind": "agent-set", "ref": "aletheia", "revision": "r1", "source_path": "y",
             "record": {"ref": "aletheia", "revision": "r1", "members": []}}]}})
        .to_string();
        assert_eq!(
            set_join_from_list(list.as_bytes(), "anima").unwrap(),
            Some(SetJoinReading {
                agent_set_ref: "anima".into(),
                revision: "r4".into(),
                join: Some(BarrierJoin::First),
            })
        );
        assert_eq!(
            set_join_from_list(list.as_bytes(), "aletheia")
                .unwrap()
                .unwrap()
                .join,
            None
        );
        // A workflow names the set with Factory's qualification; Central keeps it under its own ref.
        assert_eq!(
            set_join_from_list(list.as_bytes(), "agent-set:anima")
                .unwrap()
                .map(|read| (read.agent_set_ref, read.join)),
            Some(("agent-set:anima".to_string(), Some(BarrierJoin::First)))
        );
        assert_eq!(set_join_from_list(list.as_bytes(), "absent").unwrap(), None);
        let refused = json!({"ok": false, "status": "invalid_input", "action": LIST,
            "error": {"message": "scope must be root or project"}})
        .to_string();
        assert!(set_join_from_list(refused.as_bytes(), "anima")
            .unwrap_err()
            .contains("scope must be root or project"));
    }
}
