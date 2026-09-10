"""One-shot source edit for this PR; removed after the product commit.
No compiler download, binary transfer, fixture execution or acceptance shortcut.
Every replacement is bounded to inspected source; concurrent changes fail closed.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
pending = {}

def read(path):
    return (ROOT / path).read_text()

def once(text, old, new):
    assert text.count(old) == 1, f"source anchor changed: {old[:100]!r}"
    return text.replace(old, new, 1)

def region(text, start, end, replacement):
    assert text.count(start) == 1, f"start changed: {start}"
    left = text.index(start)
    right = text.index(end, left)
    return text[:left] + replacement + text[right:]

def save(path, content):
    pending[path] = content

libpath = 'factory/src/lib.rs'
lib = read(libpath)
if 'pub mod attempt_native_store;' in lib:
    print('Native integration source edits already applied.')
    raise SystemExit(0)
save(libpath, once(lib, 'pub mod attempt_runtime;', 'pub mod attempt_native_store;\npub mod attempt_runtime;'))

path = 'factory/src/developmental_read.rs'
s = read(path)
s = once(s, '    pub developmental_mutations: Vec<FactoryDevelopmentalMutationRecord>,', '''    pub developmental_mutations: Vec<FactoryDevelopmentalMutationRecord>,
    /// Native coordinator metadata only; the Run remains in Build's registry.
    #[serde(default)]
    pub attempt_states: BTreeMap<RunRef, crate::attempt_native_store::FactoryRunAttempts>,''')
s = once(s, '            developmental_mutations: Vec::new(),', '            developmental_mutations: Vec::new(),\n            attempt_states: BTreeMap::new(),')
save(path, s)

path = 'factory/src/build.rs'
s = read(path)
s = once(s, '    fn request_more_evidence(\n', '''    /// Commit the existing coordinator's Run under the provider's locked CAS.
    /// Semantic identity, writer ownership and unrelated cognition cannot drift.
    pub(crate) fn replace_attempt_run(
        &mut self,
        expected_revision: Revision,
        next: Run,
    ) -> Result<(), FactoryBuildError> {
        let reference = next.reference().clone();
        let current = self.runs.get(&reference)
            .ok_or_else(|| FactoryBuildError::RunNotFound(reference.to_string()))?;
        if current.revision() != expected_revision {
            return Err(RunContractError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision(),
            }.into());
        }
        if next.project_ref() != current.project_ref()
            || next.destination() != current.destination()
            || next.lifecycle() != current.lifecycle()
            || next.write_authority() != current.write_authority()
            || next.thought_field() != current.thought_field()
            || next.revision().get() < current.revision().get()
        {
            return Err(RunContractError::CorruptRun.into());
        }
        next.validate()?;
        let revision = self.revision.next().ok_or(FactoryBuildError::RevisionOverflow)?;
        *self.runs.get_mut(&reference).expect("canonical Run checked above") = next;
        self.revision = revision;
        Ok(())
    }

    fn request_more_evidence(
''')
save(path, s)

path = 'factory/src/orchestration_persistence.rs'
s = read(path)
s = once(s, 'impl OrchestrationSnapshot {\n', '''impl OrchestrationSnapshot {
    pub(crate) fn run_ref(&self) -> &RunRef {
        &self.run_ref
    }

    pub(crate) fn writer_reservations(&self) -> &BTreeMap<String, WorkflowUnitRef> {
        &self.active_writers
    }

''')
save(path, s)

path = 'factory/src/attempt_runtime.rs'
s = read(path)
s = region(s, '#[derive(Debug)]\npub struct FileAttemptStore {', '\nfn apply_operation(', 'pub use crate::attempt_native_store::FileAttemptStore;\n')
s = region(s, 'fn read_state(path: &Path)', '\nfn required_text(', '')
s = s.replace('use fs2::FileExt;\n', '').replace('use std::fs::{self, OpenOptions};', 'use std::fs;').replace('use std::io::{self, Read, Write};', 'use std::io::{self, Read};').replace('use std::path::{Path, PathBuf};', 'use std::path::PathBuf;')
start = s.index('struct StoredAttemptState {')
end = s.index('\n}', start) + 2
block = s[start:end]
block = block.replace('struct StoredAttemptState', 'pub(crate) struct StoredAttemptState')
import re
block = re.sub(r'^    (\w+:)', r'    pub(crate) \1', block, flags=re.M)
block = block[:-2] + '''
    #[serde(default)]
    pub(crate) action_receipts: BTreeMap<String, PersistedAttemptAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PersistedAttemptAction {
    pub(crate) request_digest: String,
    pub(crate) request: FactoryAttemptActionRequest,
    pub(crate) receipt: FactoryAttemptActionReceipt,
}'''
s = s[:start] + block + s[end:]
s = once(s, '    pub workflow_source_digest: String,', '    pub workflow_source_digest: String,\n    pub source_current: bool,')
s = once(s, '        workflow_source_digest: engine.workflow().source.digest.clone(),', '        workflow_source_digest: engine.workflow().source.digest.clone(),\n        source_current: true,')
for name in ['apply_operation', 'validate_action_request', 'validate_state', 'reading_for']:
    s = once(s, '\nfn ' + name + '(', '\npub(crate) fn ' + name + '(')
s = s.replace('request.authority.native_owner != "factory"', 'request.authority.native_owner != crate::build::FACTORY_NATIVE_OWNER')
# Preserve the concurrently repaired current_attempt_for_unit implementation.
assert 'fn current_attempt_for_unit' in s
start = s.index('pub(crate) fn validate_state(')
end = s.index('\npub(crate) fn reading_for(', start)
block = s[start:end]
block = once(block, '    for record in state.attempts.values() {', '''    for (reference, record) in &state.attempts {
        if reference != &record.attempt_ref
            || record.disposition.selection.demand.project_ref != state.run.project_ref().to_string()
        {
            return Err(FactoryAttemptError::CorruptState("attempt key or Project identity drift".into()));
        }''')
last = block.rfind('    Ok(())')
assert last >= 0
block = block[:last] + '''    for (reference, applied) in &state.action_receipts {
        if reference != &applied.request.projection_ref
            || reference != &applied.receipt.projection_ref
            || applied.receipt.run_ref != *state.run.reference()
            || applied.request.run_ref != *state.run.reference()
            || applied.receipt.previous_revision.checked_add(1) != Some(applied.receipt.next_revision)
            || applied.receipt.next_revision > state.revision
            || applied.request_digest != blake3::hash(&serde_json::to_vec(&applied.request)?).to_hex().to_string()
        {
            return Err(FactoryAttemptError::CorruptState("retained Action identity or revision drift".into()));
        }
    }
    crate::attempt_application::validate_reading(&reading_for(state)?)?;
''' + block[last:]
s = s[:start] + block + s[end:]
s = once(s, '        Some("init") => {', '''        Some("attach") => {
            let path = args.get(1).ok_or_else(|| FactoryAttemptError::Cli("missing native state path".into()))?;
            let run_ref = args.get(2).ok_or_else(|| FactoryAttemptError::Cli("missing canonical Run ref".into()))?
                .parse::<RunRef>().map_err(|error| FactoryAttemptError::Cli(error.to_string()))?;
            let source_ref = args.get(3).ok_or_else(|| FactoryAttemptError::Cli("missing admitted source ref".into()))?;
            render_reading(FileAttemptStore::attach(path, run_ref, source_ref)?.reading()?, json)
        }
        Some("init") => {''')
save(path, s)

path = 'factory/src/attempt_application.rs'
s = read(path)
s = s.replace('use serde::Deserialize;\n', '').replace('use std::collections::BTreeSet;', 'use std::collections::{BTreeMap, BTreeSet};')
s = region(s, 'pub fn apply_attempt_action(', '\nfn validate_reading(', '''pub fn apply_attempt_action(
    path: &Path,
    request: FactoryAttemptActionRequest,
) -> Result<FactoryAttemptActionReceipt, FactoryAttemptError> {
    FileAttemptStore::open_run(path, request.run_ref.clone())?.apply(request)
}

pub(crate) fn validate_native_action(
    reading: &FactoryAttemptReading,
    run: &Run,
    request: &FactoryAttemptActionRequest,
) -> Result<(), FactoryAttemptError> {
    validate_reading(reading)?;
    if reading.revision != request.expected_revision {
        return Err(FactoryAttemptError::RevisionConflict {
            expected: request.expected_revision,
            actual: reading.revision,
        });
    }
    validate_operation(reading, run, &request.operation)
}
''')
s = once(s, '\nfn validate_reading(', '\npub(crate) fn validate_reading(')
s = region(s, 'fn has_uncertain_operation(', '\nfn validate_fact(', '''fn has_uncertain_operation(record: &FactoryAttemptRecord) -> bool {
    let mut latest = BTreeMap::new();
    for receipt in record.dispatch.iter().chain(record.observations.iter()) {
        latest.insert((&receipt.owner_ref, &receipt.operation_ref), receipt.phase);
    }
    latest.values().any(|phase| matches!(phase,
        OwnerOperationPhase::Uncertain | OwnerOperationPhase::Dispatching))
}
''')
# The public read remains fresh and can explicitly select a Run in a shared store.
s = once(s, 'let reading = read_attempts(Path::new(path))?;', '''let reading = if let Some(run_ref) = positional.get(2) {
                FileAttemptStore::open_run(Path::new(path), run_ref.parse::<crate::core::run::RunRef>()
                    .map_err(|error| invalid(&error.to_string()))?)?.reading()?
            } else { read_attempts(Path::new(path))? };''')
save(path, s)

path = 'factory/src/attempt_native_store.rs'
s = read(path).replace('use std::path::{Path, PathBuf};', 'use std::path::PathBuf;')
save(path, s)

path = 'factory/tests/attempt_public_regressions.rs'
s = read(path).replace('use std::path::{Path, PathBuf};', 'use std::path::PathBuf;')
s = s.replace('stored["run"]["writeAuthority"]', 'stored["state"]["build"]["runs"]["runs"][RUN]["writeAuthority"]')
s = s.replace('state["snapshot"]', 'state["state"]["attemptStates"][RUN]["snapshot"]')
s = s.replace('state["workflowSource"]', 'state["state"]["attemptStates"][RUN]["workflowSource"]')
save(path, s)

# No file changes are published until all inspected source anchors matched.
for path, content in pending.items():
    (ROOT / path).write_text(content)
    print('Updated', path)
