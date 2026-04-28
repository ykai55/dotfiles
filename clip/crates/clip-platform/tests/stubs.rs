use std::sync::Arc;

use clip_core::{ReadRequest, TargetKind};
use clip_platform::{available_targets, resolve_backend, EnvProbe, ProcessCommandRunner};

#[derive(Default)]
struct FakeEnv;

impl EnvProbe for FakeEnv {
    fn os(&self) -> &'static str {
        "linux"
    }

    fn var(&self, _key: &str) -> Option<String> {
        None
    }

    fn command_exists(&self, _name: &str) -> bool {
        false
    }
}

#[test]
fn targets_all_includes_stub_backends() {
    let env = FakeEnv;
    assert_eq!(available_targets(&env, true), vec![TargetKind::Windows, TargetKind::Adb]);
}

#[test]
fn windows_backend_returns_not_implemented_error() {
    let backend =
        resolve_backend(&FakeEnv, Arc::new(ProcessCommandRunner), Some(TargetKind::Windows), None)
            .unwrap();
    let err = backend.read(ReadRequest::text()).unwrap_err();
    assert_eq!(err.to_string(), "windows backend is not implemented yet");
}
