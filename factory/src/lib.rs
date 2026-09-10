//! Factory semantic core, authority and execution-intelligence boundaries.

pub mod action_projection;
pub mod agent_capability_intake;
pub mod artifact_evidence;
pub mod attempt_application;
pub mod attempt_cli;
pub mod attempt_learning;
pub mod attempt_native_store;
pub mod attempt_owner_cli;
pub mod attempt_owner_dispatch;
pub mod attempt_receiving;
pub mod attempt_runtime;
pub mod attempt_task;
pub mod authority;
pub mod build;
pub mod build_cognitive;
pub mod build_provider;
pub mod cli;
pub mod commission;
pub mod conformance;
pub mod core;
pub mod development_field;
pub mod development_field_cli;
pub mod developmental_read;
pub mod execution_intelligence;
pub mod git_development;
// JourneyRef implements the standard AsRef trait below. Keep the legacy inherent
// accessor during this additive contract tranche without weakening any other lint.
#[allow(clippy::should_implement_trait)]
pub mod journey;
pub mod journey_build;
pub mod journey_commission;
pub mod journey_praxis;
pub mod native_owner;
pub mod native_process;
pub mod orchestration;
pub mod project_development;
pub mod project_development_store;
pub mod routine_continuation;
pub mod structural_ground;
pub mod workflow;

impl AsRef<core::identity::Ref> for journey::JourneyRef {
    fn as_ref(&self) -> &core::identity::Ref {
        journey::JourneyRef::as_ref(self)
    }
}
