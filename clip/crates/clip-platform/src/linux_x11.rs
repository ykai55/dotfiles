use std::sync::Arc;

use clip_core::{
    BackendCapabilities, ClipError, ClipboardBackend, ClipboardBlob, ClipboardItem, MimeType,
    ReadRequest,
};

use crate::{CommandOutput, CommandRunner, CommandSpec, X11Tool};

pub struct X11Backend {
    tool: X11Tool,
    runner: Arc<dyn CommandRunner>,
}

impl X11Backend {
    pub fn new(tool: X11Tool, runner: Arc<dyn CommandRunner>) -> Self {
        Self { tool, runner }
    }

    fn run(&self, program: &str, args: &[&str], stdin: Vec<u8>) -> Result<CommandOutput, ClipError> {
        let output = self.runner.run(CommandSpec {
            program: String::from(program),
            args: args.iter().map(|value| String::from(*value)).collect(),
            stdin,
        })?;
        if output.status != 0 {
            return Err(ClipError::clipboard(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(output)
    }

    fn text_plain() -> MimeType {
        MimeType::new("text/plain").unwrap()
    }

    fn is_text_plain(mime: &MimeType) -> bool {
        mime.as_str() == "text/plain"
    }
}

impl ClipboardBackend for X11Backend {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn capabilities(&self) -> BackendCapabilities {
        match self.tool {
            X11Tool::Xclip => BackendCapabilities {
                supports_text: true,
                supports_type_listing: true,
                guaranteed_types: vec![Self::text_plain()],
                supports_custom_mime: true,
            },
            X11Tool::Xsel => BackendCapabilities {
                supports_text: true,
                supports_type_listing: false,
                guaranteed_types: vec![Self::text_plain()],
                supports_custom_mime: false,
            },
        }
    }

    fn list_types(&self) -> Result<Vec<MimeType>, ClipError> {
        match self.tool {
            X11Tool::Xclip => {
                let output =
                    self.run("xclip", &["-selection", "clipboard", "-o", "-t", "TARGETS"], Vec::new())?;
                let mut types = Vec::new();
                for line in output.stdout.split(|byte| *byte == b'\n') {
                    if line.is_empty() {
                        continue;
                    }
                    let value = String::from_utf8_lossy(line);
                    if value == "UTF8_STRING" || value == "STRING" {
                        if !types.iter().any(Self::is_text_plain) {
                            types.push(Self::text_plain());
                        }
                    } else if value.contains('/') {
                        types.push(MimeType::new(value.to_string())?);
                    }
                }
                if types.is_empty() {
                    types.push(Self::text_plain());
                }
                Ok(types)
            }
            X11Tool::Xsel => Err(ClipError::clipboard(
                "xsel backend does not support listing clipboard MIME types",
            )),
        }
    }

    fn read(&self, request: ReadRequest) -> Result<ClipboardBlob, ClipError> {
        match (&self.tool, request.mime()) {
            (X11Tool::Xclip, Some(mime)) => {
                let mime = mime.clone();
                let output =
                    self.run("xclip", &["-selection", "clipboard", "-o", "-t", mime.as_str()], Vec::new())?;
                Ok(ClipboardBlob::Bytes {
                    mime,
                    data: output.stdout,
                })
            }
            (X11Tool::Xclip, None) => {
                let output = self.run("xclip", &["-selection", "clipboard", "-o"], Vec::new())?;
                Ok(ClipboardBlob::Text(String::from_utf8(output.stdout).map_err(|_| {
                    ClipError::clipboard("clipboard text is not valid UTF-8")
                })?))
            }
            (X11Tool::Xsel, Some(mime)) if Self::is_text_plain(mime) => {
                let output = self.run("xsel", &["--clipboard", "--output"], Vec::new())?;
                Ok(ClipboardBlob::Text(String::from_utf8(output.stdout).map_err(|_| {
                    ClipError::clipboard("clipboard text is not valid UTF-8")
                })?))
            }
            (X11Tool::Xsel, Some(_)) => Err(ClipError::clipboard("xsel backend only supports text/plain")),
            (X11Tool::Xsel, None) => {
                let output = self.run("xsel", &["--clipboard", "--output"], Vec::new())?;
                Ok(ClipboardBlob::Text(String::from_utf8(output.stdout).map_err(|_| {
                    ClipError::clipboard("clipboard text is not valid UTF-8")
                })?))
            }
        }
    }

    fn write(&self, item: &ClipboardItem) -> Result<(), ClipError> {
        match (&self.tool, item) {
            (X11Tool::Xclip, ClipboardItem::Text(text)) => {
                self.run("xclip", &["-selection", "clipboard", "-i"], text.as_bytes().to_vec())?;
                Ok(())
            }
            (X11Tool::Xclip, ClipboardItem::Bytes { mime, data }) => {
                self.run("xclip", &["-selection", "clipboard", "-t", mime.as_str(), "-i"], data.clone())?;
                Ok(())
            }
            (X11Tool::Xsel, ClipboardItem::Text(text)) => {
                self.run("xsel", &["--clipboard", "--input"], text.as_bytes().to_vec())?;
                Ok(())
            }
            (X11Tool::Xsel, ClipboardItem::Bytes { mime, data }) if Self::is_text_plain(mime) => {
                self.run("xsel", &["--clipboard", "--input"], data.clone())?;
                Ok(())
            }
            (X11Tool::Xsel, ClipboardItem::Bytes { .. }) => {
                Err(ClipError::clipboard("xsel backend only supports text/plain"))
            }
        }
    }
}
