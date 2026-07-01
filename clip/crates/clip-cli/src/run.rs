use std::sync::Arc;

use clap::Parser;
use clip_core::{ClipError, MimeType, ReadRequest};
use clip_platform::{available_targets, resolve_backend, ProcessCommandRunner, RealEnvProbe};

use crate::args::{command_or_default, validate_command, Cli, Command};
use crate::input::load_item;
use crate::output::write_output;

pub fn run(cli: Cli) -> Result<(), ClipError> {
    crate::args::validate(&cli)?;
    let command = command_or_default(cli);
    validate_command(&command)?;

    let probe = RealEnvProbe;
    let runner = Arc::new(ProcessCommandRunner);
    let macos_helper = std::env::var_os("CLIP_MACOS_HELPER").map(Into::into);

    match command {
        Command::Set(args) => {
            let backend = resolve_backend(&probe, runner, args.target, macos_helper)?;
            let item = load_item(&args)?;
            backend.write(&item)
        }
        Command::Get(args) => {
            let backend = resolve_backend(&probe, runner, args.target, macos_helper)?;
            let request = match args.mime.as_deref() {
                Some(mime) => ReadRequest::typed(MimeType::new(mime)?),
                None => ReadRequest::text(),
            };
            let blob = backend.read(request)?;
            write_output(blob, args.output.as_deref())
        }
        Command::Types(args) => {
            let backend = resolve_backend(&probe, runner, args.target, macos_helper)?;
            for mime in backend.list_types()? {
                println!("{}", mime.as_str());
            }
            Ok(())
        }
        Command::Targets(args) => {
            for target in available_targets(&probe, args.all) {
                println!("{}", target.as_str());
            }
            Ok(())
        }
    }
}

pub fn main_entry() -> i32 {
    let cli = Cli::parse();
    if let Err(err) = crate::args::validate(&cli) {
        eprintln!("{err}");
        return 2;
    }

    match run(cli) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}
