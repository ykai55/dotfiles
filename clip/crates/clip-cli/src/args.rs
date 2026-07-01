use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clip_core::{ClipError, TargetKind};

#[derive(Debug, Parser)]
#[command(name = "clip")]
pub struct Cli {
    #[arg(short = 'T', long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
    #[command(subcommand)]
    pub command: Option<Command>,
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
    #[arg(short = 't', long = "type")]
    pub mime: Option<String>,
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,
    #[arg(short = 'T', long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    pub text: Option<String>,
    #[arg(short = 't', long = "type")]
    pub mime: Option<String>,
    #[arg(short = 'i', long)]
    pub input: Option<PathBuf>,
    #[arg(short = 'T', long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    #[arg(short = 'T', long, value_parser = parse_target_kind)]
    pub target: Option<TargetKind>,
}

#[derive(Debug, Args)]
pub struct TargetsArgs {
    #[arg(short = 'a', long)]
    pub all: bool,
}

pub fn validate(cli: &Cli) -> Result<(), ClipError> {
    if cli.target.is_some() && cli.command.is_some() {
        return Err(ClipError::config(
            "--target before a subcommand is only supported in auto mode",
        ));
    }

    if let Some(command) = &cli.command {
        validate_command(command)?;
    }
    Ok(())
}

pub fn validate_command(command: &Command) -> Result<(), ClipError> {
    match command {
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

pub fn command_or_default(cli: Cli) -> Command {
    match cli.command {
        Some(command) => command,
        None if std::io::stdin().is_terminal() => Command::Get(GetArgs {
            mime: None,
            output: None,
            target: cli.target,
        }),
        None => Command::Set(SetArgs {
            text: None,
            mime: None,
            input: None,
            target: cli.target,
        }),
    }
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
