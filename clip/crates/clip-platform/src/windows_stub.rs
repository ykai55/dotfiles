use clip_core::{
    BackendCapabilities, ClipError, ClipboardBackend, ClipboardBlob, ClipboardItem, MimeType,
    ReadRequest,
};

pub struct WindowsBackend;

impl ClipboardBackend for WindowsBackend {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }

    fn list_types(&self) -> Result<Vec<MimeType>, ClipError> {
        Err(ClipError::backend_unavailable(
            "windows backend is not implemented yet",
        ))
    }

    fn read(&self, _request: ReadRequest) -> Result<ClipboardBlob, ClipError> {
        Err(ClipError::backend_unavailable(
            "windows backend is not implemented yet",
        ))
    }

    fn write(&self, _item: &ClipboardItem) -> Result<(), ClipError> {
        Err(ClipError::backend_unavailable(
            "windows backend is not implemented yet",
        ))
    }
}
