use clip_core::{ClipError, TargetKind};

use crate::EnvProbe;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum X11Tool {
    Xclip,
    Xsel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendChoice {
    pub target: TargetKind,
    pub x11_tool: Option<X11Tool>,
}

pub fn detect_backend(
    probe: &dyn EnvProbe,
    requested: Option<TargetKind>,
) -> Result<BackendChoice, ClipError> {
    if let Some(target) = requested {
        return detect_requested(probe, target);
    }

    match probe.os() {
        "macos" => Ok(BackendChoice {
            target: TargetKind::MacOS,
            x11_tool: None,
        }),
        "linux" => detect_linux(probe),
        other => Err(ClipError::backend_unavailable(format!(
            "unsupported host os: {other}"
        ))),
    }
}

pub fn available_targets(probe: &dyn EnvProbe, include_stubs: bool) -> Vec<TargetKind> {
    let mut targets = detected_targets(probe);
    if include_stubs {
        push_unique(&mut targets, TargetKind::Windows);
        push_unique(&mut targets, TargetKind::Adb);
    }
    targets
}

fn detect_requested(probe: &dyn EnvProbe, target: TargetKind) -> Result<BackendChoice, ClipError> {
    match target {
        TargetKind::MacOS if probe.os() == "macos" => Ok(BackendChoice {
            target,
            x11_tool: None,
        }),
        TargetKind::Wayland
            if has_non_empty_var(probe, "WAYLAND_DISPLAY")
                && probe.command_exists("wl-copy")
                && probe.command_exists("wl-paste") =>
        {
            Ok(BackendChoice {
                target,
                x11_tool: None,
            })
        }
        TargetKind::X11 if has_non_empty_var(probe, "DISPLAY") && probe.command_exists("xclip") => {
            Ok(BackendChoice {
                target,
                x11_tool: Some(X11Tool::Xclip),
            })
        }
        TargetKind::X11 if has_non_empty_var(probe, "DISPLAY") && probe.command_exists("xsel") => {
            Ok(BackendChoice {
                target,
                x11_tool: Some(X11Tool::Xsel),
            })
        }
        TargetKind::Windows => Ok(BackendChoice {
            target,
            x11_tool: None,
        }),
        TargetKind::Adb => Ok(BackendChoice {
            target,
            x11_tool: None,
        }),
        _ => Err(ClipError::backend_unavailable(format!(
            "target {} is not available in this environment",
            target.as_str()
        ))),
    }
}

fn detect_linux(probe: &dyn EnvProbe) -> Result<BackendChoice, ClipError> {
    if has_non_empty_var(probe, "WAYLAND_DISPLAY")
        && probe.command_exists("wl-copy")
        && probe.command_exists("wl-paste")
    {
        return Ok(BackendChoice {
            target: TargetKind::Wayland,
            x11_tool: None,
        });
    }

    if has_non_empty_var(probe, "DISPLAY") && probe.command_exists("xclip") {
        return Ok(BackendChoice {
            target: TargetKind::X11,
            x11_tool: Some(X11Tool::Xclip),
        });
    }

    if has_non_empty_var(probe, "DISPLAY") && probe.command_exists("xsel") {
        return Ok(BackendChoice {
            target: TargetKind::X11,
            x11_tool: Some(X11Tool::Xsel),
        });
    }

    Err(ClipError::backend_unavailable(
        "no clipboard backend found; expected Wayland with wl-copy/wl-paste or X11 with xclip/xsel",
    ))
}

fn detected_targets(probe: &dyn EnvProbe) -> Vec<TargetKind> {
    let mut targets = Vec::new();

    match probe.os() {
        "macos" => push_unique(&mut targets, TargetKind::MacOS),
        "linux" => {
            if has_non_empty_var(probe, "WAYLAND_DISPLAY")
                && probe.command_exists("wl-copy")
                && probe.command_exists("wl-paste")
            {
                push_unique(&mut targets, TargetKind::Wayland);
            }

            if has_non_empty_var(probe, "DISPLAY")
                && (probe.command_exists("xclip") || probe.command_exists("xsel"))
            {
                push_unique(&mut targets, TargetKind::X11);
            }
        }
        _ => {}
    }

    targets
}

fn push_unique(targets: &mut Vec<TargetKind>, target: TargetKind) {
    if !targets.contains(&target) {
        targets.push(target);
    }
}

fn has_non_empty_var(probe: &dyn EnvProbe, key: &str) -> bool {
    probe.var(key).is_some_and(|value| !value.is_empty())
}
