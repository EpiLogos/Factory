# Factory sensing operations

`factory telemetry` reads and extends the existing developmental provider. It
does not create a second task database. Source observations, coverage receipts,
decisions and work/Return relations live in that provider; raw GitHub and native
owner payloads remain at their owners. This implements the telemetry extension
of O:I #65/#220 and Factory #195/#199/#222.

## Read and collect

```sh
factory telemetry policy STATE --policy ProjectCentral/user/factory-policy.json --json
factory telemetry collect STATE --policy ProjectCentral/user/factory-policy.json --last-days 7 --json
factory telemetry signals STATE --json
factory telemetry signal STATE SIGNAL_REF --json
factory telemetry field STATE --json
factory telemetry digest STATE --json
factory telemetry day STATE --day 2026-09-24 --json
factory telemetry lookback STATE --from-day 2026-09-20 --through-day 2026-09-24 --compare-previous --json
factory telemetry stats STATE --last-days 7 --compare-previous --json
```

Named Days use Central's actual civil-time policy, including timezone, boundary
and DST. Explicit `--since`/`--until` RFC3339 timestamps provide a half-open
interval for sensing reads. They cannot be mixed with named Days. These commands
never write human Day prose or invoke a model to advance the calendar.

The native Factory adapter reads failed Attempts, failed verification receipts,
explicit retry ancestry, completed custody without its readable Return, missing
child NOW joins, blocked custody and incomplete temporal correlations. Blocked
custody supplies the explicit stalled-work basis; age or silence alone does not.
Each verification retains
its own occurrence time. Missing producer timing is disclosed as partial
temporal coverage. GitHub adapters enumerate issue headers and workflow runs;
discussion remains linked at GitHub. Workflow-run windows use creation time,
not an invented completion timestamp. Configured pagination limits and provider
failures remain visible. Other providers are unavailable until a real adapter
exists; naming Sentry in policy does not install Sentry.

`empty`, `unavailable`, and `truncated` have distinct meanings. Coverage of a
still-open Day reaches only as far as the collection's actual observation time.
Affected people/sessions and unprovided costs remain unknown. Lookback groups
only by an explicitly evidenced boundary relation and searches prior Returns
outside the selected period. A similar title is not recurrence.

## Project policy

The source is `ProjectCentral/user/factory-policy.json`, schema
`factory.sensing-policy/v1`, version 1. The checked-in Factory policy is a concrete
example. A native Central Project link binds its ProjectWorld; an explicit
`--policy` never bypasses that scope check. Repository and provider scopes are
declared separately. Collection must be enabled and name existing source IDs.

Builder's repositories, sources, workflows, independent action policies,
worktree preferences and skill overlays remain available to the adapted skills.
Only the native operations described here execute automatically. A policy
overlay never grants action authority. YAML is migration input, not a second
silently incomplete parser: explicitly account for unsupported choices before
converting an existing `.agent-factory/config.yaml`.

AIKit's project-bound `factory-collect` and `factory-field-refresh` native
Methods bind a saved Routine to the exact state, policy and ProjectWorld.
Schedules use `every:<milliseconds>`, `daily:<HH:MM>` or `cron:<five fields>`.
The runner rechecks enabled policy and saved cadence, then publishes a bounded
Redis projection with compare-and-swap. Redis loss is rebuilt from native
owners. Collection does not launch coding work by itself.

## Classify and commission

```sh
factory telemetry classify STATE --request decision.json --json
factory telemetry commission STATE --policy POLICY --request commission.json --json
factory telemetry return STATE --request return.json --json
```

Each mutation request includes `authority_request`, the ordinary
`actuation.local-authority/v1` resolution request. Factory calls the installed
Actuation authority owner. Requested bounds must contain both
`bound:factory-sensing:<world>` and an independently granted action bound such as
`bound:factory-sensing:project:Factory:telemetry.classify`. Allowed/denied action
refs must admit `factory:action/telemetry.classify` (or `commission`/`return`).
Unknown, revoked, expired or wrong-World authority refuses. A policy boolean or
caller-authored authority label is insufficient.

Classification requires `signal_ref`, `expected_revision`, `source_revision`,
`classification`, `evidence_refs` and `reason`. A verified defect requires
reproduction refs. A human decision requires `decision_needed`, the actual
question. Optional `boundary_ref` identifies an evidenced common failure
boundary. Changing the source revision invalidates its prior classification.

Commission additionally requires `position_ref`, `criteria_evidence`,
`stop_conditions_present` and a native `factory.commission-request/v1` in
`commission`. `criteria_evidence` maps each configured criterion to a nonempty array of evidence refs;
a configured stop holds the action. The native Commission must retain the
resolved Agent and originating signal. Admission and ordinary custody creation
share one transaction. Before new admission the configured native/provider
source is reread; a changed revision requires collection and classification
again. Replay returns the same work/Run. Child NOW allocation
joins the actual Workcell root NOW; an unavailable cross-owner join retains the
committed work and an explicit retry condition.

## Return and delivery evidence

A signal Return includes its `return_ref`, `attempt_ref`, `verification_ref`,
`source_revision`, `change_revision`, `evidence_refs` and `outcome`. The native
Run must already contain that Attempt and readable Return, preserving both the
signal and original source refs. Verification must be an owner-admitted passed
receipt bound to the exact changed revision. The receipt must be the latest
verification for that Attempt; a later failed or unknown result invalidates an
older success. Stale sensing revisions and reused
Return identities with different facts refuse.

`implemented`, `verified`, `merged`, `installed`, `live-resolved` and `unresolved`
remain separate outcomes. Live resolution additionally requires
`installed_revision`, `running_revision` and `live_evidence_ref`, corroborated
by the existing native verification's `installed:<revision>`,
`running:<revision>` and live evidence refs. Caller strings alone cannot resolve
the signal. Human Recognition is never inferred from those receipts.

A merged outcome also requires `pull_request_ref` and `expected_pr_head`.
Factory rereads the exact GitHub PR, checks the configured repository, merged
state, current head and merge commit against `change_revision`, and retains
that provider reading. Its native verification must name
`merge:<change_revision>`. An installed outcome requires `installed_revision`
and matching native verification evidence. One readable Return may acquire
separate immutable stage receipts; a merge receipt never implies installation.

The digest is a read-only queue: unresolved human questions survive beyond the
day they were first observed. It cannot assign, approve, reply, close, merge,
deploy, change policy or recognise a Return. The adapted review/babysit/ship
skills separately re-read live GitHub head, checks, review threads, authority
and installed behaviour before their respective effects.

## Skill source and verification

The ten Builder-derived entrypoints, licenses, exact source hashes and adaptation
diff are retained under `skills/`. `skills/factory/provenance/upstream.json`
pins the upstream cut. `scripts/check_factory_upstream.py` compares upstream
without modifying the installed source. `scripts/verify_factory_skill_projection.py`
exercises real AIKit source sync, promotion, discovery, SkillSet selection,
missing-member refusal and rollback in an isolated home.

Native sensing integration tests exercise real provider files and CLI dispatch.
Actuation and Redis integration tests require the actual native binaries/server;
their environment-gated absence is not live proof. Installed-World receipts,
Cradle walks and exact merged/installed revisions are retained with the campaign
Return, distinct from unit-test success.
