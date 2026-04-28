use clip_core::TargetKind;
use clip_platform::{available_targets, detect_backend, BackendChoice, EnvProbe, X11Tool};

#[derive(Default)]
struct FakeEnv {
    os: &'static str,
    wayland_display: Option<&'static str>,
    display: Option<&'static str>,
    wl_copy: bool,
    wl_paste: bool,
    xclip: bool,
    xsel: bool,
}

impl EnvProbe for FakeEnv {
    fn os(&self) -> &'static str {
        self.os
    }

    fn var(&self, key: &str) -> Option<String> {
        match key {
            "WAYLAND_DISPLAY" => self.wayland_display.map(String::from),
            "DISPLAY" => self.display.map(String::from),
            _ => None,
        }
    }

    fn command_exists(&self, name: &str) -> bool {
        match name {
            "wl-copy" => self.wl_copy,
            "wl-paste" => self.wl_paste,
            "xclip" => self.xclip,
            "xsel" => self.xsel,
            _ => false,
        }
    }
}

#[test]
fn auto_selects_wayland_when_display_and_tools_exist() {
    let env = FakeEnv {
        os: "linux",
        wayland_display: Some("wayland-0"),
        wl_copy: true,
        wl_paste: true,
        ..Default::default()
    };

    assert_eq!(
        detect_backend(&env, None).unwrap(),
        BackendChoice {
            target: TargetKind::Wayland,
            x11_tool: None,
        }
    );
}

#[test]
fn falls_back_to_xsel_when_xclip_is_missing() {
    let env = FakeEnv {
        os: "linux",
        display: Some(":0"),
        xsel: true,
        ..Default::default()
    };

    assert_eq!(
        detect_backend(&env, None).unwrap(),
        BackendChoice {
            target: TargetKind::X11,
            x11_tool: Some(X11Tool::Xsel),
        }
    );
}

#[test]
fn targets_include_all_detected_backends_in_preferred_order() {
    let env = FakeEnv {
        os: "linux",
        wayland_display: Some("wayland-0"),
        display: Some(":0"),
        wl_copy: true,
        wl_paste: true,
        xclip: true,
        ..Default::default()
    };

    assert_eq!(
        available_targets(&env, false),
        vec![TargetKind::Wayland, TargetKind::X11]
    );
}

#[test]
fn empty_display_variables_do_not_activate_backends() {
    let env = FakeEnv {
        os: "linux",
        wayland_display: Some(""),
        display: Some(""),
        wl_copy: true,
        wl_paste: true,
        xclip: true,
        xsel: true,
        ..Default::default()
    };

    assert!(detect_backend(&env, None).is_err());
    assert_eq!(available_targets(&env, false), Vec::<TargetKind>::new());
}

#[test]
fn targets_without_stubs_only_include_detected_backends() {
    let env = FakeEnv {
        os: "macos",
        ..Default::default()
    };

    assert_eq!(available_targets(&env, false), vec![TargetKind::MacOS]);
}
