use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
    pub capture_output: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{CommandRunner, CommandSpec, ProcessCommandRunner};

    #[test]
    #[cfg(unix)]
    fn run_without_output_capture_does_not_wait_for_inherited_pipes() {
        let runner = ProcessCommandRunner;
        let started = Instant::now();

        let output = runner
            .run(CommandSpec {
                program: String::from("sh"),
                args: vec![String::from("-c"), String::from("sleep 2 & exit 0")],
                stdin: Vec::new(),
                capture_output: false,
            })
            .unwrap();

        assert_eq!(output.status, 0);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, spec: CommandSpec) -> Result<CommandOutput, std::io::Error>;
}

pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, spec: CommandSpec) -> Result<CommandOutput, std::io::Error> {
        if !spec.capture_output {
            let mut child = Command::new(&spec.program)
                .args(&spec.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;

            if let Some(mut stdin) = child.stdin.take() {
                if let Err(error) = stdin.write_all(&spec.stdin) {
                    drop(stdin);
                    let _ = child.wait();
                    return Err(error);
                }
            }

            let status = child.wait()?;
            return Ok(CommandOutput {
                status: status.code().unwrap_or(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }

        let mut child = Command::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(&spec.stdin) {
                drop(stdin);
                let _ = child.wait();
                return Err(error);
            }
        }

        let output = child.wait_with_output()?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
