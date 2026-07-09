use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use clip_core::{
    ClipboardBackend, ClipboardItem, ClipboardVariant, MimeType, ReadRequest, TargetKind,
};
use clip_platform::{
    resolve_backend, resolve_macos_helper_path_for, CommandOutput, CommandRunner, CommandSpec,
    EnvProbe, MacOsBackend,
};

#[derive(Default)]
struct FakeRunner {
    calls: Mutex<Vec<CommandSpec>>,
    outputs: Mutex<VecDeque<CommandOutput>>,
}

impl FakeRunner {
    fn with_output(output: CommandOutput) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            outputs: Mutex::new(VecDeque::from([output])),
        })
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, spec: CommandSpec) -> Result<CommandOutput, std::io::Error> {
        self.calls.lock().unwrap().push(spec);
        Ok(self.outputs.lock().unwrap().pop_front().unwrap())
    }
}

struct FakeEnv {
    os: &'static str,
    vars: HashMap<String, String>,
}

impl FakeEnv {
    fn macos() -> Self {
        Self {
            os: "macos",
            vars: HashMap::new(),
        }
    }
}

impl EnvProbe for FakeEnv {
    fn os(&self) -> &'static str {
        self.os
    }

    fn var(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }

    fn command_exists(&self, _command: &str) -> bool {
        false
    }
}

#[test]
fn write_html_invokes_helper_with_explicit_type() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    });
    let backend = MacOsBackend::new(runner.clone(), PathBuf::from("/tmp/clip-macos-helper"));

    backend
        .write(&ClipboardItem::bytes(
            MimeType::new("text/html").unwrap(),
            b"<strong>hi</strong>".to_vec(),
        ))
        .unwrap();

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "/tmp/clip-macos-helper");
    assert_eq!(calls[0].args, vec!["write", "--type", "text/html"]);
}

#[test]
fn write_bundle_invokes_helper_with_encoded_variants() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    });
    let backend = MacOsBackend::new(runner.clone(), PathBuf::from("/tmp/clip-macos-helper"));

    backend
        .write(&ClipboardItem::bundle(vec![
            ClipboardVariant {
                mime: MimeType::new("text/html").unwrap(),
                data: b"<strong>hi</strong>".to_vec(),
            },
            ClipboardVariant {
                mime: MimeType::new("text/plain").unwrap(),
                data: b"hi".to_vec(),
            },
        ]))
        .unwrap();

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "/tmp/clip-macos-helper");
    assert_eq!(calls[0].args, vec!["write-bundle"]);
    assert_eq!(
        calls[0].stdin,
        b"clip-bundle-v1\ntext/html\n19\n<strong>hi</strong>\ntext/plain\n2\nhi\n"
    );
}

#[test]
fn list_types_maps_helper_output_to_mime_values() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: b"public.utf8-plain-text\npublic.html\npublic.png\npublic.file-url\npublic.url\npublic.tiff\npublic.rtf\n".to_vec(),
        stderr: Vec::new(),
    });
    let backend = MacOsBackend::new(runner, PathBuf::from("/tmp/clip-macos-helper"));

    let values = backend.list_types().unwrap();
    assert_eq!(
        values.iter().map(|item| item.as_str()).collect::<Vec<_>>(),
        vec!["text/plain", "text/html", "image/png", "text/uri-list"]
    );
}

#[test]
fn read_text_preserves_unicode_from_helper() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: "中文".as_bytes().to_vec(),
        stderr: Vec::new(),
    });
    let backend = MacOsBackend::new(runner, PathBuf::from("/tmp/clip-macos-helper"));

    assert_eq!(
        backend.read(ReadRequest::text()).unwrap(),
        clip_core::ClipboardBlob::Text(String::from("中文"))
    );
}

#[test]
fn resolve_helper_path_finds_project_relative_release_binary() {
    let root = unique_temp_dir();
    let helper = root.join("helpers/macos/clip-macos-helper/.build/release/clip-macos-helper");
    fs::create_dir_all(helper.parent().unwrap()).unwrap();
    fs::write(&helper, b"#!/bin/sh\n").unwrap();

    let resolved = resolve_macos_helper_path_for(&root, None).unwrap();
    assert_eq!(resolved, helper);

    fs::remove_file(&helper).unwrap();
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn resolve_backend_uses_env_override_before_default_search() {
    let backend = resolve_backend(
        &FakeEnv::macos(),
        Arc::new(FakeRunner::default()),
        Some(TargetKind::MacOS),
        Some(PathBuf::from("/tmp/override-helper")),
    )
    .unwrap();

    assert_eq!(backend.name(), "macos");
}

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("clip-platform-macos-test-{nanos}"))
}

#[test]
fn capabilities_do_not_advertise_custom_mime_support() {
    let backend = MacOsBackend::new(
        Arc::new(FakeRunner::default()),
        PathBuf::from("/tmp/clip-macos-helper"),
    );

    assert!(!backend.capabilities().supports_custom_mime);
}

#[test]
fn typed_read_rejects_custom_mime_without_invoking_helper() {
    let runner = Arc::new(FakeRunner::default());
    let backend = MacOsBackend::new(runner.clone(), PathBuf::from("/tmp/clip-macos-helper"));

    let error = backend
        .read(ReadRequest::typed(
            MimeType::new("application/json").unwrap(),
        ))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "macos backend only supports text/plain, text/html, image/png, and text/uri-list"
    );
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[test]
fn write_rejects_custom_mime_without_invoking_helper() {
    let runner = Arc::new(FakeRunner::default());
    let backend = MacOsBackend::new(runner.clone(), PathBuf::from("/tmp/clip-macos-helper"));

    let error = backend
        .write(&ClipboardItem::bytes(
            MimeType::new("application/json").unwrap(),
            br#"{"ok":true}"#.to_vec(),
        ))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "macos backend only supports text/plain, text/html, image/png, and text/uri-list"
    );
    assert!(runner.calls.lock().unwrap().is_empty());
}
