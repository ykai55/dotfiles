use clip_core::{ClipboardItem, MimeType, ReadRequest};

#[test]
fn mime_type_accepts_builtin_and_custom_values() {
    assert_eq!(MimeType::new("text/plain").unwrap().as_str(), "text/plain");
    assert_eq!(
        MimeType::new("application/x.clip-custom").unwrap().as_str(),
        "application/x.clip-custom"
    );
    assert!(MimeType::new("not-a-mime").is_err());
    assert!(MimeType::new("text/").is_err());
    assert!(MimeType::new("/plain").is_err());
    assert!(MimeType::new("text//plain").is_err());
    assert!(MimeType::new("text/plain/extra").is_err());
}

#[test]
fn read_request_text_mode_defaults_to_prefer_text() {
    let request = ReadRequest::text();
    assert!(request.prefer_text());
    assert!(request.mime().is_none());
}

#[test]
fn clipboard_item_text_helper_preserves_value() {
    assert_eq!(
        ClipboardItem::text("hello"),
        ClipboardItem::Text(String::from("hello"))
    );
}
