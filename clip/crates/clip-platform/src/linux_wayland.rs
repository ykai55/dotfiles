use std::sync::Arc;

use clip_core::{
    BackendCapabilities, ClipError, ClipboardBackend, ClipboardBlob, ClipboardItem, MimeType,
    ReadRequest,
};

use crate::{CommandOutput, CommandRunner, CommandSpec};

pub struct WaylandBackend {
    runner: Arc<dyn CommandRunner>,
}

impl WaylandBackend {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdin: Vec<u8>,
        capture_output: bool,
    ) -> Result<CommandOutput, ClipError> {
        let output = self.runner.run(CommandSpec {
            program: String::from(program),
            args: args.iter().map(|value| String::from(*value)).collect(),
            stdin,
            capture_output,
        })?;

        if output.status != 0 {
            return Err(ClipError::clipboard(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        Ok(output)
    }
}

impl ClipboardBackend for WaylandBackend {
    fn name(&self) -> &'static str {
        "wayland"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_text: true,
            supports_type_listing: true,
            guaranteed_types: vec![
                MimeType::new("text/plain").unwrap(),
                MimeType::new("text/html").unwrap(),
                MimeType::new("image/png").unwrap(),
                MimeType::new("text/uri-list").unwrap(),
            ],
            supports_custom_mime: true,
        }
    }

    fn list_types(&self) -> Result<Vec<MimeType>, ClipError> {
        let output = self.run("wl-paste", &["--list-types"], Vec::new(), true)?;
        let mut values = Vec::new();
        for mime in output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .filter_map(|line| MimeType::new(String::from_utf8_lossy(line).to_string()).ok())
        {
            if !values.contains(&mime) {
                values.push(mime);
            }
        }
        Ok(values)
    }

    fn read(&self, request: ReadRequest) -> Result<ClipboardBlob, ClipError> {
        if let Some(mime) = request.mime() {
            let mime = mime.clone();
            let output = self.run(
                "wl-paste",
                &["--type", mime.as_str(), "--no-newline"],
                Vec::new(),
                true,
            )?;
            return Ok(ClipboardBlob::Bytes {
                mime,
                data: output.stdout,
            });
        }

        let output = self.run("wl-paste", &["--no-newline"], Vec::new(), true)?;
        Ok(ClipboardBlob::Text(
            String::from_utf8(output.stdout)
                .map_err(|_| ClipError::clipboard("clipboard text is not valid UTF-8"))?,
        ))
    }

    fn write(&self, item: &ClipboardItem) -> Result<(), ClipError> {
        match item {
            ClipboardItem::Text(text) => {
                self.run(
                    "wl-copy",
                    &["--type", "text/plain"],
                    text.as_bytes().to_vec(),
                    false,
                )?;
            }
            ClipboardItem::Bytes { mime, data } => {
                self.run("wl-copy", &["--type", mime.as_str()], data.clone(), false)?;
            }
            ClipboardItem::Bundle { variants } => {
                let variant = variants
                    .iter()
                    .next()
                    .ok_or_else(|| ClipError::clipboard("clipboard bundle has no variants"))?;
                self.run(
                    "wl-copy",
                    &["--type", variant.mime.as_str()],
                    variant.data.clone(),
                    false,
                )?;
            }
        }
        Ok(())
    }
}
