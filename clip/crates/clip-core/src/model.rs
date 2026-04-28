use crate::MimeType;

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ReadRequest {
    mime: Option<MimeType>,
    prefer_text: bool,
}

impl ReadRequest {
    pub fn text() -> Self {
        Self {
            mime: None,
            prefer_text: true,
        }
    }

    pub fn typed(mime: MimeType) -> Self {
        Self {
            mime: Some(mime),
            prefer_text: false,
        }
    }

    pub fn mime(&self) -> Option<&MimeType> {
        self.mime.as_ref()
    }

    pub fn prefer_text(&self) -> bool {
        self.prefer_text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardBlob {
    Text(String),
    Bytes { mime: MimeType, data: Vec<u8> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardItem {
    Text(String),
    Bytes { mime: MimeType, data: Vec<u8> },
}

impl ClipboardItem {
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    pub fn bytes(mime: MimeType, data: Vec<u8>) -> Self {
        Self::Bytes { mime, data }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendCapabilities {
    pub supports_text: bool,
    pub supports_type_listing: bool,
    pub guaranteed_types: Vec<MimeType>,
    pub supports_custom_mime: bool,
}
