use std::time::{Duration, Instant};
use tokio::process::Command;

pub struct SandboxConfig {
    pub timeout_secs: u64,
    pub max_memory_mb: u64,
    pub image: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 60,
            max_memory_mb: 256,
            image: "ubuntu:24.04".into(),
        }
    }
}

pub enum SandboxMode {
    Docker(String),
    Direct,
}

pub struct SandboxResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

pub struct Sandbox {
    config: SandboxConfig,
    mode: SandboxMode,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        let mode = if Self::check_docker() {
            SandboxMode::Docker(config.image.clone())
        } else {
            SandboxMode::Direct
        };
        Self { config, mode }
    }

    pub fn with_direct(config: SandboxConfig) -> Self {
        Self {
            config,
            mode: SandboxMode::Direct,
        }
    }

    fn check_docker() -> bool {
        std::process::Command::new("docker")
            .arg("info")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub async fn execute(&self, command: &str, args: &[&str]) -> Result<SandboxResult, String> {
        match &self.mode {
            SandboxMode::Docker(image) => self.execute_docker(image, command, args).await,
            SandboxMode::Direct => self.execute_direct(command, args).await,
        }
    }

    async fn run_command(
        cmd: &mut Command,
        timeout: Duration,
    ) -> Result<SandboxResult, String> {
        let start = Instant::now();

        match tokio::time::timeout(timeout, cmd.output()).await {
            Ok(Ok(output)) => Ok(SandboxResult {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
                duration_ms: start.elapsed().as_millis() as u64,
            }),
            Ok(Err(e)) => Err(format!("Command lỗi: {}", e)),
            Err(_) => Err(format!("Timeout sau {}s", timeout.as_secs())),
        }
    }

    async fn execute_docker(&self, image: &str, command: &str, args: &[&str]) -> Result<SandboxResult, String> {
        let mut cmd = Command::new("docker");
        cmd.args([
            "run", "--rm",
            "-i",
            &format!("--memory={}m", self.config.max_memory_mb),
            &format!("--memory-swap={}m", self.config.max_memory_mb),
            "--network", "none",
            "--pids-limit", "64",
            "--read-only",
            image,
            command,
        ]);
        cmd.args(args);

        let timeout = Duration::from_secs(self.config.timeout_secs);
        Self::run_command(&mut cmd, timeout).await
    }

    async fn execute_direct(&self, command: &str, args: &[&str]) -> Result<SandboxResult, String> {
        let mut cmd = Command::new(command);
        cmd.args(args);

        let timeout = Duration::from_secs(self.config.timeout_secs);
        Self::run_command(&mut cmd, timeout).await
    }

    pub async fn execute_script(&self, script: &str) -> Result<SandboxResult, String> {
        match &self.mode {
            SandboxMode::Docker(image) => {
                let mut cmd = Command::new("docker");
                cmd.args([
                    "run", "--rm",
                    "-i",
                    &format!("--memory={}m", self.config.max_memory_mb),
                    "--network", "none",
                    "--pids-limit", "64",
                    "--read-only",
                    image,
                    "bash", "-c", script,
                ]);
                let timeout = Duration::from_secs(self.config.timeout_secs);
                Self::run_command(&mut cmd, timeout).await
            }
            SandboxMode::Direct => {
                let mut cmd = Command::new("bash");
                cmd.args(["-c", script]);
                let timeout = Duration::from_secs(self.config.timeout_secs);
                Self::run_command(&mut cmd, timeout).await
            }
        }
    }

    pub fn mode(&self) -> &str {
        match &self.mode {
            SandboxMode::Docker(_) => "docker",
            SandboxMode::Direct => "direct",
        }
    }
}
