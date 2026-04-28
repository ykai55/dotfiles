use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, spec: CommandSpec) -> Result<CommandOutput, std::io::Error>;
}

pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, spec: CommandSpec) -> Result<CommandOutput, std::io::Error> {
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
