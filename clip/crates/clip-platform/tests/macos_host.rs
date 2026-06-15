#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::Arc;

use clip_core::{ClipboardBackend, ClipboardBlob, ClipboardItem, MimeType, ReadRequest};
use clip_platform::{resolve_macos_helper_path, MacOsBackend, ProcessCommandRunner};

#[test]
fn helper_round_trips_text_and_html() {
    let Ok(helper) =
        resolve_macos_helper_path(std::env::var_os("CLIP_MACOS_HELPER").map(Into::into))
    else {
        eprintln!("skipping macOS host test because no built helper could be found");
        return;
    };
    let backend = MacOsBackend::new(Arc::new(ProcessCommandRunner), PathBuf::from(helper));

    backend.write(&ClipboardItem::text("macos text")).unwrap();
    assert_eq!(
        backend.read(ReadRequest::text()).unwrap(),
        ClipboardBlob::Text(String::from("macos text"))
    );

    backend
        .write(&ClipboardItem::Bytes {
            mime: MimeType::new("text/html").unwrap(),
            data: b"<b>macos</b>".to_vec(),
        })
        .unwrap();

    assert_eq!(
        backend.read(ReadRequest::text()).unwrap(),
        ClipboardBlob::Text(String::from("<b>macos</b>"))
    );

    assert!(backend
        .list_types()
        .unwrap()
        .iter()
        .any(|mime| mime.as_str() == "text/html"));

    backend
        .write(&ClipboardItem::Bytes {
            mime: MimeType::new("text/uri-list").unwrap(),
            data: b"https://example.com\nhttps://example.org".to_vec(),
        })
        .unwrap();

    assert_eq!(
        backend.read(ReadRequest::text()).unwrap(),
        ClipboardBlob::Text(String::from("https://example.com\nhttps://example.org"))
    );
}
