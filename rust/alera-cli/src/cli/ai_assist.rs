use super::{OutputArgs, RuntimeDirArgs};
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct AiAssistCommand {
    #[command(flatten)]
    pub runtime: RuntimeDirArgs,
    #[command(flatten)]
    pub output: OutputArgs,
    /// Required: connect only to a running authenticated Host.
    #[arg(long = "no-start", required = true)]
    pub no_start: bool,
    #[command(subcommand)]
    pub action: AiAssistAction,
}

#[derive(Debug, Subcommand)]
pub enum AiAssistAction {
    /// Generate a draft from a preapproved staged HEAD/index snapshot.
    #[command(name = "commit-message")]
    CommitMessage(AiCommitMessageArgs),
}

#[derive(Debug, Args)]
pub struct AiCommitMessageArgs {
    #[arg(long, value_name = "workspace-path")]
    pub workspace: std::path::PathBuf,
    /// Exact HEAD object shown during the Native preview.
    #[arg(long = "expected-head")]
    pub expected_head: String,
    /// Exact index tree shown during the Native preview.
    #[arg(long = "expected-index")]
    pub expected_index: String,
}
