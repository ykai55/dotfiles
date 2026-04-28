use clip_core::{
    BackendCapabilities, ClipError, ClipboardBackend, ClipboardBlob, ClipboardItem, MimeType,
    ReadRequest,
};

pub struct AdbBackend;

impl ClipboardBackend for AdbBackend {
    fn name(&self) -> &'static str {
        "adb"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }

    fn list_types(&self) -> Result<Vec<MimeType>, ClipError> {
        Err(ClipError::backend_unavailable(
            "adb backend is not implemented yet",
        ))
    }

    fn read(&self, _request: ReadRequest) -> Result<ClipboardBlob, ClipError> {
        Err(ClipError::backend_unavailable(
            "adb backend is not implemented yet",
        ))
    }

    fn write(&self, _item: &ClipboardItem) -> Result<(), ClipError> {
        Err(ClipError::backend_unavailable(
            "adb backend is not implemented yet",
        ))
    }
}
