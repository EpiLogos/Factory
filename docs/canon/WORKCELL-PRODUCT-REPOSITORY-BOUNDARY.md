# Workcell Product / Repository Boundary

**Status:** current implementation-location clarification  
**Date:** 2026-08-14  
**Implementation repository:** `EpiLogos/Workcell`

Workcell is a **distinct EpiLogos product** implemented in `EpiLogos/Workcell`.

Where the Factory corpus calls Workcell a module or subsystem, that language describes its architectural function in the larger composition. It does not assign Workcell product-code ownership to `EpiLogos/Factory`.

The stable boundary is:

```text
Factory / semantic client
  -> provider-neutral ExecutionDemand
EpiLogos/Workcell
  -> provider resolution
  -> MaterialisedExecutionWorld + Binding/provenance
  -> execute / observe / collect / release
semantic result/evidence -> semantic owner
```

## Ownership

The Factory retains shared semantic meaning and integration acceptance. Workcell owns its public API, provider-neutral materialisation contracts, resolver/provider model, Workcell offers, bindings, materialisation lifecycle, material provenance, deterministic/reference providers, real provider adapters, conformance suite and repository verification.

The existing invariants remain unchanged:

- `ExecutionDemand` is provider-neutral.
- `CandidateMaterialisationDemand` is a specialised view/constructor over `ExecutionDemand`.
- `MaterialisedExecutionWorld` is a Workcell-owned material result.
- `Binding` is ephemeral provider-resolution state, not semantic identity.
- `Host` is not `Project`.
- provider/worktree/container/VM/process identifiers do not become Factory semantic identity.
- Workcell may materially realise a Context or Candidate but does not own their canonical meaning.
- provider replacement changes materialisation/availability, not canonical identity.
- AIKit may expose availability but is not the Workcell scheduler.
- Factory decides developmental meaning; Workcell realises material execution.

## Ticket interpretation

- Factory #11 (`RC-010`) owns the shared semantic contract; Workcell consumes it.
- Factory #24 (`SI-009`) supplies provider source-inspection evidence; provider adapter code belongs in `EpiLogos/Workcell`.
- Factory #57-#61 (`S4`) are Factory-side integration/acceptance gates over the separate Workcell product, not Workcell code-location tickets.
- Factory #83 continues to enforce the boundary.
- Factory #113 defines the cross-repository fixture floor; Workcell imports the applicable fixtures without becoming their semantic owner.
- Factory #114 supplies shared verification/Closure/Gate semantics where applicable; Workcell still owns one thin repository verification operation.

## Development rule

Workcell may implement Workcell-owned behaviour now where shared semantic references can remain opaque. It must not invent unfinished Factory semantics. Cross-repository conformance closes only when the relevant #113 fixture subset is stable.

This clarification changes implementation ownership, not the settled materialisation semantics of `QL-SOFTWARE-FACTORY-WORKCELL-MODULE-SPEC.md`.
