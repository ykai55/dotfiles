use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use clip_core::{ClipboardBackend, ClipboardBlob, ClipboardItem, MimeType, ReadRequest};
use clip_platform::{CommandOutput, CommandRunner, CommandSpec, WaylandBackend};

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

#[test]
fn list_types_splits_wayland_output_lines() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: b"text/plain\ntext/html\n".to_vec(),
        stderr: Vec::new(),
    });
    let backend = WaylandBackend::new(runner);

    let types = backend.list_types().unwrap();
    assert_eq!(
        types.iter().map(|item| item.as_str()).collect::<Vec<_>>(),
        vec!["text/plain", "text/html"]
    );
}

#[test]
fn list_types_ignores_non_mime_wayland_targets() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: b"text/plain;charset=utf-8\nUTF8_STRING\nCOMPOUND_TEXT\nTEXT\ntext/plain\nSTRING\ntext/plain;charset=utf-8\ntext/plain\nSAVE_TARGETS\n".to_vec(),
        stderr: Vec::new(),
    });
    let backend = WaylandBackend::new(runner);

    let types = backend.list_types().unwrap();
    assert_eq!(
        types.iter().map(|item| item.as_str()).collect::<Vec<_>>(),
        vec!["text/plain;charset=utf-8", "text/plain"]
    );
}

#[test]
fn write_bytes_uses_wl_copy_with_explicit_type() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    });
    let backend = WaylandBackend::new(runner.clone());

    backend
        .write(&ClipboardItem::bytes(
            MimeType::new("text/html").unwrap(),
            b"<b>hi</b>".to_vec(),
        ))
        .unwrap();

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "wl-copy");
    assert_eq!(calls[0].args, vec!["--type", "text/html"]);
    assert_eq!(calls[0].stdin, b"<b>hi</b>".to_vec());
}

#[test]
fn read_text_returns_text_blob() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: b"hello".to_vec(),
        stderr: Vec::new(),
    });
    let backend = WaylandBackend::new(runner.clone());

    assert_eq!(
        backend.read(ReadRequest::text()).unwrap(),
        ClipboardBlob::Text(String::from("hello"))
    );

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "wl-paste");
    assert_eq!(calls[0].args, vec!["--no-newline"]);
}

#[test]
fn read_text_preserves_unicode() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: "中文".as_bytes().to_vec(),
        stderr: Vec::new(),
    });
    let backend = WaylandBackend::new(runner);

    assert_eq!(
        backend.read(ReadRequest::text()).unwrap(),
        ClipboardBlob::Text(String::from("中文"))
    );
}

#[test]
fn typed_read_uses_no_newline_and_preserves_bytes() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: b"<p>hi</p>".to_vec(),
        stderr: Vec::new(),
    });
    let backend = WaylandBackend::new(runner.clone());
    let mime = MimeType::new("text/html").unwrap();

    assert_eq!(
        backend.read(ReadRequest::typed(mime.clone())).unwrap(),
        ClipboardBlob::Bytes {
            mime: mime.clone(),
            data: b"<p>hi</p>".to_vec(),
        }
    );

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "wl-paste");
    assert_eq!(calls[0].args, vec!["--type", "text/html", "--no-newline"]);
}
