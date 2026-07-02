use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clap::Parser;
use clip_core::{ClipError, TargetKind};
use clip_platform::{resolve_backend, ProcessCommandRunner, RealEnvProbe};

use crate::args::{Cli, Command};
use crate::capture::capture_snapshot;
use crate::store::{
    clear_entries, default_store_dir, delete_entry, entry_item, entry_preview, find_entry,
    list_entries, save_snapshot,
};

pub fn run(cli: Cli) -> Result<(), ClipError> {
    let store_dir = cli.store.unwrap_or_else(default_store_dir);
    match cli.command {
        Command::Poll(args) => {
            let saved = poll_once(&store_dir, cli.target, args.max_bytes)?;
            println!("{}", if saved { "saved" } else { "unchanged" });
            Ok(())
        }
        Command::Daemon(args) => loop {
            if let Err(err) = poll_once(&store_dir, cli.target, args.max_bytes) {
                eprintln!("{err}");
            }
            thread::sleep(Duration::from_millis(args.interval_ms));
        },
        Command::List(args) => {
            print_entries(&store_dir, None, args.limit)?;
            Ok(())
        }
        Command::Search(args) => {
            print_entries(&store_dir, Some(&args.query), args.limit)?;
            Ok(())
        }
        Command::Select(args) => {
            let entry = find_entry(&store_dir, &args.id)?;
            let item = entry_item(&entry, args.mime.as_deref())?;
            let backend = backend(cli.target)?;
            backend.write(&item)
        }
        Command::Delete(args) => delete_entry(&store_dir, &args.id),
        Command::Clear => clear_entries(&store_dir),
    }
}

pub fn main_entry() -> i32 {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn poll_once(
    store_dir: &PathBuf,
    target: Option<TargetKind>,
    max_bytes: usize,
) -> Result<bool, ClipError> {
    let backend = backend(target)?;
    let Some(snapshot) = capture_snapshot(backend.as_ref(), max_bytes)? else {
        return Ok(false);
    };
    save_snapshot(store_dir, &snapshot)
}

fn backend(target: Option<TargetKind>) -> Result<Box<dyn clip_core::ClipboardBackend>, ClipError> {
    let probe = RealEnvProbe;
    let runner = Arc::new(ProcessCommandRunner);
    let macos_helper = std::env::var_os("CLIP_MACOS_HELPER").map(Into::into);
    resolve_backend(&probe, runner, target, macos_helper)
}

fn print_entries(store_dir: &PathBuf, query: Option<&str>, limit: usize) -> Result<(), ClipError> {
    let query = query.map(str::to_lowercase);
    let mut printed = 0;

    for entry in list_entries(store_dir)? {
        let preview = entry_preview(&entry)?;
        if let Some(query) = &query {
            let haystack =
                format!("{} {} {}", entry.id, entry.primary_mime.as_str(), preview).to_lowercase();
            if !haystack.contains(query) {
                continue;
            }
        }

        println!(
            "{}\t{}\t{}\t{}",
            short_id(&entry.id),
            entry.primary_mime.as_str(),
            entry.variant_count,
            preview
        );
        printed += 1;
        if printed >= limit {
            break;
        }
    }
    Ok(())
}

fn short_id(id: &str) -> &str {
    id.get(..24).unwrap_or(id)
}
