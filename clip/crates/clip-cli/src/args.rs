use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clip_core::{ClipError, TargetKind};

#[derive(Debug, Parser)]
#[command(name = "clip")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Get(GetArgs),
    Set(SetArgs),
    Types(TargetArgs),
    Targets(TargetsArgs),
}

#[derive(Debug, Args)]
pub struct GetArgs {
    #[arg(long = "type")]
    pub mime: Option<String>,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    pub text: Option<String>,
    #[arg(long = "type")]
    pub mime: Option<String>,
    #[arg(long)]
    pub input: Option<PathBuf>,
    #[arg(long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    #[arg(long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
}

#[derive(Debug, Args)]
pub struct TargetsArgs {
    #[arg(long)]
    pub all: bool,
}

pub fn validate(cli: &Cli) -> Result<(), ClipError> {
    match &cli.command {
        Command::Get(_) => {}
        Command::Set(args) => {
            let mut explicit_sources = 0;
            if args.text.is_some() {
                explicit_sources += 1;
            }
            if args.input.is_some() {
                explicit_sources += 1;
            }
            if explicit_sources > 1 {
                return Err(ClipError::config(
                    "set accepts exactly one of positional text, stdin, or --input",
                ));
            }
        }
        Command::Types(_) => {}
        Command::Targets(_) => {}
    }
    Ok(())
}

fn parse_target_kind(value: &str) -> Result<TargetKind, String> {
    parse_target(value).map_err(|err| err.to_string())
}

pub fn parse_target(value: &str) -> Result<TargetKind, ClipError> {
    match value {
        "macos" => Ok(TargetKind::MacOS),
        "wayland" => Ok(TargetKind::Wayland),
        "x11" => Ok(TargetKind::X11),
        "windows" => Ok(TargetKind::Windows),
        "adb" => Ok(TargetKind::Adb),
        other => Err(ClipError::config(format!("unknown target: {other}"))),
    }
}
