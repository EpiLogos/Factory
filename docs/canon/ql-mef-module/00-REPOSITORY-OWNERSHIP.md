# QL/MEF Repository Ownership

**Decision date:** 2026-08-14

The standalone QL/MEF product now has a dedicated implementation repository:

`EpiLogos/QL-MEF`

This `docs/canon/ql-mef-module/` package in `EpiLogos/agent-system-design` is the governing cross-product target design according to the Factory Constitutional Index — settled, not pending. (#111, which proposed the package, closed 2026-08-22 without a tracked GitHub merge; the package's content is current canon regardless, and is already built on by `AGENT-CAPABILITY-INTAKE.md` and `contracts/factory/source-ground.json`.)

The ownership boundary is now explicit:

- `EpiLogos/QL-MEF` owns executable QL/MEF product code, product-local fixtures, implementation evidence, product branches/pull requests, release/readiness manifests, and the concrete Q1–Q7 implementation work.
- `EpiLogos/agent-system-design` owns Factory integration semantics, the cross-repository coordination/ticket graph, shared interoperability floor #113, Factory-side QL integration #77–#80, and the QL Loop Runtime.
- `EpiLogos/ai-kit` owns AIKit-side optional QL/MEF interoperability, including #30.

Factory Wayfinder #115 and stages #116–#122 remain the dependency/coordination programme, but they do **not** imply that standalone product implementation belongs in `agent-system-design`.

The QL Loop Runtime remains a distinct product/runtime concern. Q1–Q4 in QL/MEF do not depend on runtime #100. Q5 is the first stage consuming the frozen runtime seam; #100 is now closed and pinned.

> Alignment, not translation. Refraction, not renaming.
