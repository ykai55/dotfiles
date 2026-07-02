use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clip_core::{ClipError, TargetKind};

#[derive(Debug, Parser)]
#[command(name = "clip-history")]
pub struct Cli {
    #[arg(long)]
    pub store: Option<PathBuf>,
    #[arg(short = 'T', long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Capture the current clipboard once.
    Poll(PollArgs),
    /// Poll the clipboard forever and save changes.
    Daemon(DaemonArgs),
    /// List recent clipboard history entries.
    List(ListArgs),
    /// Search history previews, MIME types, and ids.
    Search(SearchArgs),
    /// Select a history entry as the current clipboard.
    Select(SelectArgs),
    /// Delete one history entry by id prefix.
    Delete(IdArgs),
    /// Delete all history entries.
    Clear,
}

#[derive(Debug, Args)]
pub struct PollArgs {
    #[arg(long, default_value_t = 8 * 1024 * 1024)]
    pub max_bytes: usize,
}

#[derive(Debug, Args)]
pub struct DaemonArgs {
    #[arg(long, default_value_t = 700)]
    pub interval_ms: u64,
    #[arg(long, default_value_t = 8 * 1024 * 1024)]
    pub max_bytes: usize,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(short = 'n', long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    pub query: String,
    #[arg(short = 'n', long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct SelectArgs {
    pub id: String,
    #[arg(short = 't', long = "type")]
    pub mime: Option<String>,
}

#[derive(Debug, Args)]
pub struct IdArgs {
    pub id: String,
}

fn parse_target_kind(value: &str) -> Result<TargetKind, String> {
    match value {
        "macos" => Ok(TargetKind::MacOS),
        "wayland" => Ok(TargetKind::Wayland),
        "x11" => Ok(TargetKind::X11),
        "windows" => Ok(TargetKind::Windows),
        "adb" => Ok(TargetKind::Adb),
        other => Err(ClipError::config(format!("unknown target: {other}")).to_string()),
    }
}
