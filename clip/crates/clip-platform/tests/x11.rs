use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use clip_core::{ClipboardBackend, ClipboardBlob, ClipboardItem, MimeType, ReadRequest};
use clip_platform::{CommandOutput, CommandRunner, CommandSpec, X11Backend, X11Tool};

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
fn xclip_write_uses_type_flag_for_custom_mime() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    });
    let backend = X11Backend::new(X11Tool::Xclip, runner.clone());

    backend
        .write(&ClipboardItem::bytes(
            MimeType::new("text/html").unwrap(),
            b"<p>ok</p>".to_vec(),
        ))
        .unwrap();

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "xclip");
    assert_eq!(
        calls[0].args,
        vec!["-selection", "clipboard", "-t", "text/html", "-i"]
    );
}

#[test]
fn xsel_rejects_binary_writes() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    });
    let backend = X11Backend::new(X11Tool::Xsel, runner);

    let err = backend
        .write(&ClipboardItem::bytes(
            MimeType::new("image/png").unwrap(),
            vec![0, 1, 2],
        ))
        .unwrap_err();

    assert_eq!(err.to_string(), "xsel backend only supports text/plain");
}

#[test]
fn xsel_typed_text_plain_write_uses_plain_text_path() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    });
    let backend = X11Backend::new(X11Tool::Xsel, runner.clone());

    backend
        .write(&ClipboardItem::bytes(
            MimeType::new("text/plain").unwrap(),
            b"hello".to_vec(),
        ))
        .unwrap();

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "xsel");
    assert_eq!(calls[0].args, vec!["--clipboard", "--input"]);
    assert_eq!(calls[0].stdin, b"hello".to_vec());
}

#[test]
fn xsel_typed_text_plain_read_uses_plain_text_path() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: b"hello".to_vec(),
        stderr: Vec::new(),
    });
    let backend = X11Backend::new(X11Tool::Xsel, runner.clone());

    assert_eq!(
        backend
            .read(ReadRequest::typed(MimeType::new("text/plain").unwrap()))
            .unwrap(),
        ClipboardBlob::Text(String::from("hello"))
    );

    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[0].program, "xsel");
    assert_eq!(calls[0].args, vec!["--clipboard", "--output"]);
}

#[test]
fn xsel_list_types_reports_unsupported() {
    let runner = FakeRunner::default();
    let backend = X11Backend::new(X11Tool::Xsel, Arc::new(runner));

    let err = backend.list_types().unwrap_err();

    assert_eq!(
        err.to_string(),
        "xsel backend does not support listing clipboard MIME types"
    );
    assert!(!backend.capabilities().supports_type_listing);
}

#[test]
fn xclip_list_types_deduplicates_text_plain_targets() {
    let runner = FakeRunner::with_output(CommandOutput {
        status: 0,
        stdout: b"UTF8_STRING\nSTRING\ntext/html\n".to_vec(),
        stderr: Vec::new(),
    });
    let backend = X11Backend::new(X11Tool::Xclip, runner);

    let types = backend.list_types().unwrap();

    assert_eq!(
        types.iter().map(|item| item.as_str()).collect::<Vec<_>>(),
        vec!["text/plain", "text/html"]
    );
}
