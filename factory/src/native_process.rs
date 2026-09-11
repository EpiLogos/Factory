//! Bounded transport to an owner client. Killing this client is deliberately
//! NOT evidence that a remote worker stopped or its effects were rolled back.

use std::io::{self, Read};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

pub const MAX_OWNER_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

fn read_pipe(mut pipe: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.by_ref()
        .take(MAX_OWNER_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OWNER_OUTPUT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native owner output exceeded its byte bound; effects may be unknown",
        ));
    }
    Ok(bytes)
}

fn stop_client(mut child: Child) {
    let _ = child.kill();
    // A surviving descendant can hold inherited pipes open. Do not join pipe
    // readers or wait indefinitely for client teardown on the caller's thread.
    thread::spawn(move || {
        let _ = child.wait();
    });
}

/// Drain both pipes concurrently, cap their memory, and bound the entire client
/// call (including EOF). Readers own no Factory state and cannot publish a late
/// result after this function returns. Owner recovery is an explicit operation.
pub fn output(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    if timeout.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native owner transport timeout must be positive",
        ));
    }
    let started = Instant::now();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = mpsc::channel();
    let other = sender.clone();
    thread::spawn(move || {
        let _ = sender.send((true, read_pipe(stdout)));
    });
    thread::spawn(move || {
        let _ = other.send((false, read_pipe(stderr)));
    });
    let mut stdout: Option<Vec<u8>> = None;
    let mut stderr: Option<Vec<u8>> = None;
    let mut status = None;
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(value) => status = value,
                Err(error) => {
                    stop_client(child);
                    return Err(error);
                }
            }
        }
        if let (Some(status), Some(stdout), Some(stderr)) =
            (status, stdout.as_ref(), stderr.as_ref())
        {
            return Ok(Output {
                status,
                stdout: stdout.clone(),
                stderr: stderr.clone(),
            });
        }
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            stop_client(child);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "native owner transport timed out; inspect the original operation, never implicitly resend",
            ));
        };
        match receiver.recv_timeout(remaining.min(Duration::from_millis(10))) {
            Ok((is_stdout, Ok(bytes))) => {
                if is_stdout {
                    stdout = Some(bytes);
                } else {
                    stderr = Some(bytes);
                }
            }
            Ok((_, Err(error))) => {
                stop_client(child);
                return Err(error);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if stdout.is_none() || stderr.is_none() {
                    stop_client(child);
                    return Err(io::Error::other("native owner pipe reader disconnected"));
                }
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_timeout_does_not_start_a_process() {
        assert_eq!(
            output(&mut Command::new("must-not-run"), Duration::ZERO)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[cfg(unix)]
    #[test]
    fn drains_both_pipes_and_preserves_exit_status() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf output; printf diagnostic >&2; exit 7"]);
        let result = output(&mut command, Duration::from_secs(5)).unwrap();
        assert_eq!(result.status.code(), Some(7));
        assert_eq!(result.stdout, b"output");
        assert_eq!(result.stderr, b"diagnostic");
    }

    #[cfg(unix)]
    #[test]
    fn inherited_pipe_does_not_turn_timeout_into_an_unbounded_wait() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 1 & wait"]);
        let started = Instant::now();
        let error = output(&mut command, Duration::from_millis(40)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn oversized_output_is_refused_not_truncated_into_valid_evidence() {
        let mut command = Command::new("sh");
        command.args(["-c", "yes x"]);
        assert_eq!(
            output(&mut command, Duration::from_secs(5))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
