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
    pub primary_mime: MimeType,
    pub variants: Vec<HistoryVariant>,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryVariant {
    pub mime: MimeType,
    pub file_name: String,
    pub len: usize,
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
    if latest_entry(store_dir)?.is_some_and(|entry| entry.hash == hash) {
        return Ok(false);
    }

    let created_at_ms = now_ms();
    let id = format!("{created_at_ms}-{hash}");
    let entry_dir = entries_dir(store_dir).join(&id);
    fs::create_dir_all(&entry_dir)?;

    let primary_mime = choose_primary_mime(snapshot);
    let mut meta = String::new();
    meta.push_str(&format!("id={id}\n"));
    meta.push_str(&format!("created_at_ms={created_at_ms}\n"));
    meta.push_str(&format!("hash={hash}\n"));
    meta.push_str(&format!("primary_mime={}\n", primary_mime.as_str()));

    for (index, variant) in snapshot.variants.iter().enumerate() {
        let file_name = format!("{index:02}-{}", mime_file_name(&variant.mime));
        fs::write(entry_dir.join(&file_name), &variant.data)?;
        meta.push_str(&format!(
            "variant={}|{}|{}\n",
            variant.mime.as_str(),
            file_name,
            variant.data.len()
        ));
    }

    fs::write(entry_dir.join("meta.txt"), meta)?;
    Ok(true)
}

pub fn list_entries(store_dir: &Path) -> Result<Vec<HistoryEntry>, ClipError> {
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

pub fn delete_entry(store_dir: &Path, id: &str) -> Result<(), ClipError> {
    let entry = find_entry(store_dir, id)?;
    fs::remove_dir_all(entry.path)?;
    Ok(())
}

pub fn clear_entries(store_dir: &Path) -> Result<(), ClipError> {
    let dir = entries_dir(store_dir);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub fn entry_item(entry: &HistoryEntry, mime: Option<&str>) -> Result<ClipboardItem, ClipError> {
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
    let mut primary_mime = None;
    let mut variants = Vec::new();

    for line in meta.lines() {
        if let Some(value) = line.strip_prefix("id=") {
            id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("created_at_ms=") {
            created_at_ms = value.parse::<u128>().ok();
        } else if let Some(value) = line.strip_prefix("hash=") {
            hash = Some(value.to_string());
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

    Ok(HistoryEntry {
        id: id.ok_or_else(|| ClipError::config("history entry is missing id"))?,
        created_at_ms: created_at_ms
            .ok_or_else(|| ClipError::config("history entry is missing timestamp"))?,
        hash: hash.ok_or_else(|| ClipError::config("history entry is missing hash"))?,
        primary_mime: primary_mime
            .ok_or_else(|| ClipError::config("history entry is missing primary MIME type"))?,
        variants,
        path: path.to_path_buf(),
    })
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
