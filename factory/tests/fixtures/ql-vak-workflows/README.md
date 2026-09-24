Byte-for-byte copies of the domain-authored workflows in EpiLogos/QL-MEF
`workflows/` (expression development, Technē constellation, development and
knowledge). QL CI does not build Factory, so Factory checks them here:
`factory/tests/workflow_domain_adapter.rs` compiles and lowers them, and
`scripts/test_workflow_sdk.py` typechecks them with `tsc` against the vendored
`@epilogos/ql-vak` declarations. When QL changes a workflow, copy it here.
