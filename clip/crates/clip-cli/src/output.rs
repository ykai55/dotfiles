use std::fs;
use std::path::Path;

use clip_core::{ClipError, ClipboardBlob, MimeType};

pub fn write_output(blob: ClipboardBlob, output: Option<&Path>) -> Result<(), ClipError> {
    match blob {
        ClipboardBlob::Text(text) => {
            if let Some(path) = output {
                fs::write(path, text.as_bytes())?;
            } else {
                print!("{text}");
            }
            Ok(())
        }
        ClipboardBlob::Bytes { mime, data } => {
            if let Some(text) = decode_text_like_bytes(&mime, &data) {
                if let Some(path) = output {
                    fs::write(path, text.as_bytes())?;
                } else {
                    print!("{text}");
                }
                Ok(())
            } else {
                let path = output.ok_or_else(|| {
                    ClipError::config("binary clipboard reads require --output")
                })?;
                fs::write(path, data)?;
                Ok(())
            }
        }
    }
}

fn decode_text_like_bytes(mime: &MimeType, data: &[u8]) -> Option<String> {
    if !mime.as_str().starts_with("text/") {
        return None;
    }

    String::from_utf8(data.to_vec()).ok()
}
