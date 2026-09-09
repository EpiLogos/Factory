# Routine-backed Journey continuation

Factory can admit one accepted `aikit.routine-invocation-evidence/v1` envelope
into an existing active or paused Journey. The single provider transaction
creates one bounded Factory Run, links it to the Journey, and retains every AIKit
evidence field unchanged through a strict typed projection.

The standing remains exactly what AIKit established:

- proof is `current-on-supplied-basis`;
- authority is `owner-attested`;
- the envelope requests invocation; it does not prove an Action ran.

Factory does not independently renew external authority at admission. Actuation
must still revalidate current authority at the later execution boundary. A later
revocation can therefore prevent all material effect even though the bounded
developmental Run remains a truthful record of the obligation that was opened.

The relation preserves Journey, Run, Routine, Method, proof, trigger observation,
authority-validation and optional provider delivery refs. It does not create a
scheduler record, WorkflowUnit, Execution, Activity, Evidence, Return, Attention,
Flow or DAY record. Cancellation and partial-result provenance enter through the
existing execution/Activity/Return correlations only if those events actually
occur.

Identity and persistence laws:

- the Factory Run identity is deterministically minted from Journey identity,
  AIKit invocation identity and the observed occurrence time;
- exact replay returns the same Run without rewriting state;
- new delivery/restart receipts may accumulate on the same occurrence without a
  new Run;
- changed semantic replay, reused trigger identity, or cross-invocation delivery
  reuse fails without mutation;
- an exclusive reload-under-lock transaction publishes Build, Journey and
  continuation changes together using a synced same-directory atomic replacement.

The native commands are:

```text
factory development admit-routine-continuation <state> [request-file|-] --json
factory development routine-continuation <state> <invocation-ref> --json
factory conformance developmental-state <output> --json
```

The second command is the bounded Explain traversal for this tranche. Journey
and Run developmental readings also expose the related invocation refs. The
owner pin, exact upstream schema, fixture and consumer request schema live under
`contracts/factory/`; their conformance gate refuses drift.

The conformance command writes a deterministic, validated native provider state
and returns a versioned manifest containing every locator needed to exercise
project, Journey, Run, WorkflowUnit, execution-telemetry and continuation reads.
It is contract conformance state, not empirical execution history: its model and
material telemetry links are independently `unavailable` because the generator
supplies no Actuation or Workcell observation.
