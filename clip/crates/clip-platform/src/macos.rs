use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use clip_core::{
    BackendCapabilities, ClipError, ClipboardBackend, ClipboardBlob, ClipboardItem,
    ClipboardVariant, MimeType, ReadRequest,
};

use crate::{CommandOutput, CommandRunner, CommandSpec};

pub struct MacOsBackend {
    runner: Arc<dyn CommandRunner>,
    helper: PathBuf,
}

impl MacOsBackend {
    pub fn new(runner: Arc<dyn CommandRunner>, helper: PathBuf) -> Self {
        Self { runner, helper }
    }

    fn run(
        &self,
        args: &[&str],
        stdin: Vec<u8>,
        capture_output: bool,
    ) -> Result<CommandOutput, ClipError> {
        let output = self.runner.run(CommandSpec {
            program: self.helper.display().to_string(),
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

    fn map_helper_type(raw: &str) -> Option<MimeType> {
        match raw {
            "public.utf8-plain-text" => Some(MimeType::new("text/plain").unwrap()),
            "public.html" => Some(MimeType::new("text/html").unwrap()),
            "public.png" => Some(MimeType::new("image/png").unwrap()),
            "public.url" => Some(MimeType::new("text/uri-list").unwrap()),
            "public.file-url" => Some(MimeType::new("text/uri-list").unwrap()),
            _ => None,
        }
    }

    fn supports_mime(mime: &MimeType) -> bool {
        matches!(
            mime.as_str(),
            "text/plain" | "text/html" | "image/png" | "text/uri-list"
        )
    }

    fn unsupported_type_error() -> ClipError {
        ClipError::clipboard(
            "macos backend only supports text/plain, text/html, image/png, and text/uri-list",
        )
    }

    fn encode_bundle(variants: &[ClipboardVariant]) -> Result<Vec<u8>, ClipError> {
        let mut output = b"clip-bundle-v1\n".to_vec();
        for variant in variants {
            if !Self::supports_mime(&variant.mime) {
                return Err(Self::unsupported_type_error());
            }
            output.extend_from_slice(variant.mime.as_str().as_bytes());
            output.push(b'\n');
            output.extend_from_slice(variant.data.len().to_string().as_bytes());
            output.push(b'\n');
            output.extend_from_slice(&variant.data);
            output.push(b'\n');
        }
        Ok(output)
    }
}

pub fn resolve_macos_helper_path(override_path: Option<PathBuf>) -> Result<PathBuf, ClipError> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    resolve_macos_helper_path_for(&repo_root, override_path)
}

pub fn resolve_macos_helper_path_for(
    repo_root: &Path,
    override_path: Option<PathBuf>,
) -> Result<PathBuf, ClipError> {
    if let Some(path) = override_path {
        return Ok(path);
    }

    let helper_dir = repo_root.join("helpers/macos/clip-macos-helper");
    let arch_dirs = match std::env::consts::ARCH {
        "aarch64" => ["arm64-apple-macosx", "aarch64-apple-macosx"],
        "x86_64" => ["x86_64-apple-macosx", "x86_64-apple-macosx"],
        other => [other, other],
    };
    let candidates = [
        helper_dir.join(format!(".build/{}/release/clip-macos-helper", arch_dirs[0])),
        helper_dir.join(format!(".build/{}/debug/clip-macos-helper", arch_dirs[0])),
        helper_dir.join(format!(".build/{}/release/clip-macos-helper", arch_dirs[1])),
        helper_dir.join(format!(".build/{}/debug/clip-macos-helper", arch_dirs[1])),
        helper_dir.join(".build/release/clip-macos-helper"),
        helper_dir.join(".build/debug/clip-macos-helper"),
    ];

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            ClipError::backend_unavailable(
                "macos backend could not find the built helper; set CLIP_MACOS_HELPER or build helpers/macos/clip-macos-helper",
            )
        })
}

impl ClipboardBackend for MacOsBackend {
    fn name(&self) -> &'static str {
        "macos"
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
            supports_custom_mime: false,
        }
    }

    fn list_types(&self) -> Result<Vec<MimeType>, ClipError> {
        let output = self.run(&["types"], Vec::new(), true)?;
        let mut values = Vec::new();
        for mime in output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .filter_map(|line| Self::map_helper_type(&String::from_utf8_lossy(line)))
        {
            if !values.contains(&mime) {
                values.push(mime);
            }
        }
        Ok(values)
    }

    fn read(&self, request: ReadRequest) -> Result<ClipboardBlob, ClipError> {
        if let Some(mime) = request.mime() {
            if !Self::supports_mime(mime) {
                return Err(Self::unsupported_type_error());
            }
            let mime = mime.clone();
            let output = self.run(&["read", "--type", mime.as_str()], Vec::new(), true)?;
            return Ok(ClipboardBlob::Bytes {
                mime,
                data: output.stdout,
            });
        }

        let output = self.run(&["read", "--type", "text/plain"], Vec::new(), true)?;
        Ok(ClipboardBlob::Text(
            String::from_utf8(output.stdout)
                .map_err(|_| ClipError::clipboard("clipboard text is not valid UTF-8"))?,
        ))
    }

    fn write(&self, item: &ClipboardItem) -> Result<(), ClipError> {
        match item {
            ClipboardItem::Text(text) => {
                self.run(
                    &["write", "--type", "text/plain"],
                    text.as_bytes().to_vec(),
                    false,
                )?;
            }
            ClipboardItem::Bytes { mime, data } => {
                if !Self::supports_mime(mime) {
                    return Err(Self::unsupported_type_error());
                }
                self.run(&["write", "--type", mime.as_str()], data.clone(), false)?;
            }
            ClipboardItem::Bundle { variants } => {
                self.run(&["write-bundle"], Self::encode_bundle(variants)?, false)?;
            }
        }
        Ok(())
    }
}
