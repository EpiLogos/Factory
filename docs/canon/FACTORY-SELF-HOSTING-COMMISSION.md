# Factory self-hosting Commission

Factory owns the developmental consequence of a Commission: stable Project,
Journey and initial Run identities, their atomic persistence, and a public read
of the relation. The request is evidence, not an identity receipt supplied by a
consumer. Its `writeOwner` is therefore fixed to `factory`.

Central composition is retained as exact record ref, revision, source path and
SHA-256 provenance. Resolved Agent refs are membership evidence only. The
required `membership-non-authoritative` standing prevents membership from being
misread as provider, execution or Actuation authority. The bounded root act is
stored as `commissioned-not-executed`.

```text
factory development commission <state> [request-file|-] --json
factory development commission-read <state> <request-ref> --json
factory development mutate <state> [request-file|-] --json
```

New state creation refuses an existing output. Appends reload under an exclusive
provider lock and publish by atomic replacement. Exact request, occurrence and
source replay collapses; a changed replay or a foreign Project/Journey/Run ref
fails without mutation.

The mutation request is a closed tagged union. It can attach a native
`WorkflowSource`, correlate an owner Activity, or record the existing Journey
Return/Recognition models. It cannot carry arbitrary JSON or create a parallel
execution ontology. Every retained mutation record is revalidated against its
current canonical effect when the provider reopens.

`contracts/factory/fixtures/oi-self-hosting-state.json` is a deterministic state
created only through the native Commission and mutation commands. Its distinct
Factory Guardian, AIKit Guardian and O:I snapshot/#188 units remain `ready` or
`planned`; empty Agency and Execution registries are part of its truthful
standing.
