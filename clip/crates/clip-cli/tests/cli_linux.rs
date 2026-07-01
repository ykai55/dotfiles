use std::fs;
use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn write_script(dir: &TempDir, name: &str, body: &str) {
    let path = dir.path().join(name);
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn command_with_tty(dir: &TempDir, name: &str, args: &[&str]) -> Command {
    let clip = assert_cmd::cargo::cargo_bin("clip");
    let clip = clip.to_str().unwrap();

    if cfg!(target_os = "macos") {
        let mut command = Command::new("script");
        command.arg("-q").arg("/dev/null").arg(&clip).args(args);
        return command;
    }

    if cfg!(target_os = "linux") {
        let wrapper = format!("{name}.sh");
        let mut body = format!("#!/usr/bin/env bash\nexec {}", shell_quote(&clip));
        for arg in args {
            body.push(' ');
            body.push_str(&shell_quote(arg));
        }
        body.push('\n');
        write_script(dir, &wrapper, &body);

        let mut command = Command::new("script");
        command
            .arg("-q")
            .arg("-e")
            .arg("-c")
            .arg(dir.path().join(wrapper))
            .arg("/dev/null");
        return command;
    }

    let mut command = Command::cargo_bin("clip").unwrap();
    command.args(args);
    command
}

#[test]
fn set_text_uses_wayland_commands() {
    let temp = TempDir::new().unwrap();
    write_script(
        &temp,
        "wl-copy",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\ncat > \"$CLIP_TEST_STDIN\"\n",
    );
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nexit 0\n");

    let args_path = temp.path().join("args.txt");
    let stdin_path = temp.path().join("stdin.txt");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    command_with_tty(&temp, "set-text", &["set", "hello", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env("CLIP_TEST_ARGS", &args_path)
        .env("CLIP_TEST_STDIN", &stdin_path)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(args_path).unwrap(),
        "--type text/plain\n"
    );
    assert_eq!(fs::read_to_string(stdin_path).unwrap(), "hello");
}

#[test]
fn set_rejects_positional_text_when_stdin_is_piped() {
    let temp = TempDir::new().unwrap();
    write_script(
        &temp,
        "wl-copy",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\ncat > \"$CLIP_TEST_STDIN\"\n",
    );
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nexit 0\n");

    let args_path = temp.path().join("args.txt");
    let stdin_path = temp.path().join("stdin.txt");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["set", "hello", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env("CLIP_TEST_ARGS", &args_path)
        .env("CLIP_TEST_STDIN", &stdin_path)
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "set accepts exactly one of positional text, stdin, or --input",
        ));
}

#[test]
fn set_rejects_input_file_when_stdin_is_piped() {
    let temp = TempDir::new().unwrap();
    write_script(
        &temp,
        "wl-copy",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\ncat > \"$CLIP_TEST_STDIN\"\n",
    );
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nexit 0\n");

    let fixture = temp.path().join("note.txt");
    fs::write(&fixture, "from file").unwrap();
    let args_path = temp.path().join("args.txt");
    let stdin_path = temp.path().join("stdin.txt");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args([
            "set",
            "--input",
            fixture.to_str().unwrap(),
            "--target",
            "wayland",
        ])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env("CLIP_TEST_ARGS", &args_path)
        .env("CLIP_TEST_STDIN", &stdin_path)
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "set accepts exactly one of positional text, stdin, or --input",
        ));
}

#[test]
fn set_accepts_empty_piped_stdin_for_text() {
    let temp = TempDir::new().unwrap();
    write_script(
        &temp,
        "wl-copy",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\ncat > \"$CLIP_TEST_STDIN\"\n",
    );
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nexit 0\n");

    let args_path = temp.path().join("args.txt");
    let stdin_path = temp.path().join("stdin.txt");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["set", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env("CLIP_TEST_ARGS", &args_path)
        .env("CLIP_TEST_STDIN", &stdin_path)
        .write_stdin("")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(args_path).unwrap(),
        "--type text/plain\n"
    );
    assert_eq!(fs::read(stdin_path).unwrap(), b"");
}

#[test]
fn no_args_with_piped_stdin_sets_text() {
    let temp = TempDir::new().unwrap();
    write_script(
        &temp,
        "wl-copy",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\ncat > \"$CLIP_TEST_STDIN\"\n",
    );
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nexit 0\n");

    let args_path = temp.path().join("args.txt");
    let stdin_path = temp.path().join("stdin.txt");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env("CLIP_TEST_ARGS", &args_path)
        .env("CLIP_TEST_STDIN", &stdin_path)
        .write_stdin("abc\n")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(args_path).unwrap(),
        "--type text/plain\n"
    );
    assert_eq!(fs::read_to_string(stdin_path).unwrap(), "abc\n");
}

#[test]
fn no_args_with_terminal_stdin_gets_text() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\nprintf 'abc\\n'\n",
    );

    let args_path = temp.path().join("args.txt");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    command_with_tty(&temp, "get-default", &["--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env("CLIP_TEST_ARGS", &args_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("abc"));

    assert_eq!(fs::read_to_string(args_path).unwrap(), "--no-newline\n");
}

#[test]
fn set_accepts_empty_piped_stdin_for_typed_bytes() {
    let temp = TempDir::new().unwrap();
    write_script(
        &temp,
        "wl-copy",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\ncat > \"$CLIP_TEST_STDIN\"\n",
    );
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nexit 0\n");

    let args_path = temp.path().join("args.txt");
    let stdin_path = temp.path().join("stdin.bin");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["set", "--type", "image/png", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env("CLIP_TEST_ARGS", &args_path)
        .env("CLIP_TEST_STDIN", &stdin_path)
        .write_stdin("")
        .assert()
        .success();

    assert_eq!(fs::read_to_string(args_path).unwrap(), "--type image/png\n");
    assert_eq!(fs::read(stdin_path).unwrap(), b"");
}

#[test]
fn types_prints_detected_mime_values() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf 'text/plain\ntext/html\n'\n",
    );
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["types", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .success()
        .stdout(predicate::str::contains("text/plain\ntext/html\n"));
}

#[test]
fn get_text_can_write_to_an_output_file() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf 'hello from backend'\n",
    );
    let output = temp.path().join("out.txt");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args([
            "get",
            "--output",
            output.to_str().unwrap(),
            "--target",
            "wayland",
        ])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .success();

    assert_eq!(fs::read_to_string(output).unwrap(), "hello from backend");
}

#[test]
fn get_missing_default_text_fails() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf 'text/plain is not available in the clipboard\n' >&2\nexit 1\n",
    );
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["get", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "text/plain is not available in the clipboard",
        ));
}

#[test]
fn get_missing_typed_text_fails() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf 'text/plain is not available in the clipboard\n' >&2\nexit 1\n",
    );
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["get", "--type", "text/plain", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "text/plain is not available in the clipboard",
        ));
}

#[test]
fn get_png_writes_binary_output() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf '\\x89PNG\\x0d\\x0a'\n",
    );
    let output = temp.path().join("out.png");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args([
            "get",
            "--type",
            "image/png",
            "--output",
            output.to_str().unwrap(),
            "--target",
            "wayland",
        ])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .success();

    assert_eq!(fs::read(output).unwrap(), b"\x89PNG\x0d\x0a");
}

#[test]
fn get_missing_binary_type_fails_without_writing_output_file() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf 'image/png is not available in the clipboard\n' >&2\nexit 1\n",
    );
    let output = temp.path().join("out.png");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args([
            "get",
            "--type",
            "image/png",
            "--output",
            output.to_str().unwrap(),
            "--target",
            "wayland",
        ])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "image/png is not available in the clipboard",
        ));

    assert!(!output.exists());
}

#[test]
fn get_typed_text_plain_prints_to_stdout_without_output_file() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf 'hello typed text'\n",
    );
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["get", "--type", "text/plain", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .success()
        .stdout("hello typed text");
}

#[test]
fn get_text_prints_unicode_to_stdout_without_escaping() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nprintf '中文'\n");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    Command::cargo_bin("clip")
        .unwrap()
        .args(["get", "--target", "wayland"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .assert()
        .success()
        .stdout("中文");
}

#[test]
fn set_html_file_uses_mime_and_raw_bytes() {
    let temp = TempDir::new().unwrap();
    write_script(
        &temp,
        "wl-copy",
        "#!/usr/bin/env bash\nprintf '%s\n' \"$*\" > \"$CLIP_TEST_ARGS\"\ncat > \"$CLIP_TEST_STDIN\"\n",
    );
    write_script(&temp, "wl-paste", "#!/usr/bin/env bash\nexit 0\n");

    let fixture = temp.path().join("snippet.html");
    fs::write(&fixture, "<b>fixture</b>").unwrap();
    let args_path = temp.path().join("args.txt");
    let stdin_path = temp.path().join("stdin.bin");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    command_with_tty(
        &temp,
        "set-html-file",
        &[
            "set",
            "--type",
            "text/html",
            "--input",
            fixture.to_str().unwrap(),
            "--target",
            "wayland",
        ],
    )
    .env("WAYLAND_DISPLAY", "wayland-0")
    .env("PATH", path)
    .env("CLIP_TEST_ARGS", &args_path)
    .env("CLIP_TEST_STDIN", &stdin_path)
    .assert()
    .success();

    assert_eq!(fs::read_to_string(args_path).unwrap(), "--type text/html\n");
    assert_eq!(fs::read(stdin_path).unwrap(), b"<b>fixture</b>");
}

#[test]
fn omitted_target_detection_matches_host_platform() {
    let temp = TempDir::new().unwrap();
    write_script(&temp, "wl-copy", "#!/usr/bin/env bash\nexit 0\n");
    write_script(
        &temp,
        "wl-paste",
        "#!/usr/bin/env bash\nprintf 'text/plain\n'\n",
    );
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );

    let assert = Command::cargo_bin("clip")
        .unwrap()
        .arg("types")
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("PATH", path)
        .env_remove("CLIP_MACOS_HELPER")
        .assert();

    if cfg!(target_os = "macos") {
        assert.success();
    } else if cfg!(target_os = "linux") {
        assert.success().stdout("text/plain\n");
    } else {
        assert
            .failure()
            .stderr(predicate::str::contains("unsupported host os"));
    }
}
