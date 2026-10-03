//! Physical publication for existing Factory owner encodings. No semantic state
//! lives here: owners admit and encode their objects before calling publish.
use std::ffi::OsStr;
use std::io;
use std::path::Path;

pub const FACTORY_PUBLICATION_FAILURE_CONTRACT: &str = "factory.publication-failure/v1";

/// Facts from an actual owner publication, serializable without flattening its cause.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativePublicationDetails {
    pub source_path: std::path::PathBuf,
    pub published: bool,
    pub outcome: String,
    pub automatic_retry: bool,
    pub cause: NativePublicationCause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readback_observation: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativePublicationCause {
    pub kind: String,
    pub raw_os_error: Option<i32>,
    pub message: String,
}
#[derive(Debug)]
pub struct NativePublicationUncertainty {
    pub source_path: std::path::PathBuf,
    pub cause: io::Error,
    stage_name: std::path::PathBuf,
    sync_failed: bool,
    readback_observation: Option<String>,
}
impl NativePublicationUncertainty {
    pub fn details(&self) -> NativePublicationDetails {
        NativePublicationDetails {
            source_path: self.source_path.clone(),
            published: true,
            outcome: "unknown".into(),
            automatic_retry: false,
            cause: NativePublicationCause {
                kind: format!("{:?}", self.cause.kind()),
                raw_os_error: self.cause.raw_os_error(),
                message: self.cause.to_string(),
            },
            readback_observation: self.readback_observation.clone(),
        }
    }
}
impl std::fmt::Display for NativePublicationUncertainty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.sync_failed {
            write!(f, "directory sync failed after replacement: ")?;
        }
        write!(f, "{}", self.cause)?;
        if let Some(observation) = &self.readback_observation {
            write!(f, "; {observation}")?;
        }
        write!(
            f,
            "; candidate stage name {} (no pathname cleanup)",
            self.stage_name.display()
        )
    }
}
impl std::error::Error for NativePublicationUncertainty {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
pub fn native_publication_uncertainty<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a NativePublicationUncertainty> {
    let mut current = Some(error);
    // A foreign provider may expose an arbitrary source chain; never loop forever.
    for _ in 0..16 {
        let error = current?;
        if let Some(publication) = error.downcast_ref::<NativePublicationUncertainty>() {
            return Some(publication);
        }
        current = error.source();
    }
    None
}
pub fn native_publication_failure(
    error: &(dyn std::error::Error + 'static),
) -> Option<serde_json::Value> {
    let publication = native_publication_uncertainty(error)?;
    Some(
        serde_json::json!({"contract":FACTORY_PUBLICATION_FAILURE_CONTRACT, "ok":false,
        "error":{"code":"factory.publication_uncertain", "message":error.to_string(),
        "details":publication.details()}}),
    )
}

pub(crate) const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug)]
pub(crate) enum PublicationError {
    Before(io::Error),
    Uncertain(NativePublicationUncertainty),
}
impl From<io::Error> for PublicationError {
    fn from(error: io::Error) -> Self {
        Self::Before(error)
    }
}

fn empty(value: &serde_json::Value) -> bool {
    value.is_null()
        || value.as_array().is_some_and(Vec::is_empty)
        || value.as_object().is_some_and(serde_json::Map::is_empty)
}
pub(crate) fn unsupported_field(
    raw: &serde_json::Value,
    known: &serde_json::Value,
    path: &str,
) -> Option<String> {
    match (raw, known) {
        (serde_json::Value::Object(raw), serde_json::Value::Object(known)) => {
            for (key, value) in raw {
                let path = format!("{path}.{key}");
                match known.get(key) {
                    Some(current) => {
                        if let Some(error) = unsupported_field(value, current, &path) {
                            return Some(error);
                        }
                    }
                    None if !empty(value) => return Some(path),
                    None => {}
                }
            }
        }
        (serde_json::Value::Array(raw), serde_json::Value::Array(known)) => {
            if raw.len() != known.len() {
                return Some(format!("{path}: unsupported retained array shape"));
            }
            for (index, (value, current)) in raw.iter().zip(known).enumerate() {
                if let Some(error) = unsupported_field(value, current, &format!("{path}[{index}]"))
                {
                    return Some(error);
                }
            }
        }
        _ => {}
    }
    None
}
pub(crate) fn has_omitted(raw: &serde_json::Value, known: &serde_json::Value) -> bool {
    match (raw, known) {
        (serde_json::Value::Object(raw), serde_json::Value::Object(known)) => {
            raw.iter().any(|(key, value)| {
                known
                    .get(key)
                    .is_none_or(|current| has_omitted(value, current))
            })
        }
        (serde_json::Value::Array(raw), serde_json::Value::Array(known)) => {
            raw.len() != known.len() || raw.iter().zip(known).any(|(a, b)| has_omitted(a, b))
        }
        _ => false,
    }
}

/// Mechanical preservation only. Owners supply explicit native record/array
/// identity fields; an unrecognised changed array is refused, never matched by
/// position or by guesses about fields ending in "ref".
pub(crate) fn retain_empty_fields(
    raw: &serde_json::Value,
    before: &serde_json::Value,
    after: &mut serde_json::Value,
    path: &str,
    record_key: fn(&str) -> Option<&'static str>,
    array_key: fn(&str) -> Option<&'static str>,
) -> Result<(), String> {
    if !has_omitted(raw, before) {
        return Ok(());
    }
    let refused =
        || format!("ambiguous retained empty-field transfer at {path}; original bytes unchanged");
    match (raw, before, after) {
        (
            serde_json::Value::Object(raw),
            serde_json::Value::Object(before),
            serde_json::Value::Object(after),
        ) => {
            if let Some(key) = record_key(path) {
                if before.get(key).is_none() || before.get(key) != after.get(key) {
                    return Err(refused());
                }
            } else if path != "$"
                && before != &*after
                && raw.keys().any(|key| !before.contains_key(key))
            {
                // Only the owner-root envelope has a stable source scope here.
                // Unknown nested records need an explicit native identity, or
                // their exact typed contents must remain unchanged.
                return Err(refused());
            }
            for (key, value) in raw {
                match (before.get(key), after.get_mut(key)) {
                    (None, None) if empty(value) => {
                        after.insert(key.clone(), value.clone());
                    }
                    (None, _) => {}
                    (Some(previous), Some(current)) => retain_empty_fields(
                        value,
                        previous,
                        current,
                        &format!("{path}.{key}"),
                        record_key,
                        array_key,
                    )?,
                    (Some(previous), None) if has_omitted(value, previous) => return Err(refused()),
                    _ => {}
                }
            }
        }
        (
            serde_json::Value::Array(raw),
            serde_json::Value::Array(before),
            serde_json::Value::Array(after),
        ) => {
            if before.as_slice() == after.as_slice() {
                for (index, ((raw, previous), current)) in
                    raw.iter().zip(before).zip(after.iter_mut()).enumerate()
                {
                    retain_empty_fields(
                        raw,
                        previous,
                        current,
                        &format!("{path}[{index}]"),
                        record_key,
                        array_key,
                    )?;
                }
            } else {
                let key = array_key(path).ok_or_else(refused)?;
                for (raw, previous) in raw.iter().zip(before) {
                    if !has_omitted(raw, previous) {
                        continue;
                    }
                    let identity = previous
                        .get(key)
                        .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                        .ok_or_else(refused)?;
                    if before
                        .iter()
                        .filter(|v| v.get(key) == Some(identity))
                        .count()
                        != 1
                    {
                        return Err(refused());
                    }
                    let matches: Vec<_> = after
                        .iter()
                        .enumerate()
                        .filter(|(_, value)| value.get(key) == Some(identity))
                        .map(|(i, _)| i)
                        .collect();
                    if matches.len() != 1 {
                        return Err(refused());
                    }
                    retain_empty_fields(
                        raw,
                        previous,
                        &mut after[matches[0]],
                        &format!("{path}[{identity}]"),
                        record_key,
                        array_key,
                    )?;
                }
            }
        }
        _ => return Err(refused()),
    }
    Ok(())
}

#[cfg(unix)]
mod unix {
    use super::*;
    use fs2::FileExt;
    use std::collections::BTreeMap;
    use std::ffi::{CStr, CString, OsString};
    use std::fs::{File, Metadata};
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};
    #[cfg(target_os = "macos")]
    use std::os::macos::fs::MetadataExt as _;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Component, PathBuf};
    use std::time::{Duration, Instant};

    const OPEN_FLAGS: libc::c_int = libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
    const MAX_XATTR_BYTES: usize = 4 * 1024 * 1024;

    fn invalid(message: impl Into<String>) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, message.into())
    }
    #[derive(Debug)]
    struct NativePreparationFailure {
        cause: io::Error,
        stage_name: PathBuf,
    }
    impl std::fmt::Display for NativePreparationFailure {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                formatter,
                "{}; candidate stage name {} (no pathname cleanup)",
                self.cause,
                self.stage_name.display()
            )
        }
    }
    impl std::error::Error for NativePreparationFailure {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.cause)
        }
    }
    fn name(value: &OsStr) -> io::Result<CString> {
        CString::new(value.as_bytes()).map_err(|_| invalid("NUL in native file name"))
    }
    fn open_at(parent: &File, name: &CStr, flags: libc::c_int) -> io::Result<File> {
        // SAFETY: the parent descriptor is held; name is a live NUL-terminated
        // component. Success transfers a fresh descriptor to exactly one File.
        let fd =
            unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags | OPEN_FLAGS, 0o666) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a new, uniquely owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn same_inode(a: &Metadata, b: &Metadata) -> bool {
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    fn readable_regular(file: &File) -> io::Result<Metadata> {
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(invalid("native source/lock/stage must be a regular file"));
        }
        Ok(metadata)
    }
    fn regular(file: &File) -> io::Result<Metadata> {
        let metadata = readable_regular(file)?;
        if metadata.nlink() != 1 {
            return Err(invalid(
                "native mutation source/lock/stage must have a single link",
            ));
        }
        Ok(metadata)
    }
    fn bytes(file: &File) -> io::Result<Vec<u8>> {
        use std::os::unix::fs::FileExt as _;
        // Existing read authority may include a retained hardlink alias. The
        // mutation route separately admits each source/lock/stage with regular().
        let length = usize::try_from(readable_regular(file)?.len())
            .map_err(|_| invalid("native source length is unavailable"))?;
        let mut output = vec![0; length];
        file.read_exact_at(&mut output, 0)?;
        let mut extra = [0];
        if file.read_at(&mut extra, length as u64)? != 0 {
            return Err(invalid("native source grew while reading"));
        }
        Ok(output)
    }
    fn directory(path: &Path, create: bool) -> io::Result<File> {
        // A provider address may include a platform alias (/var on Mac). Bind
        // its existing parent to the resolved physical directory once; then all
        // actual traversal/open/create uses no-follow directory descriptors.
        // Re-resolve the original address before/after publication and compare
        // the held inode, so retargeting that alias cannot return a success.
        let mut existing = path;
        let mut suffix = Vec::new();
        let physical = loop {
            match existing.canonicalize() {
                Ok(mut physical) => {
                    for part in suffix.iter().rev() {
                        physical.push(part);
                    }
                    break physical;
                }
                Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                    suffix.push(
                        existing
                            .file_name()
                            .ok_or_else(|| invalid("native parent cannot be resolved"))?
                            .to_os_string(),
                    );
                    existing = existing
                        .parent()
                        .ok_or_else(|| invalid("native parent cannot be resolved"))?;
                }
                Err(error) => return Err(error),
            }
        };
        let mut dir = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(OPEN_FLAGS | libc::O_DIRECTORY)
            .open("/")?;
        for component in physical.components() {
            let part = match component {
                Component::RootDir | Component::CurDir => continue,
                Component::Normal(part) => part,
                Component::ParentDir => OsStr::new(".."),
                Component::Prefix(_) => return Err(invalid("unsupported native directory prefix")),
            };
            let part = name(part)?;
            match open_at(&dir, &part, libc::O_RDONLY | libc::O_DIRECTORY) {
                Ok(next) => dir = next,
                Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                    // SAFETY: held directory fd and live component; no pathname traversal.
                    let result = unsafe { libc::mkdirat(dir.as_raw_fd(), part.as_ptr(), 0o777) };
                    if result != 0
                        && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists
                    {
                        return Err(io::Error::last_os_error());
                    }
                    dir.sync_all()?;
                    dir = open_at(&dir, &part, libc::O_RDONLY | libc::O_DIRECTORY)?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(dir)
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Privacy {
        mode: u32,
        uid: u32,
        gid: u32,
        attributes: BTreeMap<Vec<u8>, Vec<u8>>,
        #[cfg(target_os = "macos")]
        acl: Vec<u8>,
        #[cfg(target_os = "macos")]
        flags: u32,
    }
    impl Privacy {
        fn read(file: &File) -> io::Result<Self> {
            let m = regular(file)?;
            Ok(Self {
                mode: m.mode() & 0o7777,
                uid: m.uid(),
                gid: m.gid(),
                attributes: attributes(file)?,
                #[cfg(target_os = "macos")]
                acl: acl(file)?,
                #[cfg(target_os = "macos")]
                flags: m.st_flags(),
            })
        }
    }
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        // sys/acl.h: ACL_TYPE_EXTENDED=0x100. Allocation ownership is acl_free.
        fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_int) -> *mut libc::c_void;
        fn acl_to_text(value: *mut libc::c_void, length: *mut libc::ssize_t) -> *mut libc::c_char;
        fn acl_free(value: *mut libc::c_void) -> libc::c_int;
    }
    #[cfg(target_os = "macos")]
    fn acl(file: &File) -> io::Result<Vec<u8>> {
        acl_from_fd(file.as_raw_fd())
    }
    #[cfg(target_os = "macos")]
    fn acl_from_fd(fd: libc::c_int) -> io::Result<Vec<u8>> {
        // SAFETY: the descriptor is passed only to the native retrieval API;
        // each owned ACL/text allocation is released on all paths.
        unsafe {
            let value = acl_get_fd_np(fd, 0x100);
            if value.is_null() {
                let error = io::Error::last_os_error();
                // Darwin reports ENOENT for an existing held file with no
                // extended ACL. Absence is metadata, not a missing source.
                // Preserve every other retrieval failure, including EBADF.
                return if error.raw_os_error() == Some(libc::ENOENT) {
                    Ok(Vec::new())
                } else {
                    Err(error)
                };
            }
            let mut length = 0;
            let text = acl_to_text(value, &mut length);
            let result = if text.is_null() {
                Err(io::Error::last_os_error())
            } else if length < 0 || length as usize > MAX_XATTR_BYTES {
                Err(invalid("native ACL exceeds metadata bound"))
            } else {
                Ok(std::slice::from_raw_parts(text.cast::<u8>(), length as usize).to_vec())
            };
            if !text.is_null() {
                acl_free(text.cast());
            }
            acl_free(value);
            result
        }
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn attribute_list(file: &File, buffer: &mut [u8]) -> libc::ssize_t {
        // SAFETY: held fd and valid writable buffer (or null for the size query).
        unsafe {
            let pointer = if buffer.is_empty() {
                std::ptr::null_mut()
            } else {
                buffer.as_mut_ptr().cast()
            };
            #[cfg(target_os = "macos")]
            {
                libc::flistxattr(file.as_raw_fd(), pointer, buffer.len(), 0)
            }
            #[cfg(target_os = "linux")]
            {
                libc::flistxattr(file.as_raw_fd(), pointer, buffer.len())
            }
        }
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn attribute_get(file: &File, name: &CStr, buffer: &mut [u8]) -> libc::ssize_t {
        // SAFETY: held fd, valid name and writable buffer; null is a size query.
        unsafe {
            let pointer = if buffer.is_empty() {
                std::ptr::null_mut()
            } else {
                buffer.as_mut_ptr().cast()
            };
            #[cfg(target_os = "macos")]
            {
                libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), pointer, buffer.len(), 0, 0)
            }
            #[cfg(target_os = "linux")]
            {
                libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), pointer, buffer.len())
            }
        }
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn attributes(file: &File) -> io::Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        let length = attribute_list(file, &mut []);
        if length < 0 {
            return Err(io::Error::last_os_error());
        }
        if length as usize > MAX_XATTR_BYTES {
            return Err(invalid("native xattr names exceed metadata bound"));
        }
        let mut names = vec![0; length as usize];
        if attribute_list(file, &mut names) != length {
            return Err(invalid("native xattr names changed while reading"));
        }
        let mut output = BTreeMap::new();
        let mut total = names.len();
        for part in names.split(|v| *v == 0).filter(|v| !v.is_empty()) {
            if output.len() >= 1024 {
                return Err(invalid("native xattrs exceed 1024-entry metadata bound"));
            }
            let name = CString::new(part).map_err(|_| invalid("invalid xattr name"))?;
            let length = attribute_get(file, &name, &mut []);
            if length < 0 {
                return Err(io::Error::last_os_error());
            }
            total = total
                .checked_add(length as usize)
                .ok_or_else(|| invalid("native xattr size overflow"))?;
            if total > MAX_XATTR_BYTES {
                return Err(invalid("native xattrs exceed 4 MiB metadata bound"));
            }
            let mut value = vec![0; length as usize];
            if attribute_get(file, &name, &mut value) != length {
                return Err(invalid("native xattr changed while reading"));
            }
            output.insert(part.to_vec(), value);
        }
        Ok(output)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn attributes(_: &File) -> io::Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native metadata preservation is supported on Mac/Linux",
        ))
    }
    fn copy_privacy(source: &File, stage: &File, expected: &Privacy) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: source/stage descriptors remain held. Copy metadata only,
            // never overwrite the candidate bytes with the old source contents.
            // Establish security before copying possibly private xattr values.
            if unsafe {
                libc::fcopyfile(
                    source.as_raw_fd(),
                    stage.as_raw_fd(),
                    std::ptr::null_mut(),
                    libc::COPYFILE_SECURITY,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            if unsafe {
                libc::fcopyfile(
                    source.as_raw_fd(),
                    stage.as_raw_fd(),
                    std::ptr::null_mut(),
                    libc::COPYFILE_XATTR,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(target_os = "linux")]
        {
            let _ = source;
            use std::os::unix::fs::PermissionsExt;
            let current = stage.metadata()?;
            if current.uid() != expected.uid || current.gid() != expected.gid {
                // SAFETY: held stage fd; retained uid/gid are read from source.
                if unsafe { libc::fchown(stage.as_raw_fd(), expected.uid, expected.gid) } != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            stage.set_permissions(std::fs::Permissions::from_mode(expected.mode))?;
            for key in attributes(stage)?
                .keys()
                .filter(|key| !expected.attributes.contains_key(*key))
            {
                let key = CString::new(key.as_slice()).map_err(|_| invalid("invalid xattr"))?;
                // SAFETY: held stage fd and live name. Only stage metadata changes.
                if unsafe { libc::fremovexattr(stage.as_raw_fd(), key.as_ptr()) } != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            let mut attributes: Vec<_> = expected.attributes.iter().collect();
            attributes.sort_by_key(|(key, _)| key.as_slice() != b"system.posix_acl_access");
            for (key, value) in attributes {
                let key = CString::new(key.as_slice()).map_err(|_| invalid("invalid xattr"))?;
                // SAFETY: live name/value and held stage fd. Linux ACLs are xattrs.
                if unsafe {
                    libc::fsetxattr(
                        stage.as_raw_fd(),
                        key.as_ptr(),
                        value.as_ptr().cast(),
                        value.len(),
                        0,
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
            stage.set_permissions(std::fs::Permissions::from_mode(expected.mode))?;
        }
        if &Privacy::read(stage)? != expected {
            return Err(invalid(
                "native source/stage privacy metadata differs; replacement refused",
            ));
        }
        Ok(())
    }

    struct Source {
        file: File,
        bytes: Vec<u8>,
        privacy: Privacy,
    }
    pub(crate) struct NativeFileTransaction {
        parent: File,
        parent_path: PathBuf,
        target: CString,
        lock: File,
        lock_name: CString,
        source: Option<Source>,
    }
    impl NativeFileTransaction {
        pub(crate) fn acquire(
            path: &Path,
            lock_name: &OsStr,
            create_parent: bool,
        ) -> io::Result<Self> {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()?.join(path)
            };
            let parent_path = absolute
                .parent()
                .ok_or_else(|| invalid("native owner path must name a file"))?
                .to_path_buf();
            let target = name(
                absolute
                    .file_name()
                    .ok_or_else(|| invalid("native owner path must name a file"))?,
            )?;
            let parent = directory(&parent_path, create_parent)?;
            let lock_name = name(lock_name)?;
            let lock = open_at(&parent, &lock_name, libc::O_RDWR | libc::O_CREAT)?;
            regular(&lock)?;
            let deadline = Instant::now() + LOCK_TIMEOUT;
            loop {
                match FileExt::try_lock_exclusive(&lock) {
                    Ok(()) => break,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "native owner lock deadline exceeded (2 seconds)",
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error),
                }
            }
            let source = match open_at(&parent, &target, libc::O_RDWR) {
                Ok(file) => {
                    if regular(&file)?.mode() & 0o222 == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "native source has no writable mode; parent rename cannot bypass read-only source",
                        ));
                    }
                    Some(Source {
                        bytes: bytes(&file)?,
                        privacy: Privacy::read(&file)?,
                        file,
                    })
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            let transaction = Self {
                parent,
                parent_path,
                target,
                lock,
                lock_name,
                source,
            };
            transaction.validate_binding()?;
            Ok(transaction)
        }
        pub(crate) fn current(&self) -> Option<&[u8]> {
            self.source.as_ref().map(|s| s.bytes.as_slice())
        }
        fn validate_binding(&self) -> io::Result<()> {
            let current_parent = directory(&self.parent_path, false)?;
            if !same_inode(&self.parent.metadata()?, &current_parent.metadata()?) {
                return Err(invalid("native parent directory changed"));
            }
            let current_lock = open_at(&self.parent, &self.lock_name, libc::O_RDONLY)?;
            if !same_inode(&regular(&self.lock)?, &regular(&current_lock)?) {
                return Err(invalid("native lock inode changed"));
            }
            match (
                &self.source,
                open_at(&self.parent, &self.target, libc::O_RDONLY),
            ) {
                (None, Err(e)) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                (Some(source), Ok(current))
                    if same_inode(&regular(&source.file)?, &regular(&current)?)
                        && bytes(&current)? == source.bytes
                        && Privacy::read(&current)? == source.privacy =>
                {
                    Ok(())
                }
                (_, Err(error)) => Err(error),
                _ => Err(invalid(
                    "native source identity, bytes or metadata changed before publication",
                )),
            }
        }
        pub(crate) fn publish(
            &self,
            candidate: &[u8],
            create_new: bool,
        ) -> Result<(), PublicationError> {
            self.publish_named(
                candidate,
                create_new,
                &OsString::from(format!(".factory-owner-{}.tmp", ulid::Ulid::new())),
            )
        }
        fn publish_named(
            &self,
            candidate: &[u8],
            create_new: bool,
            stage_name: &OsStr,
        ) -> Result<(), PublicationError> {
            self.publish_candidate(candidate, create_new, stage_name)
                .map_err(|error| {
                    let annotate = |error: io::Error| {
                        io::Error::new(
                            error.kind(),
                            NativePreparationFailure {
                                cause: error,
                                stage_name: self.parent_path.join(stage_name),
                            },
                        )
                    };
                    match error {
                        PublicationError::Before(error) => {
                            PublicationError::Before(annotate(error))
                        }
                        PublicationError::Uncertain(error) => PublicationError::Uncertain(error),
                    }
                })
        }
        fn publish_candidate(
            &self,
            candidate: &[u8],
            create_new: bool,
            stage_name: &OsStr,
        ) -> Result<(), PublicationError> {
            self.validate_binding()?;
            if create_new && self.source.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "native bootstrap cannot replace retained state",
                )
                .into());
            }
            if self.current() == Some(candidate) {
                return Ok(());
            }
            let stage_name = name(stage_name)?;
            let mut stage = open_at(
                &self.parent,
                &stage_name,
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            )?;
            // Failure retains the named stage. Never unlink a pathname that may
            // now designate someone else's inode. Successful atomic rename consumes it.
            if let Some(source) = &self.source {
                copy_privacy(&source.file, &stage, &source.privacy)?;
            }
            #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
            tests::privacy_copied(
                &self
                    .parent_path
                    .join(OsStr::from_bytes(stage_name.as_bytes())),
            );
            // Retained source metadata is the admitted immutable basis. A fresh
            // stage read may detect drift, never adopt it as new source privacy.
            // Only bootstrap has no source and admits its actual stage defaults.
            let bootstrap_privacy;
            let expected_privacy = if let Some(source) = &self.source {
                &source.privacy
            } else {
                bootstrap_privacy = Privacy::read(&stage)?;
                &bootstrap_privacy
            };
            if &Privacy::read(&stage)? != expected_privacy {
                return Err(invalid("native stage privacy changed before candidate bytes").into());
            }
            // Apply retained privacy before writing any candidate bytes, so a
            // failed stage never discloses a private source through umask defaults.
            stage.write_all(candidate)?;
            if &Privacy::read(&stage)? != expected_privacy {
                return Err(invalid("native stage privacy changed while writing").into());
            }
            stage.sync_all()?;
            #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
            tests::stage_prepared(
                &self
                    .parent_path
                    .join(OsStr::from_bytes(stage_name.as_bytes())),
            );
            self.validate_binding()?;
            let named_stage = open_at(&self.parent, &stage_name, libc::O_RDONLY)?;
            if !same_inode(&regular(&stage)?, &regular(&named_stage)?)
                || bytes(&named_stage)? != candidate
                || &Privacy::read(&named_stage)? != expected_privacy
            {
                return Err(
                    invalid("native stage inode or bytes changed before publication").into(),
                );
            }
            // Preserve the admitted physical address before a late parent retarget.
            let physical_parent = std::fs::canonicalize(&self.parent_path)?;
            if !same_inode(
                &self.parent.metadata()?,
                &directory(&physical_parent, false)?.metadata()?,
            ) {
                return Err(
                    invalid("native physical source parent changed before publication").into(),
                );
            }
            let source_path = physical_parent.join(OsStr::from_bytes(self.target.as_bytes()));
            let uncertain = |cause, sync_failed, readback_observation| {
                PublicationError::Uncertain(NativePublicationUncertainty {
                    source_path: source_path.clone(),
                    cause,
                    stage_name: self
                        .parent_path
                        .join(OsStr::from_bytes(stage_name.as_bytes())),
                    sync_failed,
                    readback_observation,
                })
            };
            // SAFETY: both names are live CStrings beneath one held directory.
            // Native exclusive rename consumes stage without a hardlink/cleanup gap.
            let status = unsafe {
                #[cfg(target_os = "macos")]
                {
                    libc::renameatx_np(
                        self.parent.as_raw_fd(),
                        stage_name.as_ptr(),
                        self.parent.as_raw_fd(),
                        self.target.as_ptr(),
                        if create_new { libc::RENAME_EXCL } else { 0 },
                    )
                }
                #[cfg(target_os = "linux")]
                {
                    libc::renameat2(
                        self.parent.as_raw_fd(),
                        stage_name.as_ptr(),
                        self.parent.as_raw_fd(),
                        self.target.as_ptr(),
                        if create_new {
                            libc::RENAME_NOREPLACE
                        } else {
                            0
                        },
                    )
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "native publication supports Mac/Linux",
                    )
                    .into());
                }
            };
            if status != 0 {
                return Err(io::Error::last_os_error().into());
            }
            #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
            tests::published(
                &self
                    .parent_path
                    .join(OsStr::from_bytes(self.target.as_bytes())),
            );
            let readback = || -> io::Result<()> {
                let current = open_at(&self.parent, &self.target, libc::O_RDONLY)?;
                if !same_inode(&regular(&stage)?, &regular(&current)?)
                    || bytes(&current)? != candidate
                    || &Privacy::read(&current)? != expected_privacy
                {
                    return Err(invalid(
                        "native publication readback differs from held candidate",
                    ));
                }
                if !same_inode(
                    &self.parent.metadata()?,
                    &directory(&self.parent_path, false)?.metadata()?,
                ) {
                    return Err(invalid("native published parent binding changed"));
                }
                let current_lock = open_at(&self.parent, &self.lock_name, libc::O_RDONLY)?;
                if !same_inode(&regular(&self.lock)?, &regular(&current_lock)?) {
                    return Err(invalid("native lock binding changed after publication"));
                }
                Ok(())
            };
            if let Err(error) = self.parent.sync_all() {
                let observation = readback().err().map(|e| e.to_string()).unwrap_or_else(|| {
                    "held candidate is readable; durability remains uncertain".into()
                });
                return Err(uncertain(error, true, Some(observation)));
            }
            readback().map_err(|error| uncertain(error, false, None))
        }
    }
    pub(crate) fn lock_name(path: &Path) -> io::Result<OsString> {
        let mut output = OsString::from(".");
        output.push(
            path.file_name()
                .ok_or_else(|| invalid("native path must name a file"))?,
        );
        output.push(".lock");
        Ok(output)
    }
    pub(crate) fn read_native(path: &Path) -> io::Result<Vec<u8>> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let parent = directory(
            absolute
                .parent()
                .ok_or_else(|| invalid("native path must name a file"))?,
            false,
        )?;
        let file = open_at(
            &parent,
            &name(
                absolute
                    .file_name()
                    .ok_or_else(|| invalid("native path must name a file"))?,
            )?,
            libc::O_RDONLY,
        )?;
        bytes(&file)
    }

    // Tests observe an actual successful rename and mutate the real published
    // inode. There is no substitute IO result or production injection path.
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    pub(crate) fn observe_next_publication(observer: impl FnOnce(&Path) + 'static) {
        tests::set_publication_observer(Box::new(observer));
    }

    // Arm the existing real pre-byte checkpoint, never an injected IO result.
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    pub(crate) fn observe_next_privacy_copy(observer: impl FnOnce(&Path) + 'static) {
        tests::set_privacy_copy_observer(Box::new(observer));
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    mod tests {
        use super::*;
        use std::os::unix::fs::{symlink, PermissionsExt};

        type PublicationObserver = Box<dyn FnOnce(&Path)>;

        std::thread_local! {
            static AFTER_PRIVACY_COPIED: std::cell::RefCell<Option<PublicationObserver>> =
                const { std::cell::RefCell::new(None) };
            static AFTER_STAGE_PREPARED: std::cell::RefCell<Option<PublicationObserver>> =
                const { std::cell::RefCell::new(None) };
            static AFTER_PUBLICATION: std::cell::RefCell<Option<PublicationObserver>> =
                const { std::cell::RefCell::new(None) };
        }
        pub(super) fn set_privacy_copy_observer(observer: PublicationObserver) {
            AFTER_PRIVACY_COPIED.with(|slot| {
                let mut slot = slot.borrow_mut();
                assert!(slot.is_none(), "an unconsumed native privacy observer exists");
                *slot = Some(observer);
            });
        }
        pub(super) fn set_publication_observer(observer: PublicationObserver) {
            AFTER_PUBLICATION.with(|slot| {
                let mut slot = slot.borrow_mut();
                assert!(
                    slot.is_none(),
                    "an unconsumed native publication observer exists"
                );
                *slot = Some(observer);
            });
        }
        pub(super) fn privacy_copied(path: &Path) {
            let observer = AFTER_PRIVACY_COPIED.with(|slot| slot.borrow_mut().take());
            if let Some(observer) = observer {
                observer(path);
            }
        }
        pub(super) fn stage_prepared(path: &Path) {
            let observer = AFTER_STAGE_PREPARED.with(|slot| slot.borrow_mut().take());
            if let Some(observer) = observer {
                observer(path);
            }
        }
        pub(super) fn published(path: &Path) {
            let observer = AFTER_PUBLICATION.with(|slot| slot.borrow_mut().take());
            if let Some(observer) = observer {
                observer(path);
            }
        }

        #[test]
        fn actual_postcopy_privacy_drift_refuses_before_private_candidate_bytes() {
            let (root, path) = source();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            let transaction =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            let observed = std::sync::Arc::new(std::sync::Mutex::new(None::<PathBuf>));
            let changed = observed.clone();
            AFTER_PRIVACY_COPIED.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move |stage| {
                    std::fs::set_permissions(stage, std::fs::Permissions::from_mode(0o777))
                        .unwrap();
                    *changed.lock().unwrap() = Some(stage.to_path_buf());
                }));
            });
            let failure = transaction
                .publish(b"private candidate bytes", false)
                .unwrap_err();
            assert!(matches!(failure, PublicationError::Before(_)));
            let stage = observed.lock().unwrap().clone().unwrap();
            assert!(stage.starts_with(root.path()));
            assert!(
                std::fs::read(&stage).unwrap().is_empty(),
                "private bytes must never enter a foreign-permission stage"
            );
            assert_eq!(std::fs::metadata(&stage).unwrap().mode() & 0o7777, 0o777);
            assert_eq!(read_native(&path).unwrap(), b"retained owner bytes");
            assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o600);
        }

        #[test]
        fn actual_hardlinked_sources_refuse_mutation_and_keep_both_readable_aliases() {
            let (root, path) = source();
            let alias = root.path().join("another-owner-path.json");
            std::fs::hard_link(&path, &alias).unwrap();
            let original = std::fs::read(&path).unwrap();
            for address in [&path, &alias] {
                let error =
                    NativeFileTransaction::acquire(address, &lock_name(address).unwrap(), false)
                        .err()
                        .expect(
                            "an alias must not acquire mutation authority under a different lock",
                        );
                assert_eq!(error.kind(), io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("single link"));
                // This is a write-admission refusal, not an erasure of existing read authority.
                assert_eq!(read_native(address).unwrap(), original);
            }
            assert_eq!(std::fs::metadata(&path).unwrap().nlink(), 2);
            assert_eq!(std::fs::read(&alias).unwrap(), original);
        }

        #[test]
        fn actual_hardlinked_locks_refuse_mutation_and_retain_all_lock_bytes() {
            let (root, path) = source();
            let lock = root.path().join(".state.json.lock");
            let alias = root.path().join("retained-lock-alias");
            std::fs::write(&lock, b"native lock evidence").unwrap();
            std::fs::hard_link(&lock, &alias).unwrap();
            let error = NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false)
                .err()
                .expect("a multiply addressed lock must not admit a writer");
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("single link"));
            assert_eq!(std::fs::read(&lock).unwrap(), b"native lock evidence");
            assert_eq!(std::fs::read(&alias).unwrap(), b"native lock evidence");
            assert_eq!(std::fs::metadata(&lock).unwrap().nlink(), 2);
            assert_eq!(read_native(&path).unwrap(), b"retained owner bytes");
        }

        #[test]
        fn actual_hardlinked_stage_refuses_publication_and_retains_both_candidate_names() {
            let (root, path) = source();
            let transaction =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            let alias = root.path().join("retained-candidate-alias");
            let alias_for_observer = alias.clone();
            let observed = std::sync::Arc::new(std::sync::Mutex::new(None::<PathBuf>));
            let observed_stage = observed.clone();
            AFTER_STAGE_PREPARED.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move |stage| {
                    std::fs::hard_link(stage, &alias_for_observer).unwrap();
                    *observed_stage.lock().unwrap() = Some(stage.to_path_buf());
                }));
            });
            let error = transaction
                .publish(b"uncommitted native candidate", false)
                .unwrap_err();
            assert!(matches!(error, PublicationError::Before(_)));
            let stage = observed.lock().unwrap().clone().unwrap();
            assert_eq!(std::fs::metadata(&stage).unwrap().nlink(), 2);
            assert_eq!(
                std::fs::read(&stage).unwrap(),
                b"uncommitted native candidate"
            );
            assert_eq!(
                std::fs::read(&alias).unwrap(),
                b"uncommitted native candidate"
            );
            drop(transaction);
            assert_eq!(read_native(&path).unwrap(), b"retained owner bytes");
            assert!(stage.is_file() && alias.is_file());
        }

        #[test]
        fn actual_postpublication_mode_drift_is_uncertain_for_replacement_and_bootstrap() {
            for bootstrap in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let path = root.path().join("private-state.json");
                if !bootstrap {
                    std::fs::write(&path, b"retained owner bytes").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                let transaction =
                    NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false)
                        .unwrap();
                AFTER_PUBLICATION.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(|published| {
                        std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                            .unwrap();
                    }));
                });
                let error = transaction
                    .publish(b"actual committed candidate", bootstrap)
                    .unwrap_err();
                assert!(matches!(error, PublicationError::Uncertain(_)));
                assert_eq!(read_native(&path).unwrap(), b"actual committed candidate");
                assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o777);
                drop(transaction);
                assert_eq!(read_native(&path).unwrap(), b"actual committed candidate");
                assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o777);
            }
        }

        #[test]
        fn actual_postpublication_xattr_drift_is_uncertain_for_replacement_and_bootstrap() {
            for bootstrap in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let path = root.path().join("state.json");
                if !bootstrap {
                    std::fs::write(&path, b"retained owner bytes").unwrap();
                }
                let transaction =
                    NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false)
                        .unwrap();
                #[cfg(target_os = "macos")]
                let key = CString::new("org.epilogos.factory-native-drift").unwrap();
                #[cfg(target_os = "linux")]
                let key = CString::new("user.factory-native-drift").unwrap();
                let changed_key = key.clone();
                AFTER_PUBLICATION.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(move |published| {
                        let file = File::open(published).unwrap();
                        set_attribute(&file, &changed_key, b"foreign postpublication attribute");
                    }));
                });
                let error = transaction
                    .publish(b"actual committed candidate", bootstrap)
                    .unwrap_err();
                assert!(matches!(error, PublicationError::Uncertain(_)));
                let file = File::open(&path).unwrap();
                assert_eq!(bytes(&file).unwrap(), b"actual committed candidate");
                assert_eq!(
                    attributes(&file).unwrap().get(key.as_bytes()).unwrap(),
                    b"foreign postpublication attribute"
                );
                drop(transaction);
                assert_eq!(read_native(&path).unwrap(), b"actual committed candidate");
                assert_eq!(
                    attributes(&file).unwrap().get(key.as_bytes()).unwrap(),
                    b"foreign postpublication attribute"
                );
            }
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn actual_macos_absent_acl_allows_bootstrap_and_replacement_but_not_bad_descriptor() {
            for bootstrap in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let path = root.path().join("state.json");
                if !bootstrap {
                    std::fs::write(&path, b"retained owner without ACL").unwrap();
                    let file = File::open(&path).unwrap();
                    assert!(acl(&file).unwrap().is_empty());
                }
                let transaction =
                    NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false)
                        .unwrap();
                transaction
                    .publish(b"candidate without ACL", bootstrap)
                    .unwrap();
                let file = File::open(&path).unwrap();
                assert!(acl(&file).unwrap().is_empty());
                let added = std::process::Command::new("/bin/chmod")
                    .args(["+a", "everyone allow read"])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    added.status.success(),
                    "{}",
                    String::from_utf8_lossy(&added.stderr)
                );
                assert!(!acl(&file).unwrap().is_empty());
                let removed = std::process::Command::new("/bin/chmod")
                    .arg("-N")
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    removed.status.success(),
                    "{}",
                    String::from_utf8_lossy(&removed.stderr)
                );
                assert!(acl(&file).unwrap().is_empty());
                drop(transaction);
                assert_eq!(read_native(&path).unwrap(), b"candidate without ACL");
            }
            // -1 cannot be reused by another parallel test as a live descriptor.
            // Exercise the native retrieval failure, not a manufactured io::Error.
            assert_eq!(
                acl_from_fd(-1).unwrap_err().raw_os_error(),
                Some(libc::EBADF)
            );
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn actual_postpublication_macos_acl_drift_is_uncertain_for_replacement_and_bootstrap() {
            for bootstrap in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let path = root.path().join("state.json");
                if !bootstrap {
                    std::fs::write(&path, b"retained owner bytes").unwrap();
                }
                let transaction =
                    NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false)
                        .unwrap();
                let observed_acl = std::sync::Arc::new(std::sync::Mutex::new(None));
                let changed_acl = observed_acl.clone();
                AFTER_PUBLICATION.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(move |published| {
                        let file = File::open(published).unwrap();
                        let before = acl(&file).unwrap();
                        let changed = std::process::Command::new("/bin/chmod")
                            .args(["+a", "everyone allow read"])
                            .arg(published)
                            .output()
                            .unwrap();
                        assert!(
                            changed.status.success(),
                            "{}",
                            String::from_utf8_lossy(&changed.stderr)
                        );
                        let after = acl(&file).unwrap();
                        assert_ne!(before, after, "the actual native ACL must change");
                        *changed_acl.lock().unwrap() = Some(after);
                    }));
                });
                let error = transaction
                    .publish(b"actual committed candidate", bootstrap)
                    .unwrap_err();
                assert!(matches!(error, PublicationError::Uncertain(_)));
                let file = File::open(&path).unwrap();
                assert_eq!(bytes(&file).unwrap(), b"actual committed candidate");
                let changed_acl = observed_acl.lock().unwrap().clone().unwrap();
                assert_eq!(acl(&file).unwrap(), changed_acl);
                drop(transaction);
                assert_eq!(acl(&file).unwrap(), changed_acl);
                assert_eq!(read_native(&path).unwrap(), b"actual committed candidate");
            }
        }

        fn source() -> (tempfile::TempDir, PathBuf) {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("state.json");
            std::fs::write(&path, b"retained owner bytes").unwrap();
            (root, path)
        }
        #[test]
        fn actual_symlink_and_fifo_sources_or_locks_are_refused_without_following() {
            let (root, path) = source();
            let original = std::fs::read(&path).unwrap();
            let alias = root.path().join("alias.json");
            symlink(&path, &alias).unwrap();
            assert!(
                NativeFileTransaction::acquire(&alias, &lock_name(&alias).unwrap(), false).is_err()
            );
            let lock = root.path().join(".state.json.lock");
            symlink(&path, &lock).unwrap();
            assert!(
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).is_err()
            );
            std::fs::remove_file(&lock).unwrap();
            let fifo = name(lock.as_os_str()).unwrap();
            // SAFETY: test-owned pathname and valid CString. This creates a real OS FIFO.
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
            let started = Instant::now();
            assert!(
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).is_err()
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        #[test]
        fn real_lock_contention_has_a_finite_deadline_and_recovers_after_release() {
            let (_root, path) = source();
            let first =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            let started = Instant::now();
            let error = NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false)
                .err()
                .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(started.elapsed() < LOCK_TIMEOUT + Duration::from_secs(1));
            drop(first);
            let second =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            second.publish(b"after release", false).unwrap();
            assert_eq!(read_native(&path).unwrap(), b"after release");
        }
        #[test]
        fn foreign_stage_collision_is_retained_and_never_unlinked() {
            let (root, path) = source();
            let transaction =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            let foreign = root.path().join(".foreign.tmp");
            std::fs::write(&foreign, b"foreign attributable bytes").unwrap();
            let failure = transaction
                .publish_named(b"candidate", false, OsStr::new(".foreign.tmp"))
                .unwrap_err();
            let PublicationError::Before(error) = failure else {
                panic!("a real O_EXCL collision must refuse before publication");
            };
            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
            let cause = std::error::Error::source(&error)
                .and_then(|cause| cause.downcast_ref::<io::Error>())
                .expect("the actual syscall error must remain in the source chain");
            assert_eq!(cause.raw_os_error(), Some(libc::EEXIST));
            assert!(error.to_string().contains(".foreign.tmp"));
            assert_eq!(
                std::fs::read(foreign).unwrap(),
                b"foreign attributable bytes"
            );
            assert_eq!(read_native(&path).unwrap(), b"retained owner bytes");
        }
        #[test]
        fn held_parent_and_source_refuse_retargeting_before_publication() {
            let (root, path) = source();
            let transaction =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            let displaced = root.path().with_extension("displaced");
            std::fs::rename(root.path(), &displaced).unwrap();
            std::fs::create_dir(root.path()).unwrap();
            std::fs::write(&path, b"foreign directory state").unwrap();
            assert!(transaction.publish(b"candidate", false).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"foreign directory state");
            assert_eq!(
                std::fs::read(displaced.join("state.json")).unwrap(),
                b"retained owner bytes"
            );
            drop(transaction);
            std::fs::remove_dir_all(displaced).unwrap();
        }
        #[test]
        fn existing_parent_alias_is_bound_and_retargeting_it_refuses_publication() {
            let (root, path) = source();
            let aliases = tempfile::tempdir().unwrap();
            let alias = aliases.path().join("provider-parent");
            symlink(root.path(), &alias).unwrap();
            let addressed = alias.join("state.json");
            let admitted =
                NativeFileTransaction::acquire(&addressed, &lock_name(&addressed).unwrap(), false)
                    .unwrap();
            admitted
                .publish(b"through bound directory alias", false)
                .unwrap();
            assert_eq!(
                read_native(&path).unwrap(),
                b"through bound directory alias"
            );
            drop(admitted);
            let held =
                NativeFileTransaction::acquire(&addressed, &lock_name(&addressed).unwrap(), false)
                    .unwrap();
            let foreign = tempfile::tempdir().unwrap();
            std::fs::write(foreign.path().join("state.json"), b"foreign alias target").unwrap();
            std::fs::remove_file(&alias).unwrap();
            symlink(foreign.path(), &alias).unwrap();
            assert!(held.publish(b"must not publish", false).is_err());
            assert_eq!(
                read_native(&path).unwrap(),
                b"through bound directory alias"
            );
            assert_eq!(read_native(&addressed).unwrap(), b"foreign alias target");
        }
        #[test]
        fn actual_lock_inode_replacement_is_refused_without_deleting_foreign_lock() {
            let (root, path) = source();
            let held =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            let lock = root.path().join(".state.json.lock");
            std::fs::remove_file(&lock).unwrap();
            std::fs::write(&lock, b"foreign lock inode").unwrap();
            assert!(held.publish(b"must not publish", false).is_err());
            assert_eq!(std::fs::read(&lock).unwrap(), b"foreign lock inode");
            assert_eq!(read_native(&path).unwrap(), b"retained owner bytes");
        }
        #[test]
        fn read_only_source_is_not_bypassed_by_parent_rename() {
            let (_root, path) = source();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
            assert_eq!(
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false)
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert_eq!(read_native(&path).unwrap(), b"retained owner bytes");
        }
        fn set_attribute(file: &File, key: &CStr, value: &[u8]) {
            // SAFETY: test-owned held fd, live key/value; real native xattr syscall.
            let result = unsafe {
                #[cfg(target_os = "macos")]
                {
                    libc::fsetxattr(
                        file.as_raw_fd(),
                        key.as_ptr(),
                        value.as_ptr().cast(),
                        value.len(),
                        0,
                        0,
                    )
                }
                #[cfg(target_os = "linux")]
                {
                    libc::fsetxattr(
                        file.as_raw_fd(),
                        key.as_ptr(),
                        value.as_ptr().cast(),
                        value.len(),
                        0,
                    )
                }
            };
            assert_eq!(result, 0, "{}", io::Error::last_os_error());
        }
        #[test]
        fn actual_private_mode_uid_gid_xattrs_and_acl_survive_replacement() {
            let (_root, path) = source();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            #[cfg(target_os = "macos")]
            {
                let account = std::env::var("USER").expect("native test account");
                let output = std::process::Command::new("/bin/chmod")
                    .arg("+a")
                    .arg(format!("{account} allow read,write"))
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            #[cfg(target_os = "macos")]
            let key = CString::new("org.epilogos.factory-native-retention").unwrap();
            #[cfg(target_os = "linux")]
            let key = CString::new("user.factory-native-retention").unwrap();
            set_attribute(&file, &key, b"retained private provenance");
            let before = Privacy::read(&file).unwrap();
            let transaction =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            transaction
                .publish(b"native changed owner bytes", false)
                .unwrap();
            let reloaded = File::open(&path).unwrap();
            assert_eq!(Privacy::read(&reloaded).unwrap(), before);
            assert_eq!(bytes(&reloaded).unwrap(), b"native changed owner bytes");
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn actual_extended_posix_acl_survives_native_replacement() {
            let (_root, path) = source();
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            // Linux's real POSIX ACL xattr encoding: version 2 and native ACL
            // entries. The named principal is this process's actual account.
            let mut access = 2_u32.to_le_bytes().to_vec();
            // SAFETY: geteuid observes this process; it does not change identity.
            let account = unsafe { libc::geteuid() };
            for (tag, permissions, id) in [
                (1_u16, 6_u16, u32::MAX),
                (2, 4, account),
                (4, 0, u32::MAX),
                (16, 4, u32::MAX),
                (32, 0, u32::MAX),
            ] {
                access.extend_from_slice(&tag.to_le_bytes());
                access.extend_from_slice(&permissions.to_le_bytes());
                access.extend_from_slice(&id.to_le_bytes());
            }
            let key = CString::new("system.posix_acl_access").unwrap();
            set_attribute(&file, &key, &access);
            let before = Privacy::read(&file).unwrap();
            assert_eq!(before.attributes.get(key.as_bytes()), Some(&access));
            assert_eq!(
                before.mode, 0o640,
                "the actual ACL mask must affect native mode"
            );
            let old_inode = file.metadata().unwrap().ino();
            let transaction =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            transaction
                .publish(b"candidate retaining actual POSIX ACL", false)
                .unwrap();
            let reloaded = File::open(&path).unwrap();
            assert_ne!(old_inode, reloaded.metadata().unwrap().ino());
            assert_eq!(Privacy::read(&reloaded).unwrap(), before);
            assert_eq!(
                bytes(&reloaded).unwrap(),
                b"candidate retaining actual POSIX ACL"
            );
        }
        #[test]
        #[ignore = "requires explicit hosted native-privacy gate and real superuser fixture authority"]
        fn actual_foreign_uid_gid_privacy_is_preserved_by_privileged_native_publication() {
            // The gate invokes only this test as root, with the real unprivileged
            // runner uid/gid. Authority applies only to this owned TempDir fixture.
            assert_eq!(
                unsafe { libc::geteuid() },
                0,
                "real OS fixture authority required"
            );
            let uid: u32 = std::env::var("FACTORY_NATIVE_TEST_SOURCE_UID")
                .unwrap()
                .parse()
                .unwrap();
            let gid: u32 = std::env::var("FACTORY_NATIVE_TEST_SOURCE_GID")
                .unwrap()
                .parse()
                .unwrap();
            assert_ne!(uid, 0, "source must have an actual different owner");
            assert_ne!(gid, 0, "source must have an actual different group");
            let (_root, path) = source();
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            // SAFETY: held test-owned fd, real runner identities supplied by the gate.
            assert_eq!(
                unsafe { libc::fchown(file.as_raw_fd(), uid, gid) },
                0,
                "{}",
                io::Error::last_os_error()
            );
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .unwrap();
            #[cfg(target_os = "macos")]
            let key = CString::new("org.epilogos.factory-native-foreign-owner").unwrap();
            #[cfg(target_os = "linux")]
            let key = CString::new("user.factory-native-foreign-owner").unwrap();
            set_attribute(&file, &key, b"actual foreign-owner provenance");
            let before = Privacy::read(&file).unwrap();
            assert_eq!((before.uid, before.gid), (uid, gid));
            let transaction =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            transaction
                .publish(b"candidate retaining actual uid and gid", false)
                .unwrap();
            let reloaded = File::open(&path).unwrap();
            assert_ne!(
                file.metadata().unwrap().ino(),
                reloaded.metadata().unwrap().ino()
            );
            assert_eq!(Privacy::read(&reloaded).unwrap(), before);
            assert_eq!(
                bytes(&reloaded).unwrap(),
                b"candidate retaining actual uid and gid"
            );
        }
        #[test]
        fn bootstrap_is_no_clobber_and_does_not_leave_a_hardlink_alias() {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("new.json");
            let first =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            first.publish(b"native bootstrap", true).unwrap();
            assert_eq!(std::fs::metadata(&path).unwrap().nlink(), 1);
            drop(first);
            let next =
                NativeFileTransaction::acquire(&path, &lock_name(&path).unwrap(), false).unwrap();
            assert!(next.publish(b"replacement bootstrap", true).is_err());
            assert_eq!(read_native(&path).unwrap(), b"native bootstrap");
            assert!(std::fs::read_dir(root.path()).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")));
        }
    }
}
#[cfg(unix)]
pub(crate) use unix::{lock_name, read_native, NativeFileTransaction};

#[cfg(not(unix))]
pub(crate) struct NativeFileTransaction;
#[cfg(not(unix))]
impl NativeFileTransaction {
    pub(crate) fn acquire(_: &Path, _: &OsStr, _: bool) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native descriptor publication requires a supported Unix host",
        ))
    }
    pub(crate) fn current(&self) -> Option<&[u8]> {
        None
    }
    pub(crate) fn publish(&self, _: &[u8], _: bool) -> Result<(), PublicationError> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native descriptor publication requires a supported Unix host",
        )
        .into())
    }
}
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
pub(crate) use unix::{observe_next_privacy_copy, observe_next_publication};

#[cfg(not(unix))]
pub(crate) fn lock_name(_: &Path) -> io::Result<std::ffi::OsString> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native lock requires a supported Unix host",
    ))
}
#[cfg(not(unix))]
pub(crate) fn read_native(_: &Path) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native descriptor reading requires a supported Unix host",
    ))
}
