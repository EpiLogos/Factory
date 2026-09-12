# Native Run thought consumption

Standing: implemented native Factory contract; #217 / QL-MEF#94 AW3 increment.

A retained report is useful when subsequent work can say what it learned from it,
which actual output or response tested it, and where the useful result now lives.
`factory.run-thought-consumption/v1` adds that transition to the existing
`RunThoughtField`. It does not introduce another thought store, twelve file
buckets, a source writer, an agent loop or training authority.

`Run::apply_thought_consumption` and
`FactoryBuildState::apply_run_thought_consumption` use the same Run mutation
authority, expected Run revision and Build mutation path as thought retention.
The native cognitive view exposes consumption refs and complete receipts only
when all their input reports are within the selected view. A narrow projection
does not expand into unrelated reports through a shared receipt.

A consumption names the exact retained anchors/revisions, a revisioned working
field (Central NOW for Epi), consumer, actual retrieval/output/execution evidence,
assessment, optional human response, useful native outputs and their receiving
or Recognition evidence, and the native retention policy. Each source is checked
through `ThoughtConsumptionSources` before any lifecycle changes. Implementations
must inspect the actual native source owner, not echo the requested revision.
The observer is a read-only port, not permission to mutate the observed source.

Useful outputs may be context, Wiki readings, evaluations, Skill improvements,
recognised praxis or a continuing question. An output already belongs to its
native owner. Storing a consumption does not create that output, recognise a
Method, promote a candidate, change a human source or train a model. For Epi,
the optional source-qualified interpretation retains each T/T-prime meaning;
Factory does not hardcode or rename QL's vocabulary. M4.5 and M5 remain distinct.

All inputs must still be active and must not already have consumption lineage.
Missing, changed, denied or inconsistent source observations refuse the whole
transition. Consumption updates lifecycle and excludes those reports from the
active view, but retains their original anchors, producers and exact lineage.
The original report bytes remain with their owner. Exact command replay is
idempotent even after the world changes; conflicting reuse of an identity is
rejected. A subsequent use requires a fresh native source observation rather
than treating historical currentness as a new lease.

Validation of reloaded Run records rejects missing/duplicate observations,
changed anchors, mismatched Run/command IDs, repeated consumption and altered
lifecycle. RunMap topology does not change. Native Build publication stages the
Run so revision overflow cannot partially consume inputs.

Tests: `factory/tests/thought_consumption.rs`, retained
`factory/tests/run_thought.rs` and the existing cognitive/Build/native suites.
The controlled owner fixtures prove the native transition and refusal laws,
including retention of all twelve T/T-prime source references. They do not prove
a live Epii model consumed all twelve meanings, authored useful new Skills or
completed the wider AW3 named-praxis loop. Those producer/consumer joins remain
#94 / #267 / #217 work. Owner-machine harmonisation is a separate later receipt.
