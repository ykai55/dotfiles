use clap::Parser;
use clip_cli::args::{validate, Cli, Command};
use clip_cli::run::run;

fn run_error(argv: impl IntoIterator<Item = &'static str>) -> String {
    let cli = Cli::try_parse_from(argv).unwrap();
    run(cli).unwrap_err().to_string()
}

#[test]
fn set_rejects_conflicting_input_flags() {
    let cli = Cli::try_parse_from(["clip", "set", "hello", "--input", "note.txt"]).unwrap();
    let err = validate(&cli).unwrap_err();
    assert_eq!(err.to_string(), "set accepts exactly one of positional text, stdin, or --input");
}

#[test]
fn targets_all_flag_is_preserved() {
    let cli = Cli::try_parse_from(["clip", "targets", "--all"]).unwrap();
    match cli.command {
        Command::Targets(args) => assert!(args.all),
        _ => panic!("expected targets subcommand"),
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
