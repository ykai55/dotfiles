#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetKind {
    MacOS,
    Wayland,
    X11,
    Windows,
    Adb,
}

impl TargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MacOS => "macos",
            Self::Wayland => "wayland",
            Self::X11 => "x11",
            Self::Windows => "windows",
            Self::Adb => "adb",
        }
    }
}
