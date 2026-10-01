//! One declared owner route for Task inspection and Encounter effects. SSH is
//! transport to the same native AIKit owner, not a Gateway admission or worker.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

/// Configuration refusal is distinct from failure of the actual owner client.
/// Preserve the native I/O kind and client identity for existing public callers.
#[derive(Debug)]
pub enum AikitOwnerRouteError {
    InvalidConfiguration(String),
    ClientIo { binary: PathBuf, error: io::Error },
}

impl Display for AikitOwnerRouteError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => formatter.write_str(message),
            Self::ClientIo { binary, error } => write!(
                formatter,
                "Native AIKit owner client {}: {error}",
                binary.display()
            ),
        }
    }
}

impl std::error::Error for AikitOwnerRouteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ClientIo { error, .. } => Some(error),
            Self::InvalidConfiguration(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AikitOwnerTransport {
    Local {
        workcell_ref: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        environment: BTreeMap<String, String>,
    },
    Ssh {
        ssh_binary: PathBuf,
        target: String,
        workcell_ref: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        environment: BTreeMap<String, String>,
    },
}

fn path_text(path: &Path) -> Result<&str, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("Native AIKit route requires exact absolute binary/cwd paths".into());
    }
    path.to_str()
        .filter(|s| !s.contains('\0'))
        .ok_or_else(|| "Native owner path must be UTF-8 without NUL".into())
}

fn validate_environment(
    workcell: &str,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    if !workcell.starts_with("workcell:")
        || workcell.chars().any(char::is_whitespace)
        || workcell.contains('\0')
    {
        return Err("Declared native owner route requires its actual Workcell reference".into());
    }
    for (key, value) in environment {
        // Hosting/discovery only. Credentials and native authority remain at
        // the owner; they may never become retained transport configuration.
        if !matches!(key.as_str(), "WORKCELL_HOME" | "PATH")
            || value.is_empty()
            || value.contains('\0')
        {
            return Err("Native owner environment allows only nonempty WORKCELL_HOME/PATH; never credentials".into());
        }
        if key == "WORKCELL_HOME" {
            path_text(Path::new(value))?;
        } else if value
            .split(':')
            .any(|entry| path_text(Path::new(entry)).is_err())
        {
            return Err("Declared native owner PATH must contain only absolute directories".into());
        }
    }
    Ok(())
}

fn quote(value: &str) -> Result<String, String> {
    if value.contains('\0') {
        return Err("Native owner argument contains NUL".into());
    }
    Ok(format!("'{}'", value.replace('\'', "'\"'\"'")))
}

/// SSH joins remote argv through a shell. Encode every individual argument,
/// including serialized assertions, rather than interpolating command text.
fn remote_command(arguments: &[String]) -> Result<String, String> {
    let quoted: Result<Vec<_>, _> = arguments.iter().map(|value| quote(value)).collect();
    Ok(format!("exec {}", quoted?.join(" ")))
}

fn owner_command(
    binary: &Path,
    transport: Option<&AikitOwnerTransport>,
    arguments: &[String],
) -> Result<Command, String> {
    match transport {
        None => {
            let mut command = Command::new(binary);
            command.args(arguments);
            Ok(command)
        }
        Some(AikitOwnerTransport::Local {
            workcell_ref,
            environment,
        }) => {
            path_text(binary)?;
            validate_environment(workcell_ref, environment)?;
            let mut command = Command::new(binary);
            command.args(arguments).envs(environment);
            Ok(command)
        }
        Some(AikitOwnerTransport::Ssh {
            ssh_binary,
            target,
            workcell_ref,
            environment,
        }) => {
            let binary_text = path_text(binary)?;
            path_text(ssh_binary)?;
            validate_environment(workcell_ref, environment)?;
            if target.is_empty()
                || target.len() > 512
                || target.starts_with('-')
                || !target
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._@:-[]".contains(&byte))
            {
                return Err(
                    "SSH owner target must be an explicit host/user, never an option or command"
                        .into(),
                );
            }
            let mut remote = vec!["env".to_owned()];
            remote.extend(
                environment
                    .iter()
                    .map(|(key, value)| format!("{key}={value}")),
            );
            remote.push(binary_text.to_owned());
            remote.extend_from_slice(arguments);
            let mut command = Command::new(ssh_binary);
            command
                .env_remove("CENTRAL_NATIVE_TOKEN")
                .env_remove("ACTUATION_GATEWAY_TOKEN");
            command
                .args([
                    "-T",
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=10",
                    "--",
                    target,
                ])
                .arg(remote_command(&remote)?);
            Ok(command)
        }
    }
}

pub(crate) fn command(
    binary: &Path,
    cwd: &Path,
    transport: Option<&AikitOwnerTransport>,
    arguments: &[String],
) -> Result<Command, String> {
    let cwd_text = cwd.to_str().ok_or("Task cwd is not UTF-8")?;
    if transport.is_some() {
        path_text(cwd)?;
    }
    let mut owner_arguments = vec!["-C".into(), cwd_text.into()];
    owner_arguments.extend_from_slice(arguments);
    owner_command(binary, transport, &owner_arguments)
}

/// Related native material reads use exactly the same hop and hosting
/// declaration as Task inspection and Encounter delivery.
pub(crate) fn material_output(
    binary: &Path,
    transport: Option<&AikitOwnerTransport>,
    arguments: &[String],
    timeout: Duration,
) -> Result<Output, String> {
    crate::native_process::output(&mut owner_command(binary, transport, arguments)?, timeout)
        .map_err(|error| format!("Native Workcell owner transport failed: {error}"))
}

impl AikitOwnerTransport {
    pub(crate) fn declared_workcell(&self) -> &str {
        match self {
            Self::Local { workcell_ref, .. } | Self::Ssh { workcell_ref, .. } => workcell_ref,
        }
    }
}

pub fn output(
    binary: &Path,
    cwd: &Path,
    transport: Option<&AikitOwnerTransport>,
    arguments: &[String],
    timeout: Duration,
) -> Result<Output, AikitOwnerRouteError> {
    let mut command = command(binary, cwd, transport, arguments)
        .map_err(AikitOwnerRouteError::InvalidConfiguration)?;
    let client = PathBuf::from(command.get_program());
    crate::native_process::output(&mut command, timeout).map_err(|error| {
        AikitOwnerRouteError::ClientIo {
            binary: client,
            error,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_arguments_preserve_assertion_bytes_without_shell_effects() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("must-not-exist");
        let assertion = serde_json::json!({"expected_task":{"revision":"current'\n$HOME"},
            "text":format!("$(touch {}) `touch {}` ; '\n", marker.display(), marker.display())})
        .to_string();
        let shell = remote_command(&["printf".into(), "%s".into(), assertion.clone()]).unwrap();
        let result = crate::native_process::output(
            Command::new("/bin/sh").args(["-c", &shell]),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, assertion.as_bytes());
        assert!(!marker.exists());
    }

    #[test]
    fn credentials_options_and_relative_paths_refuse_before_transport() {
        let mut route = AikitOwnerTransport::Ssh {
            ssh_binary: "/usr/bin/ssh".into(),
            target: "host".into(),
            workcell_ref: "workcell:test".into(),
            environment: BTreeMap::from([("CENTRAL_NATIVE_TOKEN".into(), "never-forward".into())]),
        };
        let refused = command(
            Path::new("/native/aikit"),
            Path::new("/source"),
            Some(&route),
            &[],
        )
        .unwrap_err();
        assert!(refused.contains("never credentials"));
        if let AikitOwnerTransport::Ssh {
            target,
            environment,
            ..
        } = &mut route
        {
            environment.clear();
            *target = "-oProxyCommand=unexpected".into();
        }
        assert!(command(
            Path::new("/native/aikit"),
            Path::new("/source"),
            Some(&route),
            &[]
        )
        .is_err());
        assert!(command(
            Path::new("relative"),
            Path::new("/source"),
            Some(&route),
            &[]
        )
        .is_err());
    }
}
