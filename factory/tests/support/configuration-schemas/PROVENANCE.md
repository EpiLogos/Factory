# Provenance

These five JSON Schemas are copied verbatim from the frozen C0 contract of the
O:I Configuration Plane Wayfinder (EpiLogos/O-I#299, Gate A):

- source: EpiLogos/O-I, worktree branch of the C1 kernel lane, commit `ee34fb9f1206188a4c567d1f4c4ca00bb929a604`
- files: `schemas/oi.configuration-contribution-v1.schema.json`, `oi.config-validation-v1.schema.json`, `oi.config-plan-v1.schema.json`, `oi.config-receipt-v1.schema.json`, `oi.config-error-v1.schema.json`
- frozen by: `docs/cradle/09-CONFIGURATION-PLANE.md` (C0-1, C0-5, C0-8, C0-11)

They are read-only conformance fixtures for `factory/tests/configuration_plane.rs`.
They are NOT re-decided here: a contradiction is returned upstream as a contract
issue against #299, never solved by a local schema fork (#299 §24).
