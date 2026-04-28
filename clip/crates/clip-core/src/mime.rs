use crate::ClipError;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MimeType(String);

impl MimeType {
    pub fn new(value: impl Into<String>) -> Result<Self, ClipError> {
        let value = value.into();
        let mut parts = value.split('/');
        let top = parts.next();
        let sub = parts.next();
        if value.chars().any(char::is_whitespace)
            || top.is_none_or(str::is_empty)
            || sub.is_none_or(str::is_empty)
            || parts.next().is_some()
        {
            return Err(ClipError::config(format!("invalid mime type: {value}")));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
