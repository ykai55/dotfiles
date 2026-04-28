use clip_core::TargetKind;

#[test]
fn target_kind_names_are_stable() {
    assert_eq!(TargetKind::MacOS.as_str(), "macos");
    assert_eq!(TargetKind::Wayland.as_str(), "wayland");
    assert_eq!(TargetKind::X11.as_str(), "x11");
    assert_eq!(TargetKind::Windows.as_str(), "windows");
    assert_eq!(TargetKind::Adb.as_str(), "adb");
}
