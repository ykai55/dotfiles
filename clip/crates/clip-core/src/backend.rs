use crate::{BackendCapabilities, ClipError, ClipboardBlob, ClipboardItem, MimeType, ReadRequest};

pub trait ClipboardBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> BackendCapabilities;
    fn list_types(&self) -> Result<Vec<MimeType>, ClipError>;
    fn read(&self, request: ReadRequest) -> Result<ClipboardBlob, ClipError>;
    fn write(&self, item: &ClipboardItem) -> Result<(), ClipError>;
}
