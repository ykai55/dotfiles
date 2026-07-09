use std::fs;

use clip_core::{ClipboardItem, ClipboardVariant, MimeType};
use clip_history::store::{
    entry_item, entry_preview, find_entry, list_entries, migrate_entries, save_snapshot,
    ClipboardSnapshot, SnapshotVariant,
};
use tempfile::TempDir;

fn mime(value: &str) -> MimeType {
    MimeType::new(value).unwrap()
}

fn write_old_entry(store_dir: &std::path::Path, created_at_ms: u128, hash: &str, text: &str) {
    let entry_dir = store_dir
        .join("entries")
        .join(format!("{created_at_ms}-{hash}"));
    fs::create_dir_all(&entry_dir).unwrap();
    fs::write(entry_dir.join("00-text_plain"), text.as_bytes()).unwrap();
    fs::write(
        entry_dir.join("meta.txt"),
        format!(
            "id={created_at_ms}-{hash}\ncreated_at_ms={created_at_ms}\nhash={hash}\nprimary_mime=text/plain\nvariant=text/plain|00-text_plain|{}\n",
            text.len()
        ),
    )
    .unwrap();
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
    assert_eq!(entries[0].id, entries[0].hash);
    assert_eq!(
        entries[0].path,
        temp.path().join("entries").join(&entries[0].hash)
    );
    assert_eq!(entries[0].primary_mime.as_str(), "image/png");
    assert_eq!(entries[0].variant_count, 3);
    assert!(entries[0].preview.is_some());
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
fn separated_duplicate_refreshes_timestamp_and_copy_count() {
    let temp = TempDir::new().unwrap();
    let first = ClipboardSnapshot {
        variants: vec![SnapshotVariant {
            mime: mime("text/plain"),
            data: b"first".to_vec(),
        }],
    };
    let second = ClipboardSnapshot {
        variants: vec![SnapshotVariant {
            mime: mime("text/plain"),
            data: b"second".to_vec(),
        }],
    };

    assert!(save_snapshot(temp.path(), &first).unwrap());
    let original_first = list_entries(temp.path()).unwrap().remove(0);
    assert_eq!(original_first.copy_count, 1);
    assert!(save_snapshot(temp.path(), &second).unwrap());
    assert!(save_snapshot(temp.path(), &first).unwrap());

    let entries = list_entries(temp.path()).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].hash, original_first.hash);
    assert_eq!(entries[0].id, original_first.id);
    assert_eq!(entries[0].path, original_first.path);
    assert!(entries[0].created_at_ms > original_first.created_at_ms);
    assert_eq!(entries[0].copy_count, 2);
    assert!(original_first.path.exists());
    assert_eq!(
        entry_item(&entries[0], Some("text/plain")).unwrap(),
        ClipboardItem::Text(String::from("first"))
    );
}

#[test]
fn migrate_entries_merges_duplicate_hashes_and_adds_copy_count() {
    let temp = TempDir::new().unwrap();
    write_old_entry(temp.path(), 1000, "same", "old");
    write_old_entry(temp.path(), 2000, "other", "middle");
    write_old_entry(temp.path(), 3000, "same", "new");

    let result = migrate_entries(temp.path()).unwrap();
    assert_eq!(result.scanned, 3);
    assert_eq!(result.rewritten, 2);
    assert_eq!(result.merged, 1);

    let entries = list_entries(temp.path()).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].hash, "same");
    assert_eq!(entries[0].id, "same");
    assert_eq!(entries[0].path, temp.path().join("entries").join("same"));
    assert_eq!(entries[0].created_at_ms, 3000);
    assert_eq!(entries[0].copy_count, 2);
    assert_eq!(entry_preview(&entries[0]).unwrap(), "new");
    assert!(!temp.path().join("entries").join("1000-same").exists());
    assert!(!temp.path().join("entries").join("3000-same").exists());
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
fn entry_item_selects_all_variants_by_default() {
    let temp = TempDir::new().unwrap();
    let snapshot = ClipboardSnapshot {
        variants: vec![
            SnapshotVariant {
                mime: mime("text/html"),
                data: b"<b>rich</b>".to_vec(),
            },
            SnapshotVariant {
                mime: mime("text/plain"),
                data: b"rich".to_vec(),
            },
        ],
    };
    save_snapshot(temp.path(), &snapshot).unwrap();
    let entry = list_entries(temp.path()).unwrap().remove(0);

    assert_eq!(
        entry_item(&entry, None).unwrap(),
        ClipboardItem::bundle(vec![
            ClipboardVariant {
                mime: mime("text/html"),
                data: b"<b>rich</b>".to_vec(),
            },
            ClipboardVariant {
                mime: mime("text/plain"),
                data: b"rich".to_vec(),
            },
        ])
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
