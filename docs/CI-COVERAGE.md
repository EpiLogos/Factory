# Native CI ownership and coverage

The required `factory-rust` check remains present on every pull request. Branch
pushes do not duplicate PR runs. Superseded PR checks are cancellable; main
publication runs are not automatically interrupted.

## Consolidation, 11 September 2026

| Previous lane | Retained obligation | Current owner |
|---|---|---|
| Factory Rust | format, all-target Clippy, all-target tests, intake, Routine, Commission | `factory-rust.yml` / `factory-rust` |
| Factory native Skills | native Skill validator plus the same Rust checks | same job, validator before compilation |
| Pre-local build / verify | workspace tests including doctests | same job, all targets plus explicit `--doc` |
| Pre-local build / artifact | exact-main source component, checksum, attestation | same workflow / `artifact`, needs `factory-rust` |

The root Cargo workspace owns `target/`, not `factory/target/`. Cache configuration
uses `. -> target`. No test result or executable is reused across unrelated
revisions. `scripts/tests/test_ci_workflows.py` guards the transferred obligations.

Product maintenance stays advisory over the live matrix; this does not weaken
native runtime checks. Cross-product native-attempt tests on active CAW branches
are additional integration obligations, not permission to merge unfinished work.

The separate interop, UI and experiment lanes are not retired here: their unique
coverage has not been moved into this Rust gate. Suite candidate acceptance stays
with O:I and is not inferred from a native-only green result.
