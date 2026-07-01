use clap::Parser;
use clip_cli::args::{command_or_default, validate, Cli, Command};
use clip_cli::run::run;

fn run_error(argv: impl IntoIterator<Item = &'static str>) -> String {
    let cli = Cli::try_parse_from(argv).unwrap();
    run(cli).unwrap_err().to_string()
}

#[test]
fn set_rejects_conflicting_input_flags() {
    let cli = Cli::try_parse_from(["clip", "set", "hello", "--input", "note.txt"]).unwrap();
    let err = validate(&cli).unwrap_err();
    assert_eq!(
        err.to_string(),
        "set accepts exactly one of positional text, stdin, or --input"
    );
}

#[test]
fn targets_all_flag_is_preserved() {
    let cli = Cli::try_parse_from(["clip", "targets", "--all"]).unwrap();
    match cli.command.unwrap() {
        Command::Targets(args) => assert!(args.all),
        _ => panic!("expected targets subcommand"),
    }
}

#[test]
fn short_flags_parse_for_get_and_set() {
    let get_cli = Cli::try_parse_from([
        "clip",
        "get",
        "-t",
        "text/plain",
        "-o",
        "out.txt",
        "-T",
        "wayland",
    ])
    .unwrap();
    match get_cli.command.unwrap() {
        Command::Get(args) => {
            assert_eq!(args.mime.as_deref(), Some("text/plain"));
            assert_eq!(args.output.unwrap().to_string_lossy(), "out.txt");
            assert_eq!(args.target, Some(clip_core::TargetKind::Wayland));
        }
        _ => panic!("expected get subcommand"),
    }

    let set_cli = Cli::try_parse_from([
        "clip",
        "set",
        "-i",
        "note.txt",
        "-t",
        "text/html",
        "-T",
        "x11",
    ])
    .unwrap();
    match set_cli.command.unwrap() {
        Command::Set(args) => {
            assert_eq!(args.input.unwrap().to_string_lossy(), "note.txt");
            assert_eq!(args.mime.as_deref(), Some("text/html"));
            assert_eq!(args.target, Some(clip_core::TargetKind::X11));
        }
        _ => panic!("expected set subcommand"),
    }
}

#[test]
fn targets_all_short_flag_is_preserved() {
    let cli = Cli::try_parse_from(["clip", "targets", "-a"]).unwrap();
    match cli.command.unwrap() {
        Command::Targets(args) => assert!(args.all),
        _ => panic!("expected targets subcommand"),
    }
}

#[test]
fn no_subcommand_parses_for_auto_mode() {
    let cli = Cli::try_parse_from(["clip"]).unwrap();
    assert!(cli.command.is_none());
}

#[test]
fn auto_mode_accepts_top_level_target() {
    let cli = Cli::try_parse_from(["clip", "--target", "wayland"]).unwrap();
    assert_eq!(cli.target, Some(clip_core::TargetKind::Wayland));
    assert!(cli.command.is_none());
}

#[test]
fn top_level_target_is_rejected_with_explicit_subcommand() {
    let cli = Cli::try_parse_from(["clip", "--target", "wayland", "get"]).unwrap();
    let err = validate(&cli).unwrap_err();
    assert_eq!(
        err.to_string(),
        "--target before a subcommand is only supported in auto mode"
    );
}

#[test]
fn no_subcommand_defaults_to_set_when_stdin_is_not_terminal() {
    let cli = Cli::try_parse_from(["clip"]).unwrap();
    match command_or_default(cli) {
        Command::Set(args) => {
            assert!(args.text.is_none());
            assert!(args.mime.is_none());
            assert!(args.input.is_none());
            assert!(args.target.is_none());
        }
        _ => panic!("expected set subcommand"),
    }
}

#[test]
fn get_rejects_unknown_target() {
    let err = Cli::try_parse_from(["clip", "get", "--target", "bogus"]).unwrap_err();
    assert!(err.to_string().contains("unknown target: bogus"));
}

#[test]
fn set_accepts_input_file() {
    let cli = Cli::try_parse_from(["clip", "set", "--input", "note.txt"]).unwrap();
    validate(&cli).unwrap();
}

#[test]
fn set_accepts_explicit_mime_type() {
    let cli = Cli::try_parse_from(["clip", "set", "hello", "--type", "text/html"]).unwrap();
    validate(&cli).unwrap();
}

#[test]
fn get_accepts_explicit_mime_type() {
    let cli = Cli::try_parse_from(["clip", "get", "--type", "text/html"]).unwrap();
    validate(&cli).unwrap();
}

#[test]
fn get_accepts_output_file() {
    let cli = Cli::try_parse_from(["clip", "get", "--output", "out.txt"]).unwrap();
    validate(&cli).unwrap();
}

#[test]
fn run_passes_supported_options_through_to_backend_resolution() {
    assert_eq!(
        run_error(["clip", "get", "--type", "text/html", "--target", "windows"]),
        "windows backend is not implemented yet"
    );
}
