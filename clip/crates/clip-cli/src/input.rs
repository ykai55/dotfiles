use std::fs;
use std::io::{IsTerminal, Read};

use clip_core::{ClipError, ClipboardItem, MimeType};

use crate::args::SetArgs;

pub fn load_item(args: &SetArgs) -> Result<ClipboardItem, ClipError> {
    let mime = args.mime.as_deref().map(MimeType::new).transpose()?;
    let stdin_bytes = read_stdin_if_piped()?;
    let has_stdin_input = stdin_bytes.is_some();

    if has_stdin_input && (args.text.is_some() || args.input.is_some()) {
        return Err(ClipError::config(
            "set accepts exactly one of positional text, stdin, or --input",
        ));
    }

    if let Some(text) = &args.text {
        return Ok(match mime {
            Some(mime) => ClipboardItem::bytes(mime, text.as_bytes().to_vec()),
            None => ClipboardItem::text(text.clone()),
        });
    }

    if let Some(path) = &args.input {
        return Ok(match mime {
            Some(mime) => ClipboardItem::bytes(mime, fs::read(path)?),
            None => ClipboardItem::text(String::from_utf8(fs::read(path)?).map_err(|_| {
                ClipError::config("input is not valid UTF-8; pass --type to read raw bytes")
            })?),
        });
    }

    if let Some(mime) = mime {
        if let Some(buffer) = stdin_bytes {
            return Ok(ClipboardItem::bytes(mime, buffer));
        }

        return Err(ClipError::config(
            "set accepts exactly one of positional text, stdin, or --input",
        ));
    }

    if let Some(buffer) = stdin_bytes {
        return Ok(ClipboardItem::text(String::from_utf8(buffer).map_err(
            |_| ClipError::config("input is not valid UTF-8; pass --type to read raw bytes"),
        )?));
    }

    Err(ClipError::config(
        "set accepts exactly one of positional text, stdin, or --input",
    ))
}

fn read_stdin_if_piped() -> Result<Option<Vec<u8>>, ClipError> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }

    let mut buffer = Vec::new();
    stdin.read_to_end(&mut buffer)?;
    Ok(Some(buffer))
}
