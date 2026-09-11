# QL / MEF Module — Vision and Design Specification

**Status:** AUTHORITATIVE TARGET DESIGN — executable QL/MEF module  
**Date:** 2026-08-13  
**Repository:** `EpiLogos/Factory`  
**Intended location:** `docs/canon/`  
**Authority:** subordinate to the QL canon and `QL-SOFTWARE-FACTORY-CONSTITUTIONAL-INDEX.md`; authoritative for the software/module boundary through which QL and MEF become executable and interoperable. It does not alter the QL canon, replace Factory primitives, or supersede the experimental `ql-agent-spec` runtime semantics.  
**Scope:** QL canon/kernel distinction, MEF manifold, shared references, provider/service APIs, refraction and relation semantics, provenance, operational parity, no-QL modularity, AIKit/Factory adapters, agent-runtime integration, experimental extensions, and development programme.

---

## 0. Executive determination

The QL/MEF module is the **standalone executable intermediary between QL/MEF canon and software systems which wish to operate with explicit QL semantics**.

The architecture distinguishes:

```text
QL CANON
    invariant formal / semantic bimba
        │
        ├──────────────┐
        ▼              ▼
EXECUTABLE QL       MEF MANIFOLD
KERNEL              canonical lenses/refraction
        │              │
        └──────┬───────┘
               ▼
       QL / MEF SERVICE
   versioned provider boundary
               │
      ┌────────┼─────────┐
      ▼        ▼         ▼
    AIKit    Factory   Agent runtimes
      │        │         │
      └────────┼─────────┘
               ▼
      software applications
```

The module has two simultaneous obligations:

1. **deep integration:** software can increasingly make genuine QL relations executable, inspectable, and consequential;
2. **strict modularity:** ordinary software remains coherent when the QL/MEF module is absent, disabled, or replaced by a fixture provider.

The module therefore does not turn QL into a mandatory application ontology. It receives references to existing objects and returns QL/MEF readings, relations, or operators while preserving the objects' canonical identity.

The concise rule is:

> **Alignment, not translation. Refraction, not renaming. Operational consequence, not decorative labels.**

The cross-product distinction between `Run`, `Closure`, and `Gate`, including verification-oriented Runs and the L3/L3′/L1/L4′ alignment, is clarified in the sibling canonical note `../RUN-CLOSURE-VERIFICATION-ALIGNMENT.md` and reflected in the runtime integration section of this package.

---

# Specification package

This directory is the authoritative executable QL/MEF module design, split into sectional files for implementation and review. Read in order:

1. `01-CONSTITUTION-AND-PRIMITIVES.md`
2. `02-PROVIDER-AND-SERVICE.md`
3. `03-MEF-MANIFOLD.md`
4. `04-MEF-BECOMING.md`
5. `05-OBJECTIVE-INTERNALITY.md`
6. `06-CLIENTS-AGENTS-AND-EXTENSIONS.md`
7. `07-IMPLEMENTATION-AND-LANGUAGE.md`
8. `08-DEVELOPMENT-PROGRAMME.md`
9. `09-ACCEPTANCE.md`

This package does not supersede QL canon or the experimental `ql-agent-spec`; it defines the standalone kernel/MEF/provider seam which those and other software systems may consume.
