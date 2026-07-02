use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clip_core::{ClipError, ClipboardItem, MimeType};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardSnapshot {
    pub variants: Vec<SnapshotVariant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotVariant {
    pub mime: MimeType,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    pub id: String,
    pub created_at_ms: u128,
    pub hash: String,
    pub copy_count: u64,
    pub primary_mime: MimeType,
    pub preview: Option<String>,
    pub variant_count: usize,
    pub variants: Vec<HistoryVariant>,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryVariant {
    pub mime: MimeType,
    pub file_name: String,
    pub len: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MigrationResult {
    pub scanned: usize,
    pub rewritten: usize,
    pub merged: usize,
}

pub fn default_store_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("CLIP_HISTORY_DIR") {
        return PathBuf::from(value);
    }

    if let Some(value) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(value).join("clip/history");
    }

    if let Some(value) = std::env::var_os("HOME") {
        return PathBuf::from(value).join(".local/state/clip/history");
    }

    PathBuf::from(".clip-history")
}

pub fn save_snapshot(store_dir: &Path, snapshot: &ClipboardSnapshot) -> Result<bool, ClipError> {
    if snapshot.variants.is_empty() {
        return Ok(false);
    }

    fs::create_dir_all(entries_dir(store_dir))?;
    let hash = snapshot_hash(snapshot);
    let latest = latest_entry(store_dir)?;
    if latest.as_ref().is_some_and(|entry| entry.hash == hash) {
        return Ok(false);
    }

    let created_at_ms = next_timestamp_ms(latest.as_ref());
    if let Some(entry) = find_entry_by_hash(store_dir, &hash)? {
        refresh_entry_timestamp(store_dir, entry, created_at_ms)?;
        return Ok(true);
    }

    let id = hash.clone();
    let entry_dir = entries_dir(store_dir).join(&id);
    fs::create_dir_all(&entry_dir)?;

    let primary_mime = choose_primary_mime(snapshot);
    let mut variants = Vec::new();

    for (index, variant) in snapshot.variants.iter().enumerate() {
        let file_name = format!("{index:02}-{}", mime_file_name(&variant.mime));
        fs::write(entry_dir.join(&file_name), &variant.data)?;
        variants.push(HistoryVariant {
            mime: variant.mime.clone(),
            file_name,
            len: variant.data.len(),
        });
    }

    let entry = HistoryEntry {
        id,
        created_at_ms,
        hash,
        copy_count: 1,
        primary_mime,
        preview: None,
        variant_count: variants.len(),
        variants,
        path: entry_dir,
    };
    write_entry_meta(&entry)?;
    rewrite_index(store_dir)?;
    Ok(true)
}

pub fn list_entries(store_dir: &Path) -> Result<Vec<HistoryEntry>, ClipError> {
    if let Ok(entries) = read_index(store_dir) {
        return Ok(entries);
    }
    rebuild_index(store_dir)
}

fn rebuild_index(store_dir: &Path) -> Result<Vec<HistoryEntry>, ClipError> {
    let entries = scan_entries(store_dir)?;
    write_index(store_dir, &entries)?;
    Ok(entries)
}

fn scan_entries(store_dir: &Path) -> Result<Vec<HistoryEntry>, ClipError> {
    let dir = entries_dir(store_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for result in fs::read_dir(dir)? {
        let path = result?.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(entry) = read_entry(&path) {
            entries.push(entry);
        }
    }
    entries.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
    Ok(entries)
}

pub fn latest_entry(store_dir: &Path) -> Result<Option<HistoryEntry>, ClipError> {
    Ok(list_entries(store_dir)?.into_iter().next())
}

pub fn find_entry(store_dir: &Path, id: &str) -> Result<HistoryEntry, ClipError> {
    list_entries(store_dir)?
        .into_iter()
        .find(|entry| entry.id == id || entry.id.starts_with(id))
        .ok_or_else(|| ClipError::config(format!("history entry not found: {id}")))
}

fn find_entry_by_hash(store_dir: &Path, hash: &str) -> Result<Option<HistoryEntry>, ClipError> {
    Ok(list_entries(store_dir)?
        .into_iter()
        .find(|entry| entry.hash == hash))
}

fn refresh_entry_timestamp(
    store_dir: &Path,
    entry: HistoryEntry,
    created_at_ms: u128,
) -> Result<(), ClipError> {
    let mut entry = if entry.variants.is_empty() {
        read_full_entry(&entry)?
    } else {
        entry
    };
    let path = entries_dir(store_dir).join(&entry.hash);
    if entry.path != path {
        fs::rename(&entry.path, &path)?;
    }
    entry.id = entry.hash.clone();
    entry.created_at_ms = created_at_ms;
    entry.copy_count += 1;
    entry.path = path;
    write_entry_meta(&entry)?;
    rewrite_index(store_dir)
}

pub fn delete_entry(store_dir: &Path, id: &str) -> Result<(), ClipError> {
    let entry = find_entry(store_dir, id)?;
    fs::remove_dir_all(entry.path)?;
    rewrite_index(store_dir)?;
    Ok(())
}

pub fn clear_entries(store_dir: &Path) -> Result<(), ClipError> {
    let dir = entries_dir(store_dir);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    let index = index_path(store_dir);
    if index.exists() {
        fs::remove_file(index)?;
    }
    Ok(())
}

pub fn migrate_entries(store_dir: &Path) -> Result<MigrationResult, ClipError> {
    let entries = scan_entries(store_dir)?;
    let mut groups: BTreeMap<String, Vec<HistoryEntry>> = BTreeMap::new();
    for entry in entries {
        groups.entry(entry.hash.clone()).or_default().push(entry);
    }

    let mut result = MigrationResult::default();
    for (_hash, mut entries) in groups {
        result.scanned += entries.len();
        entries.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));

        let mut keep = entries.remove(0);
        let old_copy_count = keep.copy_count;
        keep.copy_count = keep
            .copy_count
            .saturating_add(entries.iter().map(|entry| entry.copy_count).sum::<u64>());

        let merged = entries.len();
        for entry in entries {
            fs::remove_dir_all(entry.path)?;
        }

        let path = entries_dir(store_dir).join(&keep.hash);
        if keep.path != path {
            fs::rename(&keep.path, &path)?;
        }
        keep.id = keep.hash.clone();
        keep.path = path;
        write_entry_meta(&keep)?;
        result.rewritten += 1;
        if merged > 0 {
            result.merged += merged;
        } else if keep.copy_count == old_copy_count {
            // Count single-entry rewrites too: old meta may not have copy_count.
        }
    }

    rewrite_index(store_dir)?;
    Ok(result)
}

pub fn entry_item(entry: &HistoryEntry, mime: Option<&str>) -> Result<ClipboardItem, ClipError> {
    let full_entry;
    let entry = if entry.variants.is_empty() {
        full_entry = read_full_entry(entry)?;
        &full_entry
    } else {
        entry
    };
    let variant = choose_variant(entry, mime)?;
    let data = fs::read(entry.path.join(&variant.file_name))?;
    if variant.mime.as_str() == "text/plain" {
        return Ok(ClipboardItem::Text(String::from_utf8(data).map_err(
            |_| ClipError::config("stored text item is not valid UTF-8"),
        )?));
    }

    Ok(ClipboardItem::bytes(variant.mime.clone(), data))
}

pub fn entry_preview(entry: &HistoryEntry) -> Result<String, ClipError> {
    if let Some(preview) = &entry.preview {
        return Ok(preview.clone());
    }
    let full_entry;
    let entry = if entry.variants.is_empty() {
        full_entry = read_full_entry(entry)?;
        &full_entry
    } else {
        entry
    };
    let variant =
        choose_variant(entry, Some("text/plain")).or_else(|_| choose_variant(entry, None));
    let Ok(variant) = variant else {
        return Ok(String::new());
    };
    if !variant.mime.as_str().starts_with("text/") {
        return Ok(format!("<{} {} bytes>", variant.mime.as_str(), variant.len));
    }
    let text = fs::read_to_string(entry.path.join(&variant.file_name)).unwrap_or_default();
    Ok(single_line_preview(&text, 80))
}

pub fn snapshot_hash(snapshot: &ClipboardSnapshot) -> String {
    let mut variants = snapshot.variants.clone();
    variants.sort_by(|a, b| a.mime.as_str().cmp(b.mime.as_str()));

    let mut hash = Fnv64::new();
    for variant in variants {
        hash.update(variant.mime.as_str().as_bytes());
        hash.update(&[0]);
        hash.update(&variant.data.len().to_le_bytes());
        hash.update(&variant.data);
        hash.update(&[0xff]);
    }
    format!("{:016x}", hash.finish())
}

fn read_entry(path: &Path) -> Result<HistoryEntry, ClipError> {
    let meta = fs::read_to_string(path.join("meta.txt"))?;
    let mut id = None;
    let mut created_at_ms = None;
    let mut hash = None;
    let mut copy_count = None;
    let mut primary_mime = None;
    let mut variants = Vec::new();

    for line in meta.lines() {
        if let Some(value) = line.strip_prefix("id=") {
            id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("created_at_ms=") {
            created_at_ms = value.parse::<u128>().ok();
        } else if let Some(value) = line.strip_prefix("hash=") {
            hash = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("copy_count=") {
            copy_count = value.parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("primary_mime=") {
            primary_mime = Some(MimeType::new(value)?);
        } else if let Some(value) = line.strip_prefix("variant=") {
            let mut parts = value.split('|');
            let mime = parts
                .next()
                .ok_or_else(|| ClipError::config("history variant is missing MIME type"))?;
            let file_name = parts
                .next()
                .ok_or_else(|| ClipError::config("history variant is missing file name"))?;
            let len = parts
                .next()
                .ok_or_else(|| ClipError::config("history variant is missing length"))?
                .parse::<usize>()
                .map_err(|_| ClipError::config("history variant length is invalid"))?;
            variants.push(HistoryVariant {
                mime: MimeType::new(mime)?,
                file_name: file_name.to_string(),
                len,
            });
        }
    }

    let hash = hash.ok_or_else(|| ClipError::config("history entry is missing hash"))?;
    Ok(HistoryEntry {
        id: id.unwrap_or_else(|| hash.clone()),
        created_at_ms: created_at_ms
            .ok_or_else(|| ClipError::config("history entry is missing timestamp"))?,
        hash,
        copy_count: copy_count.unwrap_or(1),
        primary_mime: primary_mime
            .ok_or_else(|| ClipError::config("history entry is missing primary MIME type"))?,
        preview: None,
        variant_count: variants.len(),
        variants,
        path: path.to_path_buf(),
    })
}

fn write_entry_meta(entry: &HistoryEntry) -> Result<(), ClipError> {
    let mut meta = String::new();
    meta.push_str(&format!("id={}\n", entry.id));
    meta.push_str(&format!("created_at_ms={}\n", entry.created_at_ms));
    meta.push_str(&format!("hash={}\n", entry.hash));
    meta.push_str(&format!("copy_count={}\n", entry.copy_count));
    meta.push_str(&format!("primary_mime={}\n", entry.primary_mime.as_str()));

    for variant in &entry.variants {
        meta.push_str(&format!(
            "variant={}|{}|{}\n",
            variant.mime.as_str(),
            variant.file_name,
            variant.len
        ));
    }

    fs::write(entry.path.join("meta.txt"), meta)?;
    Ok(())
}

fn read_index(store_dir: &Path) -> Result<Vec<HistoryEntry>, ClipError> {
    let content = fs::read_to_string(index_path(store_dir))?;
    let mut entries = Vec::new();

    for line in content.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 6 {
            return Err(ClipError::config("history index has invalid row"));
        }
        let created_at_ms = fields[0]
            .parse::<u128>()
            .map_err(|_| ClipError::config("history index timestamp is invalid"))?;
        let hash = fields[1].to_string();
        let copy_count = fields[2]
            .parse::<u64>()
            .map_err(|_| ClipError::config("history index copy count is invalid"))?;
        let primary_mime = MimeType::new(fields[3])?;
        let variant_count = fields[4]
            .parse::<usize>()
            .map_err(|_| ClipError::config("history index variant count is invalid"))?;
        let preview = unescape_index_field(fields[5]);
        let path = entries_dir(store_dir).join(&hash);
        if !path.is_dir() {
            return Err(ClipError::config("history index points to missing entry"));
        }

        entries.push(HistoryEntry {
            id: hash.clone(),
            created_at_ms,
            hash,
            copy_count,
            primary_mime,
            preview: Some(preview),
            variant_count,
            variants: Vec::new(),
            path,
        });
    }

    entries.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
    Ok(entries)
}

fn rewrite_index(store_dir: &Path) -> Result<(), ClipError> {
    let entries = rebuild_index(store_dir)?;
    write_index(store_dir, &entries)
}

fn write_index(store_dir: &Path, entries: &[HistoryEntry]) -> Result<(), ClipError> {
    fs::create_dir_all(store_dir)?;
    let mut content =
        String::from("# created_at_ms\thash\tcopy_count\tprimary_mime\tvariant_count\tpreview\n");
    for entry in entries {
        content.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            entry.created_at_ms,
            entry.hash,
            entry.copy_count,
            entry.primary_mime.as_str(),
            entry.variant_count,
            escape_index_field(&entry_preview(entry).unwrap_or_default())
        ));
    }
    fs::write(index_path(store_dir), content)?;
    Ok(())
}

fn read_full_entry(entry: &HistoryEntry) -> Result<HistoryEntry, ClipError> {
    read_entry(&entry.path)
}

fn choose_variant<'a>(
    entry: &'a HistoryEntry,
    requested_mime: Option<&str>,
) -> Result<&'a HistoryVariant, ClipError> {
    if let Some(mime) = requested_mime {
        return entry
            .variants
            .iter()
            .find(|variant| variant.mime.as_str() == mime)
            .ok_or_else(|| ClipError::config(format!("history entry has no {mime} variant")));
    }

    entry
        .variants
        .iter()
        .find(|variant| variant.mime == entry.primary_mime)
        .or_else(|| entry.variants.first())
        .ok_or_else(|| ClipError::config("history entry has no variants"))
}

fn choose_primary_mime(snapshot: &ClipboardSnapshot) -> MimeType {
    for preferred in ["image/png", "text/html", "text/uri-list", "text/plain"] {
        if let Some(variant) = snapshot
            .variants
            .iter()
            .find(|variant| variant.mime.as_str() == preferred)
        {
            return variant.mime.clone();
        }
    }
    snapshot.variants[0].mime.clone()
}

fn entries_dir(store_dir: &Path) -> PathBuf {
    store_dir.join("entries")
}

fn index_path(store_dir: &Path) -> PathBuf {
    store_dir.join("index.tsv")
}

fn next_timestamp_ms(latest: Option<&HistoryEntry>) -> u128 {
    let now = now_ms();
    match latest {
        Some(entry) if now <= entry.created_at_ms => entry.created_at_ms + 1,
        _ => now,
    }
}

fn mime_file_name(mime: &MimeType) -> String {
    mime.as_str()
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' => ch,
            _ => '_',
        })
        .collect()
}

fn single_line_preview(text: &str, max_chars: usize) -> String {
    let mut value = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() > max_chars {
        value = value.chars().take(max_chars.saturating_sub(3)).collect();
        value.push_str("...");
    }
    value
}

fn escape_index_field(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => ['\\', '\\'].into_iter().collect::<Vec<_>>(),
            '\t' => ['\\', 't'].into_iter().collect(),
            '\n' => ['\\', 'n'].into_iter().collect(),
            '\r' => ['\\', 'r'].into_iter().collect(),
            other => [other].into_iter().collect(),
        })
        .collect()
}

fn unescape_index_field(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('\\') => output.push('\\'),
            Some('t') => output.push('\t'),
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}
