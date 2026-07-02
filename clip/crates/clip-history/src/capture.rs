use std::collections::HashSet;

use clip_core::{ClipError, ClipboardBackend, ClipboardBlob, MimeType, ReadRequest};

use crate::store::{ClipboardSnapshot, SnapshotVariant};

pub fn capture_snapshot(
    backend: &dyn ClipboardBackend,
    max_bytes: usize,
) -> Result<Option<ClipboardSnapshot>, ClipError> {
    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    let mut types = backend.list_types().unwrap_or_default();
    if types.is_empty() {
        if let Ok(blob) = backend.read(ReadRequest::text()) {
            push_blob(&mut variants, &mut seen, blob, max_bytes);
        }
    } else {
        sort_types(&mut types);
        for mime in types {
            if let Ok(blob) = backend.read(ReadRequest::typed(mime)) {
                push_blob(&mut variants, &mut seen, blob, max_bytes);
            }
        }
    }

    if variants.is_empty() {
        return Ok(None);
    }

    Ok(Some(ClipboardSnapshot { variants }))
}

fn push_blob(
    variants: &mut Vec<SnapshotVariant>,
    seen: &mut HashSet<String>,
    blob: ClipboardBlob,
    max_bytes: usize,
) {
    let (mime, data) = match blob {
        ClipboardBlob::Text(text) => (MimeType::new("text/plain").unwrap(), text.into_bytes()),
        ClipboardBlob::Bytes { mime, data } => (mime, data),
    };

    if data.len() > max_bytes || !seen.insert(mime.as_str().to_string()) {
        return;
    }

    variants.push(SnapshotVariant { mime, data });
}

fn sort_types(types: &mut [MimeType]) {
    types.sort_by(|left, right| mime_priority(left.as_str()).cmp(&mime_priority(right.as_str())));
}

fn mime_priority(mime: &str) -> (usize, &str) {
    let priority = match mime {
        "image/png" => 0,
        "text/html" => 1,
        "text/uri-list" => 2,
        "text/plain" => 3,
        _ if mime.starts_with("image/") => 4,
        _ if mime.starts_with("text/") => 5,
        _ => 6,
    };
    (priority, mime)
}
