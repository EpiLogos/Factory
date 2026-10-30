//! Bounded transport to an owner client. Killing this client is deliberately
//! NOT evidence that a remote worker stopped or its effects were rolled back.

use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const MAX_OWNER_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const READ_BYTES: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(2);
const MAX_CLEANUP_RESERVE: Duration = Duration::from_millis(100);

/// Actual transport failure facts. Raw owner bytes are available only through
/// explicit accessors; neither Debug nor Display prints those bytes. This is
/// evidence, not a retry grant, native worker cancellation or Return authority.
pub struct NativeCaptureFailure {
    cause: io::Error,
    observation: CaptureObservation,
    cleanup_errors: Vec<io::Error>,
}

#[derive(Default)]
struct CaptureObservation {
    process_started: bool,
    status: Option<ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_eof: bool,
    stderr_eof: bool,
    stdout_truncated: bool,
    stderr_truncated: bool,
    timed_out: bool,
    ownership_lost: bool,
    stop_attempted: bool,
    stop_requested: bool,
}

impl NativeCaptureFailure {
    pub fn process_started(&self) -> bool {
        self.observation.process_started
    }
    pub fn status(&self) -> Option<ExitStatus> {
        self.observation.status
    }
    pub fn stdout(&self) -> &[u8] {
        &self.observation.stdout
    }
    pub fn stderr(&self) -> &[u8] {
        &self.observation.stderr
    }
    pub fn stdout_eof(&self) -> bool {
        self.observation.stdout_eof
    }
    pub fn stderr_eof(&self) -> bool {
        self.observation.stderr_eof
    }
    pub fn stdout_truncated(&self) -> bool {
        self.observation.stdout_truncated
    }
    pub fn stderr_truncated(&self) -> bool {
        self.observation.stderr_truncated
    }
    pub fn timed_out(&self) -> bool {
        self.observation.timed_out
    }
    pub fn ownership_lost(&self) -> bool {
        self.observation.ownership_lost
    }
    pub fn client_reaped(&self) -> bool {
        self.observation.status.is_some()
    }
    pub fn stop_attempted(&self) -> bool {
        self.observation.stop_attempted
    }
    pub fn stop_requested(&self) -> bool {
        self.observation.stop_requested
    }
    pub fn cause(&self) -> &io::Error {
        &self.cause
    }
    pub fn cleanup_errors(&self) -> &[io::Error] {
        &self.cleanup_errors
    }
    fn before_spawn(cause: io::Error) -> Self {
        Self {
            cause,
            observation: CaptureObservation::default(),
            cleanup_errors: Vec::new(),
        }
    }
    fn into_io(self) -> io::Error {
        io::Error::new(self.cause.kind(), self)
    }
}

impl Debug for NativeCaptureFailure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeCaptureFailure")
            .field("cause_kind", &self.cause.kind())
            .field("cause_os_error", &self.cause.raw_os_error())
            .field("process_started", &self.process_started())
            .field("status", &self.status())
            .field("stdout_bytes", &self.stdout().len())
            .field("stderr_bytes", &self.stderr().len())
            .field("stdout_eof", &self.stdout_eof())
            .field("stderr_eof", &self.stderr_eof())
            .field("stdout_truncated", &self.stdout_truncated())
            .field("stderr_truncated", &self.stderr_truncated())
            .field("timed_out", &self.timed_out())
            .field("ownership_lost", &self.ownership_lost())
            .field("stop_attempted", &self.stop_attempted())
            .field("stop_requested", &self.stop_requested())
            .field("cleanup_error_count", &self.cleanup_errors.len())
            .finish()
    }
}
impl Display for NativeCaptureFailure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "native owner transport {:?} (OS error {:?}); started={}, status={:?}, EOF={}/{}, truncated={}/{}, ownership_lost={}, cleanup_errors={}; effects may be unknown; inspect the original operation, never implicitly resend",
            self.cause.kind(), self.cause.raw_os_error(), self.process_started(), self.status(),
            self.stdout_eof(), self.stderr_eof(), self.stdout_truncated(), self.stderr_truncated(),
            self.ownership_lost(), self.cleanup_errors.len())
    }
}
impl Error for NativeCaptureFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}

/// Recover typed capture facts without replacing the original io::Error kind.
/// Inspect this first when deciding whether an operation actually started.
pub fn capture_failure(error: &io::Error) -> Option<&NativeCaptureFailure> {
    error.get_ref()?.downcast_ref::<NativeCaptureFailure>()
}

#[cfg(unix)]
fn prepare_pipe<T: std::os::fd::AsRawFd>(pipe: &T) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    // Held, exclusively read transport descriptors; no mutable path lookup.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(unix)]
fn pipe_read<T: Read>(pipe: &mut T, buffer: &mut [u8]) -> io::Result<Option<usize>> {
    match pipe.read(buffer) {
        Ok(count) => Ok(Some(count)),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn prepare_pipe<T: std::os::windows::io::AsRawHandle>(_pipe: &T) -> io::Result<()> {
    Ok(())
}
#[cfg(windows)]
fn pipe_read<T: Read + std::os::windows::io::AsRawHandle>(
    pipe: &mut T,
    buffer: &mut [u8],
) -> io::Result<Option<usize>> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn PeekNamedPipe(
            handle: *mut c_void,
            buffer: *mut c_void,
            bytes: u32,
            read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
    }
    let mut available = 0u32;
    // One owner accesses this held pipe, without concurrent reads. Microsoft
    // documents a synchronous-handle blocking caveat for multithreaded apps;
    // Windows absolute syscall-deadline qualification remains separate. Never
    // call blocking read when the actual native observation reports no bytes.
    let ok = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(109) {
            Ok(Some(0))
        } else {
            Err(error)
        };
    }
    if available == 0 {
        return Ok(None);
    }
    let count = buffer.len().min(available as usize);
    pipe.read(&mut buffer[..count]).map(Some)
}

// No bounded native pipe faculty is established for other targets here. The
// existing supported Unix and Windows routes above remain operative. This is
// not a pre-spawn platform refusal and does not fabricate a stopped process.
#[cfg(not(any(unix, windows)))]
fn prepare_pipe<T>(_pipe: &T) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native bounded pipe readiness is unavailable on this target",
    ))
}
#[cfg(not(any(unix, windows)))]
fn pipe_read<T: Read>(_pipe: &mut T, _buffer: &mut [u8]) -> io::Result<Option<usize>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native bounded pipe readiness is unavailable on this target",
    ))
}

fn append_capture(
    bytes: &mut Vec<u8>,
    truncated: &mut bool,
    read: &[u8],
    limit: u64,
) -> io::Result<()> {
    let room = limit as usize - bytes.len();
    bytes.extend_from_slice(&read[..read.len().min(room)]);
    if read.len() > room {
        *truncated = true;
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native owner output exceeded its byte bound; effects may be unknown",
        ));
    }
    Ok(())
}

/// One kill(2) existence probe of an owned group, read as POSIX defines it.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupProbe {
    /// ESRCH: no process of the group remains. Only this confirms retirement.
    Retired,
    /// A member answered; it may still be running.
    Present,
    /// EPERM: the group still exists but no member may be signalled. On macOS
    /// every killed member passes through this state, past signalling but not
    /// yet unlinked from its group (Factory#295, #296). It is neither
    /// retirement nor a failed read. If it lasts to the deadline, a member
    /// this process cannot stop remains, and confirmation fails.
    Unsignallable,
}

#[cfg(unix)]
fn probe_owned_group(group: u32) -> io::Result<GroupProbe> {
    if unsafe { libc::kill(-(group as libc::pid_t), 0) } == 0 {
        return Ok(GroupProbe::Present);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(GroupProbe::Retired),
        Some(libc::EPERM) => Ok(GroupProbe::Unsignallable),
        _ => Err(error),
    }
}

/// Succeeds only on ESRCH, so no member of the owned group, ours or not,
/// signallable or not, is left when it returns Ok.
#[cfg(unix)]
fn confirm_group_retired(group: u32, deadline: Instant) -> io::Result<()> {
    loop {
        let probe = probe_owned_group(group)?;
        if probe == GroupProbe::Retired {
            return Ok(());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    if probe == GroupProbe::Unsignallable {
                        "owned reader group retirement unconfirmed within capture budget; a member this process may not signal remains (EPERM)"
                    } else {
                        "owned reader group retirement unconfirmed within capture budget"
                    },
                )
            })?;
        thread::sleep(remaining.min(POLL_INTERVAL));
    }
}

/// SIGKILL the owned group; Ok(true) when a member was signalled. ESRCH (the
/// group is gone) and EPERM (no member may be signalled now, see
/// GroupProbe::Unsignallable) request nothing and prove nothing. Retirement is
/// decided only by the confirm_group_retired that follows, or the capture
/// records lost ownership with its cleanup error.
#[cfg(unix)]
fn signal_owned_group(group: u32) -> io::Result<bool> {
    if unsafe { libc::kill(-(group as libc::pid_t), libc::SIGKILL) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH | libc::EPERM) => Ok(false),
        _ => Err(error),
    }
}

fn finish_failure(
    mut child: Child,
    cause: io::Error,
    mut observation: CaptureObservation,
    deadline: Instant,
    owned_group: Option<u32>,
) -> NativeCaptureFailure {
    let mut cleanup_errors = Vec::new();
    #[cfg(unix)]
    if !observation.ownership_lost {
        if let Some(group) = owned_group {
            // This exact group was established by this capture before exec.
            // Local transport retirement never proves a remote worker stopped.
            observation.stop_attempted = true;
            match signal_owned_group(group) {
                Ok(requested) => observation.stop_requested |= requested,
                Err(error) => cleanup_errors.push(error),
            }
        }
    }
    #[cfg(not(unix))]
    let _ = owned_group;
    // Any observed wait error forbids further signaling/reaping. The ordinary
    // Unix Child path assumes exclusive reaping; a hostile external waiter in
    // the interval between a successful check and signal is not atomically
    // fenced on every platform. Do not claim immunity to that external breach.
    if observation.status.is_none() && !observation.ownership_lost {
        match child.try_wait() {
            Ok(Some(status)) => observation.status = Some(status),
            Ok(None) => {
                observation.stop_attempted = true;
                match child.kill() {
                    Ok(()) => observation.stop_requested = true,
                    Err(error) => cleanup_errors.push(error),
                }
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            observation.status = Some(status);
                            break;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            observation.ownership_lost = true;
                            cleanup_errors.push(error);
                            break;
                        }
                    }
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    if remaining.is_zero() {
                        break;
                    }
                    thread::sleep(remaining.min(POLL_INTERVAL));
                }
            }
            Err(error) => {
                observation.ownership_lost = true;
                cleanup_errors.push(error);
            }
        }
    }
    #[cfg(unix)]
    if !observation.ownership_lost {
        if let Some(group) = owned_group {
            if let Err(error) = confirm_group_retired(group, deadline) {
                cleanup_errors.push(error);
            }
        }
    }
    // No detached wait/reader is created. Unobserved reap remains explicit;
    // neither a successful kill request nor dropping Child proves retirement.
    NativeCaptureFailure {
        cause,
        observation,
        cleanup_errors,
    }
}

#[cfg(test)]
fn capture_spawned(
    child: Child,
    deadline: Instant,
    capture_deadline: Instant,
) -> Result<Output, NativeCaptureFailure> {
    capture_spawned_with_limits(
        child,
        deadline,
        capture_deadline,
        MAX_OWNER_OUTPUT_BYTES,
        MAX_OWNER_OUTPUT_BYTES,
        None,
    )
}

fn capture_spawned_with_limits(
    mut child: Child,
    deadline: Instant,
    capture_deadline: Instant,
    stdout_limit: u64,
    stderr_limit: u64,
    owned_group: Option<u32>,
) -> Result<Output, NativeCaptureFailure> {
    let mut observation = CaptureObservation {
        process_started: true,
        ..CaptureObservation::default()
    };
    let Some(mut stdout) = child.stdout.take() else {
        return Err(finish_failure(
            child,
            io::Error::other("native owner stdout pipe was absent"),
            observation,
            deadline,
            owned_group,
        ));
    };
    let Some(mut stderr) = child.stderr.take() else {
        drop(stdout);
        return Err(finish_failure(
            child,
            io::Error::other("native owner stderr pipe was absent"),
            observation,
            deadline,
            owned_group,
        ));
    };
    if let Err(error) = prepare_pipe(&stdout).and_then(|()| prepare_pipe(&stderr)) {
        drop(stdout);
        drop(stderr);
        return Err(finish_failure(
            child,
            error,
            observation,
            deadline,
            owned_group,
        ));
    }
    let mut buffer = [0u8; READ_BYTES];
    let outcome: io::Result<()> = loop {
        if observation.status.is_none() {
            match child.try_wait() {
                Ok(status) => observation.status = status,
                Err(error) => {
                    observation.ownership_lost = true;
                    break Err(error);
                }
            }
        }
        if !observation.stdout_eof {
            match pipe_read(&mut stdout, &mut buffer) {
                Ok(Some(0)) => observation.stdout_eof = true,
                Ok(Some(count)) => {
                    if let Err(error) = append_capture(
                        &mut observation.stdout,
                        &mut observation.stdout_truncated,
                        &buffer[..count],
                        stdout_limit,
                    ) {
                        break Err(error);
                    }
                }
                Ok(None) => {}
                Err(error) => break Err(error),
            }
        }
        if !observation.stderr_eof {
            match pipe_read(&mut stderr, &mut buffer) {
                Ok(Some(0)) => observation.stderr_eof = true,
                Ok(Some(count)) => {
                    if let Err(error) = append_capture(
                        &mut observation.stderr,
                        &mut observation.stderr_truncated,
                        &buffer[..count],
                        stderr_limit,
                    ) {
                        break Err(error);
                    }
                }
                Ok(None) => {}
                Err(error) => break Err(error),
            }
        }
        // Read and wait are actual observations. Late complete bytes do not
        // silently turn an expired transport into timely success.
        let Some(remaining) = capture_deadline.checked_duration_since(Instant::now()) else {
            observation.timed_out = true;
            break Err(io::Error::new(io::ErrorKind::TimedOut,
                "native owner transport timed out; inspect the original operation, never implicitly resend"));
        };
        if remaining.is_zero() {
            observation.timed_out = true;
            break Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "native owner transport deadline reached",
            ));
        }
        if observation.status.is_some() && observation.stdout_eof && observation.stderr_eof {
            break Ok(());
        }
        // Fair bounded reads: one chunk per stream per turn. No peer pipe can
        // starve a deadline or force an unbounded read_to_end allocation.
        thread::sleep(remaining.min(POLL_INTERVAL));
    };
    drop(stdout);
    drop(stderr);
    match outcome {
        Ok(()) => {
            #[cfg(unix)]
            if let Some(group) = owned_group {
                // A successful read owns no lasting background process, even
                // when a child closed inherited output descriptors early.
                let retired =
                    signal_owned_group(group).and_then(|_| confirm_group_retired(group, deadline));
                if let Err(cause) = retired {
                    return Err(finish_failure(
                        child,
                        cause,
                        observation,
                        deadline,
                        owned_group,
                    ));
                }
            }
            Ok(Output {
                status: observation.status.expect("actual status observed"),
                stdout: observation.stdout,
                stderr: observation.stderr,
            })
        }
        Err(cause) => Err(finish_failure(
            child,
            cause,
            observation,
            deadline,
            owned_group,
        )),
    }
}

/// One capture engine, including actual status, bounded bytes and cleanup facts.
/// A maximum100ms (at most one quarter) of the existing total budget is reserved
/// for owned-client cleanup. No process-group setting is added or overridden.
/// A syscall that the platform cannot make finite remains an explicit native
/// qualification limit; Windows syscall timing is not proved by this Source.
pub fn capture_output(
    command: &mut Command,
    timeout: Duration,
) -> Result<Output, NativeCaptureFailure> {
    capture_output_with_limits(
        command,
        timeout,
        Stdio::null(),
        MAX_OWNER_OUTPUT_BYTES,
        MAX_OWNER_OUTPUT_BYTES,
        false,
    )
}

/// The same capture engine for native read transports with an explicit input
/// file and response bounds. Defaults above remain unchanged. On Unix an
/// explicitly owned group includes inherited-pipe descendants in cleanup.
pub(crate) fn capture_output_with_limits(
    command: &mut Command,
    timeout: Duration,
    stdin: Stdio,
    stdout_limit: u64,
    stderr_limit: u64,
    own_group: bool,
) -> Result<Output, NativeCaptureFailure> {
    if timeout.is_zero()
        || stdout_limit == 0
        || stderr_limit == 0
        || stdout_limit > 16 * 1024 * 1024
        || stderr_limit > 16 * 1024 * 1024
    {
        return Err(NativeCaptureFailure::before_spawn(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native owner transport requires positive finite time and byte limits",
        )));
    }
    #[cfg(unix)]
    if own_group {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    if own_group {
        return Err(NativeCaptureFailure::before_spawn(io::Error::new(
            io::ErrorKind::Unsupported,
            "owned reader process-group cleanup is unavailable on this target",
        )));
    }
    let started = Instant::now();
    let deadline = started.checked_add(timeout).ok_or_else(|| {
        NativeCaptureFailure::before_spawn(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native owner transport deadline overflow",
        ))
    })?;
    let reserve = MAX_CLEANUP_RESERVE.min(timeout / 4);
    let capture_deadline = deadline.checked_sub(reserve).unwrap_or(deadline);
    let child = command
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(NativeCaptureFailure::before_spawn)?;
    let group = own_group.then(|| child.id());
    capture_spawned_with_limits(
        child,
        deadline,
        capture_deadline,
        stdout_limit,
        stderr_limit,
        group,
    )
}

/// Legacy API/Output ABI preserved. Complete actual nonzero exits remain Output.
/// IO kind remains native; its typed cause retains actual incomplete capture.
pub fn output(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    capture_output(command, timeout).map_err(NativeCaptureFailure::into_io)
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

#[cfg(all(test, unix))]
mod capture_adversity {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use std::process::ChildStdout;

    // Exact actual Child ownership, not a guessed PID cleanup. Drop never sends
    // after a wait error, and a cleanup uncertainty cannot silently pass a test.
    struct OwnedTestChild(Option<Child>);
    impl OwnedTestChild {
        fn take(&mut self) -> Child {
            self.0.take().expect("owned test child")
        }
    }
    impl Drop for OwnedTestChild {
        fn drop(&mut self) {
            let Some(mut child) = self.0.take() else {
                return;
            };
            let deadline = Instant::now() + Duration::from_secs(1);
            let mut error = None;
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => {
                    if let Err(cause) = child.kill() {
                        error = Some(cause.to_string());
                    }
                }
                Err(cause) => {
                    // Ownership uncertain; never signal a possibly reused PID.
                    error = Some(format!("test wait authority lost: {cause}"));
                }
            }
            if error.is_none() {
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => return,
                        Ok(None) => {}
                        Err(cause) => {
                            error = Some(cause.to_string());
                            break;
                        }
                    }
                    if Instant::now() >= deadline {
                        error = Some("test child reap unconfirmed".into());
                        break;
                    }
                    thread::sleep(POLL_INTERVAL);
                }
            }
            let cause = error.expect("actual cleanup error");
            if std::thread::panicking() {
                eprintln!("secondary native test cleanup: {cause}");
            } else {
                panic!("native test cleanup failed: {cause}");
            }
        }
    }

    fn spawned(script: &str) -> OwnedTestChild {
        OwnedTestChild(Some(
            Command::new("/bin/sh")
                .args(["-c", script])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ))
    }
    fn actual_capture(child: Child, timeout: Duration) -> Result<Output, NativeCaptureFailure> {
        let now = Instant::now();
        capture_spawned(
            child,
            now + timeout,
            now + timeout - MAX_CLEANUP_RESERVE.min(timeout / 4),
        )
    }
    fn wait_ready(path: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.is_file() {
            assert!(
                Instant::now() < deadline,
                "actual OS helper readiness missing"
            );
            thread::sleep(POLL_INTERVAL);
        }
    }
    fn partial_child() -> (tempfile::TempDir, OwnedTestChild) {
        let directory = tempfile::tempdir().unwrap();
        let ready = directory.path().join("actual-ready");
        let child = Command::new("/bin/sh")
            .args([
                "-c",
                "printf private-prefix; printf private-diagnostic >&2; : > \"$1\"; exec sleep 30",
                "native-process-test",
            ])
            .arg(&ready)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let owned = OwnedTestChild(Some(child));
        // Synchronize on actual bytes written before testing the private owner
        // capture loop; no scheduling-based imaginary prefix is asserted.
        wait_ready(&ready);
        (directory, owned)
    }
    fn actual_pipe() -> (OwnedFd, File) {
        let mut fds = [-1; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let write = unsafe { File::from_raw_fd(fds[1]) };
        for fd in [read.as_raw_fd(), write.as_raw_fd()] {
            assert_eq!(
                unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        (read, write)
    }
    fn child_with_actual_pipe(read: OwnedFd, write: &File) -> OwnedTestChild {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "printf early; exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::from(write.try_clone().unwrap()))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // This held native reader is the same pipe supplied to actual stdout.
        // No invented ExitStatus, patched wait or fabricated owner JSON.
        child.stdout = Some(ChildStdout::from(read));
        OwnedTestChild(Some(child))
    }

    #[test]
    fn missing_executable_retains_native_pre_spawn_kind_and_cause() {
        let directory = tempfile::tempdir().unwrap();
        let error = output(
            &mut Command::new(directory.path().join("absent-native-program")),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        let facts = capture_failure(&error).expect("same original typed IO source");
        assert!(!facts.process_started());
        assert_eq!(facts.cause().kind(), io::ErrorKind::NotFound);
        assert_eq!(facts.cause().raw_os_error(), Some(libc::ENOENT));
        assert!(facts.status().is_none());
        assert!(!facts.stop_attempted());
    }

    #[test]
    fn actual_partial_timeout_retains_bytes_and_owned_client_cleanup() {
        let (_directory, mut child) = partial_child();
        let failure = actual_capture(child.take(), Duration::from_millis(400)).unwrap_err();
        assert_eq!(failure.cause().kind(), io::ErrorKind::TimedOut);
        assert!(failure.process_started() && failure.timed_out());
        assert_eq!(failure.stdout(), b"private-prefix");
        assert_eq!(failure.stderr(), b"private-diagnostic");
        assert!(!failure.stdout_eof() && !failure.stderr_eof());
        assert!(failure.stop_attempted() && failure.stop_requested() && failure.client_reaped());
        assert!(failure.cleanup_errors().is_empty());
    }

    #[test]
    fn actual_failure_debug_display_and_io_wrapper_do_not_export_prefixes() {
        let (_directory, mut child) = partial_child();
        let error = actual_capture(child.take(), Duration::from_millis(400))
            .unwrap_err()
            .into_io();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let facts = capture_failure(&error).unwrap();
        assert_eq!(facts.stdout(), b"private-prefix");
        assert_eq!(facts.stderr(), b"private-diagnostic");
        for text in [
            format!("{error}"),
            format!("{error:?}"),
            format!("{facts:?}"),
        ] {
            assert!(!text.contains("private-prefix") && !text.contains("private-diagnostic"));
        }
        assert_eq!(
            facts
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn real_overflow_retains_bounded_prefix_without_valid_response() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf diagnostic >&2; exec yes x"]);
        let error = output(&mut command, Duration::from_secs(5)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let facts = capture_failure(&error).unwrap();
        assert_eq!(facts.stdout().len(), MAX_OWNER_OUTPUT_BYTES as usize);
        assert!(facts.stdout_truncated());
        assert!(!facts.stdout_eof());
        assert_eq!(facts.stderr(), b"diagnostic");
        assert!(facts.process_started());
        assert!(
            facts.client_reaped(),
            "actual overflow child reap missing: {facts:?}"
        );
    }

    #[test]
    fn actual_zero_exit_with_open_writer_retains_status_but_cannot_complete() {
        let (read, write) = actual_pipe();
        let mut child = child_with_actual_pipe(read, &write);
        let failure = actual_capture(child.take(), Duration::from_millis(400)).unwrap_err();
        assert_eq!(failure.status().and_then(|status| status.code()), Some(0));
        assert_eq!(failure.stdout(), b"early");
        assert!(!failure.stdout_eof());
        assert!(failure.stderr_eof() && failure.timed_out() && failure.client_reaped());
        assert!(
            !failure.stop_attempted(),
            "already reaped client must not be signalled"
        );
        drop(write);
    }

    #[test]
    fn real_escaped_writer_after_timeout_observes_closed_capture_pipe() {
        let (read, write) = actual_pipe();
        let mut producer = child_with_actual_pipe(read, &write);
        let directory = tempfile::tempdir().unwrap();
        let ready = directory.path().join("writer-ready");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                ": > \"$1\"; read -r line; printf late-output; exit 0",
                "native-writer-test",
            ])
            .arg(&ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(write.try_clone().unwrap()))
            .stderr(Stdio::piped());
        // The test owns this actual Child; it creates a distinct native session
        // before exec and does not guess a PID of a foreign worker.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut writer = OwnedTestChild(Some(command.spawn().unwrap()));
        wait_ready(&ready);
        drop(write);
        let failure = actual_capture(producer.take(), Duration::from_millis(400)).unwrap_err();
        assert_eq!(failure.status().and_then(|status| status.code()), Some(0));
        assert_eq!(failure.stdout(), b"early");
        assert!(!failure.stdout_eof());
        writer
            .0
            .as_mut()
            .unwrap()
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"release\n")
            .unwrap();
        drop(writer.0.as_mut().unwrap().stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match writer.0.as_mut().unwrap().try_wait() {
                Ok(Some(status)) => {
                    assert!(
                        !status.success(),
                        "late write unexpectedly reached a retained capture reader"
                    );
                    break;
                }
                Ok(None) => {}
                Err(error) => panic!("actual escaped writer wait failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "actual late writer retirement missing"
            );
            thread::sleep(POLL_INTERVAL);
        }
        assert!(failure.cleanup_errors().is_empty());
    }

    #[test]
    fn actual_external_reap_refuses_signal_and_retains_echild() {
        let mut owner = spawned("printf original-prefix; exit 0");
        let pid = owner.0.as_ref().unwrap().id() as libc::pid_t;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut native_status = 0;
        loop {
            let found = unsafe { libc::waitpid(pid, &mut native_status, libc::WNOHANG) };
            if found == pid {
                break;
            }
            assert_eq!(found, 0, "real exclusive fixture waitpid failed");
            assert!(Instant::now() < deadline, "actual fixture exit missing");
            thread::sleep(POLL_INTERVAL);
        }
        let failure = actual_capture(owner.take(), Duration::from_secs(1)).unwrap_err();
        assert_eq!(failure.cause().raw_os_error(), Some(libc::ECHILD));
        assert!(failure.process_started() && failure.ownership_lost());
        assert!(!failure.stop_attempted() && !failure.stop_requested());
        assert!(
            failure.status().is_none(),
            "external status cannot be forged into Child status"
        );
        assert!(failure.cleanup_errors().is_empty());
    }

    #[test]
    fn actual_default_group_and_explicit_caller_group_are_preserved() {
        let own_group = unsafe { libc::getpgid(0) };
        assert!(own_group > 0);
        let script = "ps -o pgid= -p $$";
        let normal = output(
            Command::new("/bin/sh").args(["-c", script]),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(normal.status.success());
        assert_eq!(
            String::from_utf8(normal.stdout)
                .unwrap()
                .trim()
                .parse::<i32>()
                .unwrap(),
            own_group
        );
        let mut grouped = Command::new("/bin/sh");
        grouped.args(["-c", script]).process_group(0);
        let explicit = output(&mut grouped, Duration::from_secs(5)).unwrap();
        assert!(explicit.status.success());
        let actual_group = String::from_utf8(explicit.stdout)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        assert!(actual_group > 0 && actual_group != own_group);
    }

    /// An owned group whose leader has exited and been reaped, leaving one
    /// member reparented to init: the shape an owner read leaves when it
    /// starts a child and exits. Drop stops whatever of the group remains.
    struct OrphanedGroup {
        group: u32,
        member: libc::pid_t,
    }
    impl OrphanedGroup {
        fn spawn() -> Self {
            let mut leader = Command::new("/bin/sh")
                .args(["-c", "/bin/sleep 30 </dev/null >/dev/null 2>&1 & echo $!"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
                .unwrap();
            let group = leader.id();
            let mut member = String::new();
            let read = leader.stdout.take().unwrap().read_to_string(&mut member);
            let status = leader.wait().unwrap();
            let orphaned = Self {
                group,
                member: member.trim().parse().unwrap_or(0),
            };
            read.unwrap();
            assert!(status.success() && orphaned.member > 0);
            orphaned
        }
    }
    impl Drop for OrphanedGroup {
        fn drop(&mut self) {
            if probe_owned_group(self.group).ok() != Some(GroupProbe::Retired) {
                let _ = signal_owned_group(self.group);
            }
        }
    }

    /// The owner read that failed on macOS CI (Factory#295, #296): the reader
    /// exits, its reaped leader leaves a reparented member, and the transport
    /// kills and probes the group. On macOS each killed member passes through
    /// a window where it is past signalling but still linked to its group, and
    /// kill(2) answers EPERM there. The production 2 ms poll landed in it only
    /// under runner load; this tight loop lands in it nearly every round. Our
    /// own retiring group is never a cleanup error and ends only in ESRCH.
    #[test]
    fn killed_group_with_reparented_member_retires_without_cleanup_error() {
        let mut unsignallable = 0;
        for _ in 0..20 {
            let orphaned = OrphanedGroup::spawn();
            let group = orphaned.group;
            assert_eq!(probe_owned_group(group).unwrap(), GroupProbe::Present);
            assert!(signal_owned_group(group).unwrap());
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let signal = signal_owned_group(group);
                assert!(
                    signal.is_ok(),
                    "re-signal of our retiring group: {signal:?}"
                );
                match probe_owned_group(group) {
                    Ok(GroupProbe::Retired) => break,
                    Ok(GroupProbe::Unsignallable) => unsignallable += 1,
                    Ok(GroupProbe::Present) => {}
                    Err(error) => panic!("probe of our retiring group: {error}"),
                }
                assert!(Instant::now() < deadline, "group {group} not retired");
            }
            assert_eq!(
                unsafe { libc::kill(orphaned.member, 0) },
                -1,
                "member {} survived its group",
                orphaned.member
            );
        }
        eprintln!("EPERM probes of retiring groups observed: {unsignallable}");
    }

    /// The guarantee the EPERM reading must not loosen: a live member of our
    /// group is never confirmed retired, and confirmation follows the kill.
    #[test]
    fn live_owned_group_member_is_never_confirmed_retired() {
        let orphaned = OrphanedGroup::spawn();
        let live =
            confirm_group_retired(orphaned.group, Instant::now() + Duration::from_millis(50))
                .unwrap_err();
        assert_eq!(live.kind(), io::ErrorKind::TimedOut);
        assert!(signal_owned_group(orphaned.group).unwrap());
        confirm_group_retired(orphaned.group, Instant::now() + Duration::from_secs(5)).unwrap();
        assert_eq!(unsafe { libc::kill(orphaned.member, 0) }, -1);
    }
}

#[cfg(all(test, windows))]
mod windows_capture_native {
    use super::*;
    #[test]
    fn actual_windows_cmd_dual_pipe_nonzero_output_is_preserved() {
        let result = output(
            Command::new("cmd.exe").args([
                "/d",
                "/c",
                "echo output & echo diagnostic 1>&2 & exit /b 7",
            ]),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(7));
        assert!(result.stdout.windows(6).any(|bytes| bytes == b"output"));
        assert!(result
            .stderr
            .windows(10)
            .any(|bytes| bytes == b"diagnostic"));
    }
}
