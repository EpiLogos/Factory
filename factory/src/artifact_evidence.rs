//! Freshness of explicitly scoped, local artifact evidence.
//!
//! Reuses the interop EvidenceAssessment subject-state tuple. This is a native
//! observation/admission boundary, not a filesystem lock, authority grant,
//! verification of artifact quality, or proof of whole-Run closure. Callers must
//! revalidate at each consequential use; concurrent writers need an execution
//! provider's immutable snapshot if atomic observation is required.

use crate::build::FactoryBuildError;
use crate::core::identity::{Ref, Revision};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// Exact shape from contracts/factory/interop/evidenceAssessment.schema.json.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubjectState {
    pub subject_ref: Ref,
    pub subject_revision: Revision,
    pub state_ref: String,
}

#[derive(Clone, Copy, Debug)]
pub struct ArtifactLimits {
    pub max_entries: usize,
    pub max_bytes: u64,
    pub max_depth: usize,
}
impl Default for ArtifactLimits {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            max_bytes: 64 * 1024 * 1024,
            max_depth: 64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEntry {
    pub kind: &'static str,
    pub bytes: u64,
    pub content_digest: Option<String>,
    pub permissions: u32,
}

/// Retain this manifest through the existing Evidence sourceRef/native_ref path.
/// No parallel evidence store is created. Paths are relative to the selected root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactManifest {
    pub schema: &'static str,
    pub scope: Vec<String>,
    pub entries: BTreeMap<String, ArtifactEntry>,
}

/// A source-derived observation. Private fields prevent caller-written digests
/// from masquerading as captured evidence; admission reads the scope again.
#[derive(Clone, Debug)]
pub struct ArtifactSnapshot {
    root: PathBuf,
    paths: Vec<PathBuf>,
    limits: ArtifactLimits,
    subject_state: SubjectState,
    manifest: ArtifactManifest,
}

impl ArtifactSnapshot {
    pub fn capture(
        subject_ref: Ref,
        subject_revision: Revision,
        root: &Path,
        paths: &[PathBuf],
        limits: ArtifactLimits,
    ) -> Result<Self, ArtifactEvidenceError> {
        if limits.max_entries == 0 || limits.max_bytes == 0 || limits.max_depth == 0 {
            return Err(ArtifactEvidenceError::InvalidScope(
                "limits must be positive".into(),
            ));
        }
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(ArtifactEvidenceError::InvalidScope(
                "root must be a directory".into(),
            ));
        }
        let paths = normalize_scope(paths)?;
        let manifest = scan(&root, &paths, limits)?;
        // Detect a moving selection, including directory membership. This is
        // deliberately not advertised as an atomic filesystem snapshot.
        if scan(&root, &paths, limits)? != manifest {
            return Err(ArtifactEvidenceError::ChangedDuringRead);
        }
        let encoded = serde_json::to_vec(&manifest).map_err(ArtifactEvidenceError::Encoding)?;
        let state_ref = format!(
            "factory.artifact-state.blake3/v1:{}",
            blake3::hash(&encoded).to_hex()
        );
        Ok(Self {
            root,
            paths,
            limits,
            subject_state: SubjectState {
                subject_ref,
                subject_revision,
                state_ref,
            },
            manifest,
        })
    }

    pub fn subject_state(&self) -> &SubjectState {
        &self.subject_state
    }
    pub fn manifest(&self) -> &ArtifactManifest {
        &self.manifest
    }

    /// Re-observe the original selection; a changed subject, revision, scope
    /// membership, permission or byte content makes the old evidence historical.
    pub fn validate_current(
        &self,
        assessment: &SubjectState,
        current_subject: &Ref,
        current_revision: Revision,
    ) -> Result<(), ArtifactEvidenceError> {
        if assessment != &self.subject_state {
            return Err(ArtifactEvidenceError::AssessmentSubjectMismatch);
        }
        if current_subject != &self.subject_state.subject_ref
            || current_revision != self.subject_state.subject_revision
        {
            return Err(ArtifactEvidenceError::StaleSubject);
        }
        let current = Self::capture(
            current_subject.clone(),
            current_revision,
            &self.root,
            &self.paths,
            self.limits,
        )?;
        if current.subject_state != self.subject_state {
            return Err(ArtifactEvidenceError::StaleArtifacts);
        }
        Ok(())
    }
}

fn normalize_scope(paths: &[PathBuf]) -> Result<Vec<PathBuf>, ArtifactEvidenceError> {
    if paths.is_empty() {
        return Err(ArtifactEvidenceError::InvalidScope(
            "select at least one file or directory".into(),
        ));
    }
    let mut normalized = Vec::new();
    for path in paths {
        if path == Path::new(".") {
            normalized.push(PathBuf::new());
            continue;
        }
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || path.to_str().is_none()
        {
            return Err(ArtifactEvidenceError::InvalidScope(
                "scope paths must be relative UTF-8 paths without parent traversal".into(),
            ));
        }
        normalized.push(path.clone());
    }
    normalized.sort();
    for (index, path) in normalized.iter().enumerate() {
        if normalized[..index]
            .iter()
            .any(|prior| path.starts_with(prior))
        {
            return Err(ArtifactEvidenceError::InvalidScope(
                "scope paths must not overlap".into(),
            ));
        }
    }
    Ok(normalized)
}

fn path_key(path: &Path) -> Result<String, ArtifactEvidenceError> {
    if path.as_os_str().is_empty() {
        return Ok(".".into());
    }
    path.components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str().map(str::to_owned).ok_or_else(|| {
                ArtifactEvidenceError::InvalidScope("artifact names must be UTF-8".into())
            }),
            _ => Err(ArtifactEvidenceError::InvalidScope(
                "invalid artifact path".into(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
}

fn scan(
    root: &Path,
    paths: &[PathBuf],
    limits: ArtifactLimits,
) -> Result<ArtifactManifest, ArtifactEvidenceError> {
    let mut manifest = ArtifactManifest {
        schema: "factory.artifact-manifest/v1",
        scope: paths
            .iter()
            .map(|p| path_key(p))
            .collect::<Result<_, _>>()?,
        entries: BTreeMap::new(),
    };
    let mut bytes = 0;
    for path in paths {
        // Reject symlinks in every selected ancestor, not just the leaf.
        let mut ancestor = root.to_owned();
        for component in path.components() {
            ancestor.push(component);
            if fs::symlink_metadata(&ancestor)?.file_type().is_symlink() {
                return Err(ArtifactEvidenceError::UnsupportedArtifact(path_key(path)?));
            }
        }
        visit(root, path, 0, limits, &mut bytes, &mut manifest.entries)?;
    }
    Ok(manifest)
}

fn visit(
    root: &Path,
    relative: &Path,
    depth: usize,
    limits: ArtifactLimits,
    bytes: &mut u64,
    entries: &mut BTreeMap<String, ArtifactEntry>,
) -> Result<(), ArtifactEvidenceError> {
    if depth > limits.max_depth || entries.len() >= limits.max_entries {
        return Err(ArtifactEvidenceError::LimitExceeded);
    }
    let path = root.join(relative);
    let before = fs::symlink_metadata(&path)?;
    let key = path_key(relative)?;
    if before.file_type().is_symlink() {
        return Err(ArtifactEvidenceError::UnsupportedArtifact(key));
    }
    if before.is_dir() {
        entries.insert(
            key,
            ArtifactEntry {
                kind: "directory",
                bytes: 0,
                content_digest: None,
                permissions: permissions(&before),
            },
        );
        let mut children = Vec::new();
        for entry in fs::read_dir(&path)? {
            if children.len() >= limits.max_entries.saturating_sub(entries.len()) {
                return Err(ArtifactEvidenceError::LimitExceeded);
            }
            children.push(entry?.file_name());
        }
        children.sort();
        for child in children {
            visit(
                root,
                &relative.join(child),
                depth + 1,
                limits,
                bytes,
                entries,
            )?;
        }
    } else if before.is_file() {
        let mut file = File::open(&path)?;
        if !same_file(&before, &file.metadata()?) {
            return Err(ArtifactEvidenceError::ChangedDuringRead);
        }
        let mut hash = blake3::Hasher::new();
        let mut count = 0u64;
        let mut buffer = [0; 16384];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            *bytes = bytes
                .checked_add(read as u64)
                .ok_or(ArtifactEvidenceError::LimitExceeded)?;
            if *bytes > limits.max_bytes {
                return Err(ArtifactEvidenceError::LimitExceeded);
            }
            count += read as u64;
            hash.update(&buffer[..read]);
        }
        if !same_file(&before, &file.metadata()?)
            || !same_file(&before, &fs::symlink_metadata(&path)?)
        {
            return Err(ArtifactEvidenceError::ChangedDuringRead);
        }
        entries.insert(
            key,
            ArtifactEntry {
                kind: "file",
                bytes: count,
                content_digest: Some(hash.finalize().to_hex().to_string()),
                permissions: permissions(&before),
            },
        );
    } else {
        return Err(ArtifactEvidenceError::UnsupportedArtifact(key));
    }
    Ok(())
}

fn permissions(metadata: &Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        u32::from(metadata.permissions().readonly())
    }
}
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    let same = a.file_type() == b.file_type()
        && a.len() == b.len()
        && a.modified().ok() == b.modified().ok()
        && permissions(a) == permissions(b);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        same && a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        same
    }
}

#[derive(Debug)]
pub enum ArtifactEvidenceError {
    InvalidScope(String),
    UnsupportedArtifact(String),
    LimitExceeded,
    ChangedDuringRead,
    AssessmentSubjectMismatch,
    StaleSubject,
    StaleArtifacts,
    Io(std::io::Error),
    Encoding(serde_json::Error),
    Build(FactoryBuildError),
}
impl From<std::io::Error> for ArtifactEvidenceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl fmt::Display for ArtifactEvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScope(reason) => write!(f, "invalid artifact scope: {reason}"),
            Self::UnsupportedArtifact(path) => write!(
                f,
                "unsupported artifact (only regular files/directories are admitted): {path}"
            ),
            Self::LimitExceeded => write!(f, "artifact scope exceeds declared limits"),
            Self::ChangedDuringRead => write!(f, "artifact scope changed during observation"),
            Self::AssessmentSubjectMismatch => write!(
                f,
                "assessment does not name the exact evidence subject state"
            ),
            Self::StaleSubject => {
                write!(f, "evidence names a different current subject or revision")
            }
            Self::StaleArtifacts => write!(
                f,
                "artifact bytes, membership or permissions changed since evidence was observed"
            ),
            Self::Io(error) => error.fmt(f),
            Self::Encoding(error) => error.fmt(f),
            Self::Build(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ArtifactEvidenceError {}
