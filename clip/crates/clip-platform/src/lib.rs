mod adb_stub;
mod command_runner;
mod detect;
mod env_probe;
mod linux_wayland;
mod linux_x11;
mod macos;
mod windows_stub;

use std::path::PathBuf;
use std::sync::Arc;

use clip_core::{ClipError, ClipboardBackend, TargetKind};

pub use adb_stub::AdbBackend;
pub use command_runner::{CommandOutput, CommandRunner, CommandSpec, ProcessCommandRunner};
pub use detect::{available_targets, detect_backend, BackendChoice, X11Tool};
pub use env_probe::{EnvProbe, RealEnvProbe};
pub use linux_wayland::WaylandBackend;
pub use linux_x11::X11Backend;
pub use macos::{resolve_macos_helper_path, resolve_macos_helper_path_for, MacOsBackend};
pub use windows_stub::WindowsBackend;

pub fn resolve_backend(
    probe: &dyn EnvProbe,
    runner: Arc<dyn CommandRunner>,
    requested: Option<TargetKind>,
    mac_helper: Option<PathBuf>,
) -> Result<Box<dyn ClipboardBackend>, ClipError> {
    let choice = detect_backend(probe, requested)?;
    match choice.target {
        TargetKind::Wayland => Ok(Box::new(WaylandBackend::new(runner))),
        TargetKind::X11 => Ok(Box::new(X11Backend::new(choice.x11_tool.unwrap(), runner))),
        TargetKind::MacOS => {
            let helper = resolve_macos_helper_path(mac_helper)?;
            Ok(Box::new(MacOsBackend::new(runner, helper)))
        }
        TargetKind::Windows => Ok(Box::new(WindowsBackend)),
        TargetKind::Adb => Ok(Box::new(AdbBackend)),
    }
}
