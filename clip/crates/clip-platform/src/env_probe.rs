use std::env;
use std::fs;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(test)]
use std::path::PathBuf;

pub trait EnvProbe {
    fn os(&self) -> &'static str;
    fn var(&self, key: &str) -> Option<String>;
    fn command_exists(&self, name: &str) -> bool;
}

pub struct RealEnvProbe;

impl EnvProbe for RealEnvProbe {
    fn os(&self) -> &'static str {
        env::consts::OS
    }

    fn var(&self, key: &str) -> Option<String> {
        env::var(key).ok()
    }

    fn command_exists(&self, name: &str) -> bool {
        if let Some(paths) = env::var_os("PATH") {
            for dir in env::split_paths(&paths) {
                if is_executable(&dir.join(name)) {
                    return true;
                }
            }
        }

        false
    }
}

fn is_executable(path: &std::path::Path) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };

    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{fs, PathBuf};
    use super::{EnvProbe, RealEnvProbe};
    use std::env;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use super::PermissionsExt;

    #[cfg(unix)]
    #[test]
    fn command_exists_ignores_non_executable_path_entries() {
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(&temp_dir).unwrap();

        let command_path = temp_dir.join("fake-command");
        fs::write(&command_path, b"#!/bin/sh\n").unwrap();

        let mut permissions = fs::metadata(&command_path).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&command_path, permissions).unwrap();

        let _lock = path_env_lock().lock().unwrap();
        let _path_guard = PathVarGuard::set(&temp_dir);

        let probe = RealEnvProbe;
        assert!(!probe.command_exists("fake-command"));

        fs::remove_file(&command_path).unwrap();
        fs::remove_dir(&temp_dir).unwrap();
    }

    fn path_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct PathVarGuard {
        original: Option<OsString>,
    }

    impl PathVarGuard {
        fn set(value: &std::path::Path) -> Self {
            let original = env::var_os("PATH");
            env::set_var("PATH", value);
            Self { original }
        }
    }

    impl Drop for PathVarGuard {
        fn drop(&mut self) {
            match self.original.take() {
                Some(path) => env::set_var("PATH", path),
                None => env::remove_var("PATH"),
            }
        }
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("clip-platform-test-{nanos}"))
    }
}
