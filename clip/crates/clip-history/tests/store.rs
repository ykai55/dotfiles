use clip_core::{ClipboardItem, MimeType};
use clip_history::store::{
    entry_item, entry_preview, find_entry, list_entries, save_snapshot, ClipboardSnapshot,
    SnapshotVariant,
};
use tempfile::TempDir;

fn mime(value: &str) -> MimeType {
    MimeType::new(value).unwrap()
}

#[test]
fn save_snapshot_preserves_multiple_mime_variants() {
    let temp = TempDir::new().unwrap();
    let snapshot = ClipboardSnapshot {
        variants: vec![
            SnapshotVariant {
                mime: mime("text/plain"),
                data: b"hello".to_vec(),
            },
            SnapshotVariant {
                mime: mime("text/html"),
                data: b"<b>hello</b>".to_vec(),
            },
            SnapshotVariant {
                mime: mime("image/png"),
                data: vec![137, 80, 78, 71],
            },
        ],
    };

    assert!(save_snapshot(temp.path(), &snapshot).unwrap());
    let entries = list_entries(temp.path()).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].primary_mime.as_str(), "image/png");
    assert_eq!(entries[0].variants.len(), 3);
}

#[test]
fn repeated_snapshot_is_not_saved_twice() {
    let temp = TempDir::new().unwrap();
    let snapshot = ClipboardSnapshot {
        variants: vec![SnapshotVariant {
            mime: mime("text/plain"),
            data: b"same".to_vec(),
        }],
    };

    assert!(save_snapshot(temp.path(), &snapshot).unwrap());
    assert!(!save_snapshot(temp.path(), &snapshot).unwrap());
    assert_eq!(list_entries(temp.path()).unwrap().len(), 1);
}

#[test]
fn entry_item_can_select_specific_mime_variant() {
    let temp = TempDir::new().unwrap();
    let snapshot = ClipboardSnapshot {
        variants: vec![
            SnapshotVariant {
                mime: mime("text/plain"),
                data: b"plain".to_vec(),
            },
            SnapshotVariant {
                mime: mime("image/png"),
                data: vec![1, 2, 3],
            },
        ],
    };
    save_snapshot(temp.path(), &snapshot).unwrap();
    let entry = list_entries(temp.path()).unwrap().remove(0);

    assert_eq!(
        entry_item(&entry, Some("text/plain")).unwrap(),
        ClipboardItem::Text(String::from("plain"))
    );
    assert_eq!(
        entry_item(&entry, Some("image/png")).unwrap(),
        ClipboardItem::bytes(mime("image/png"), vec![1, 2, 3])
    );
}

#[test]
fn entry_item_preserves_non_plain_text_mime() {
    let temp = TempDir::new().unwrap();
    let snapshot = ClipboardSnapshot {
        variants: vec![SnapshotVariant {
            mime: mime("text/html"),
            data: b"<b>rich</b>".to_vec(),
        }],
    };
    save_snapshot(temp.path(), &snapshot).unwrap();
    let entry = list_entries(temp.path()).unwrap().remove(0);

    assert_eq!(
        entry_item(&entry, None).unwrap(),
        ClipboardItem::bytes(mime("text/html"), b"<b>rich</b>".to_vec())
    );
}

#[test]
fn find_entry_accepts_prefix_and_preview_uses_text() {
    let temp = TempDir::new().unwrap();
    let snapshot = ClipboardSnapshot {
        variants: vec![SnapshotVariant {
            mime: mime("text/plain"),
            data: b"hello\nfrom history".to_vec(),
        }],
    };
    save_snapshot(temp.path(), &snapshot).unwrap();
    let entry = list_entries(temp.path()).unwrap().remove(0);
    let prefix = &entry.id[..12];

    let found = find_entry(temp.path(), prefix).unwrap();
    assert_eq!(found.id, entry.id);
    assert_eq!(entry_preview(&found).unwrap(), "hello from history");
}
