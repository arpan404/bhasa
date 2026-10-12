//! Capture one owned Unix process group without waiting for inherited pipes.
//! Stdin is preserved so callers can supply null input or an explicit fixture.
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

struct Logs(PathBuf);
impl Logs {
    fn new() -> Self {
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "bhasa-channel-process-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create {}: {error}", path.display()),
            }
        }
        panic!("cannot create channel process logs");
    }
}
impl Drop for Logs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Group {
    child: Child,
    id: i32,
    reaped: bool,
    cleaned: bool,
}
impl Group {
    fn kill_all(&mut self) -> io::Result<()> {
        // process_group(0) gave this child its own group. Negative PID signals
        // only that group, including descendants which inherited log handles.
        let result = unsafe { kill(-self.id, 9) };
        if result != 0 {
            let error = io::Error::last_os_error();
            // ESRCH: a normally exiting process already removed the group.
            if error.raw_os_error() != Some(3) {
                return Err(error);
            }
        }
        self.cleaned = true;
        Ok(())
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.kill_all();
        }
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

struct Capture {
    output: Output,
    timed_out: bool,
}

fn read_log(path: &std::path::Path) -> Vec<u8> {
    const MAX_OUTPUT: u64 = 4 * 1024 * 1024;
    let mut bytes = Vec::new();
    File::open(path)
        .unwrap()
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(
        bytes.len() as u64 <= MAX_OUTPUT,
        "channel fixture output exceeded 4 MiB"
    );
    bytes
}

fn capture(command: &mut Command, limit: Duration) -> io::Result<Capture> {
    let logs = Logs::new();
    let stdout = logs.0.join("stdout");
    let stderr = logs.0.join("stderr");
    let child = command
        .process_group(0)
        .stdout(Stdio::from(File::create(&stdout).unwrap()))
        .stderr(Stdio::from(File::create(&stderr).unwrap()))
        .spawn()?;
    let id = i32::try_from(child.id()).expect("Unix child PID must fit pid_t");
    let mut group = Group {
        child,
        id,
        reaped: false,
        cleaned: false,
    };
    let deadline = Instant::now() + limit;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = group.child.try_wait().unwrap() {
            group.reaped = true;
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            group.kill_all().unwrap();
            let status = group.child.wait().unwrap();
            group.reaped = true;
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // A successful direct child can also leave descendants holding inherited
    // descriptors. Clean its group before reading logs, on every exit path.
    if !group.cleaned {
        group.kill_all().unwrap();
    }
    drop(group);
    Ok(Capture {
        output: Output {
            status,
            stdout: read_log(&stdout),
            stderr: read_log(&stderr),
        },
        timed_out,
    })
}

pub fn bounded(command: &mut Command) -> Output {
    let captured = capture(command, Duration::from_secs(40)).unwrap();
    assert!(
        !captured.timed_out,
        "channel fixture exceeded 40-second deadline: {command:?}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        captured.output.status,
        String::from_utf8_lossy(&captured.output.stdout),
        String::from_utf8_lossy(&captured.output.stderr),
    );
    captured.output
}

pub fn configured_clang() -> Option<OsString> {
    let executable = std::env::var_os("BHASA_CLANG").unwrap_or_else(|| "clang".into());
    let captured = match capture(
        Command::new(&executable)
            .arg("--version")
            .stdin(Stdio::null()),
        Duration::from_secs(40),
    ) {
        Ok(captured) => captured,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("SKIP native test: Clang executable {executable:?} was not found");
            return None;
        }
        Err(error) => panic!("cannot execute Clang {executable:?}: {error}"),
    };
    assert!(
        !captured.timed_out,
        "configured Clang version probe exceeded 40 seconds"
    );
    assert!(
        captured.output.status.success(),
        "configured Clang failed: {:?}",
        captured.output
    );
    Some(executable)
}

#[cfg(test)]
mod regression {
    use super::*;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    fn descendant(exit_parent: bool) {
        let (mut witness, inherited) = UnixStream::pair().unwrap();
        witness
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let input: OwnedFd = inherited.into();
        let mut command = Command::new("/bin/sh");
        command.stdin(Stdio::from(input)).args([
            "-c",
            if exit_parent {
                "exec 3<&0; /bin/sleep 30 <&3 & printf 'ready\\n'; exit 0"
            } else {
                "exec 3<&0; /bin/sleep 30 <&3 & printf 'ready\\n'; wait"
            },
        ]);
        let start = Instant::now();
        let captured = capture(&mut command, Duration::from_millis(250)).unwrap();
        assert_eq!(captured.timed_out, !exit_parent);
        assert_eq!(captured.output.stdout, b"ready\n");
        assert_eq!(captured.output.status.success(), exit_parent);
        assert!(start.elapsed() < Duration::from_secs(5));
        // Close any stdin handle retained by Command. EOF then proves the
        // descendant also released its inherited descriptor; a zombie has
        // already released descriptors, so this does not depend on PID 1 reap.
        drop(command);
        assert_eq!(witness.read(&mut [0_u8]).unwrap(), 0);
    }

    #[test]
    fn immediate_exit_cleans_descendant_with_inherited_streams() {
        descendant(true);
    }

    #[test]
    fn timeout_cleans_descendant_with_inherited_streams() {
        descendant(false);
    }
}
