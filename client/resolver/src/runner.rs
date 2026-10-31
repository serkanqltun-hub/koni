use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::ResolveError;

/// Captured result of a finished subprocess.
#[derive(Debug, Clone)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Abstraction over running yt-dlp so tests can inject canned output.
pub trait CommandRunner {
    fn run(
        &self,
        program: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<Output, ResolveError>;
}

/// Real subprocess runner.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(
        &self,
        program: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<Output, ResolveError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A GUI app spawning a console program would flash a console window.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ResolveError::YtDlpNotFound(program.to_owned()),
            _ => ResolveError::Io(e),
        })?;

        // Drain pipes on threads so a chatty child can't block on a full pipe.
        let mut out = child.stdout.take().expect("piped");
        let mut err = child.stderr.take().expect("piped");
        let t_out = thread::spawn(move || {
            let mut b = Vec::new();
            let _ = out.read_to_end(&mut b);
            b
        });
        let t_err = thread::spawn(move || {
            let mut b = Vec::new();
            let _ = err.read_to_end(&mut b);
            b
        });

        let start = Instant::now();
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break st;
            }
            if start.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ResolveError::Timeout(timeout.as_secs()));
            }
            thread::sleep(Duration::from_millis(20));
        };

        Ok(Output {
            success: status.success(),
            stdout: String::from_utf8_lossy(&t_out.join().unwrap_or_default()).into_owned(),
            stderr: String::from_utf8_lossy(&t_err.join().unwrap_or_default()).into_owned(),
        })
    }
}
