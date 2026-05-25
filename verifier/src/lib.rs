mod sanitize;
mod verdict;
mod pipeline;
mod agent;
mod harden;
mod manifest;
mod stage1;
mod haiku_agent;

pub use sanitize::sanitize_content;
pub use verdict::{VerificationVerdict, AdmitDecision, compute_admit_decision};
pub use pipeline::VerificationPipeline;
pub use agent::{VerificationAgent, AgentAssessment, build_agent_prompt, parse_agent_response};
pub use harden::{harden_label, harden_description, harden_tool_description, HardenError};
pub use manifest::{
    Manifest, ToolEntry, ManifestError,
    validate_endpoint, parse_and_validate_manifest,
    verify_manifest_hash, fetch_manifest,
};
pub use stage1::{run_stage1, Stage1Facts, Stage1Outcome, Stage1Error};
pub use haiku_agent::HaikuAgent;
