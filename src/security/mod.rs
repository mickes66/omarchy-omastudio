use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

/// Whitelisted environment variables for subprocess execution
pub const WHITELISTED_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LC_ALL",
    "LANG",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
];

/// Maximum buffer size for command output to prevent memory overrun (64 KiB)
pub const MAX_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;

/// Cleans and configures a Command according to Omarchy security standards:
/// - Isolated process group (cmd.process_group(0))
/// - Cleared environment variables with whitelist only
pub fn secure_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.env_clear();

    for &var in WHITELISTED_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
    cmd.env("LC_ALL", "C");
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

/// Reaps a process group unconditionally with SIGTERM followed by SIGKILL
pub fn reap_process_group(child: &mut Child) {
    let pid = child.id() as i32;
    if pid <= 1 {
        return;
    }

    // SAFETY: pid is verified > 1, libc kill with negative pid targets the process group
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }

    // Grace period for cooperative shutdown (5-10ms)
    thread::sleep(Duration::from_millis(8));

    // Forceful kill
    // SAFETY: pid is verified > 1
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }

    // Prevent zombies
    let _ = child.wait();
}

/// RAII Guard ensuring process groups are terminated even upon panic or early return
pub struct ProcessGroupGuard<'a> {
    child: &'a mut Child,
    active: bool,
}

impl<'a> ProcessGroupGuard<'a> {
    pub fn new(child: &'a mut Child) -> Self {
        Self {
            child,
            active: true,
        }
    }

    pub fn defuse(mut self) {
        self.active = false;
    }
}

impl<'a> Drop for ProcessGroupGuard<'a> {
    fn drop(&mut self) {
        if self.active {
            reap_process_group(self.child);
        }
    }
}

/// Runs a command with a monotonic deadline, non-blocking I/O polling, and buffer limit
pub fn run_bounded_command(
    mut cmd: Command,
    timeout: Duration,
) -> io::Result<(i32, Vec<u8>, Vec<u8>)> {
    use std::os::unix::io::AsRawFd;

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    let guard = ProcessGroupGuard::new(&mut child);

    let deadline = Instant::now() + timeout;

    let stdout_fd = guard.child.stdout.as_ref().map(|p| p.as_raw_fd());
    let stderr_fd = guard.child.stderr.as_ref().map(|p| p.as_raw_fd());

    // Set non-blocking on pipes
    for &fd_opt in &[stdout_fd, stderr_fd] {
        if let Some(fd) = fd_opt {
            // SAFETY: fd is valid and owned by child process stdio
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                if flags >= 0 {
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
        }
    }

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut temp_chunk = [0u8; 4096];

    let mut child_exited = false;
    let mut exit_code = -1;

    while Instant::now() < deadline {
        // Poll for child exit
        if !child_exited {
            if let Ok(Some(status)) = guard.child.try_wait() {
                child_exited = true;
                exit_code = status.code().unwrap_or(-1);
            }
        }

        // Drain stdout
        if let Some(ref mut pipe) = guard.child.stdout {
            loop {
                match pipe.read(&mut temp_chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if stdout_buf.len() + n > MAX_COMMAND_OUTPUT_BYTES {
                            return Err(io::Error::new(
                                io::ErrorKind::OutOfMemory,
                                "Command stdout exceeded buffer cap",
                            ));
                        }
                        stdout_buf.extend_from_slice(&temp_chunk[..n]);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e),
                }
            }
        }

        // Drain stderr
        if let Some(ref mut pipe) = guard.child.stderr {
            loop {
                match pipe.read(&mut temp_chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if stderr_buf.len() + n > MAX_COMMAND_OUTPUT_BYTES {
                            return Err(io::Error::new(
                                io::ErrorKind::OutOfMemory,
                                "Command stderr exceeded buffer cap",
                            ));
                        }
                        stderr_buf.extend_from_slice(&temp_chunk[..n]);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e),
                }
            }
        }

        if child_exited {
            break;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        let sleep_step = Duration::from_millis(50).min(remaining);
        if sleep_step.is_zero() {
            break;
        }
        thread::sleep(sleep_step);
    }

    if !child_exited {
        // Timed out: guard will reap the process group
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Command exceeded monotonic deadline",
        ));
    }

    guard.defuse();
    Ok((exit_code, stdout_buf, stderr_buf))
}

/// Ensures directory exists with strict Mode 0700 permissions
pub fn ensure_secure_dir(path: &Path) -> io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(path, perms)?;
    } else {
        // If directory already exists, enforce 0700 if owned by user, but tolerate system dirs (e.g. /tmp, /dev/shm)
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o700);
        if let Err(e) = fs::set_permissions(path, perms) {
            if e.raw_os_error() != Some(libc::EPERM) && e.raw_os_error() != Some(libc::EACCES) {
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Verifies that a file is a regular file and NOT a symlink
pub fn verify_safe_file(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Symlink access is strictly prohibited",
        ));
    }
    if !meta.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Target is not a regular file",
        ));
    }
    Ok(())
}

/// Atomically writes sensitive data to a file with Mode 0600 permissions
pub fn atomic_write_secure(target: &Path, data: &[u8]) -> io::Result<()> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    ensure_secure_dir(parent)?;

    let temp_name = format!(".tmp_{}_{}", std::process::id(), Instant::now().elapsed().as_nanos());
    let temp_path = parent.join(temp_name);

    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp_path)?;

        file.write_all(data)?;
        file.sync_all()?;
    }

    // Verify mode 0600 on created file
    let mut perms = fs::metadata(&temp_path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&temp_path, perms)?;

    fs::rename(&temp_path, target)?;
    Ok(())
}
