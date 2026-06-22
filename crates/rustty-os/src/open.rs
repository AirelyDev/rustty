//! rustty-os::open
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/open.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std`, `builtin`, `Allocator = std.mem.Allocator`,
// `build_config = @import("../build_config.zig")`, and `apprt = @import("../apprt.zig")`,
// and uses `std.log.scoped(.@"os-open")`.
//
// R4 SEAM: the reference's `apprt` import is a cycle (apprt depends on os).
// Break #14 relocates `os.open` into `apprt`; this port keeps the file in
// rustty-os per the porting task but expresses the apprt dependency as a minimal
// local seam type (`Kind`) rather than depending on rustty-apprt-api. This adds
// nothing-lost (R1): it is the parity image of `apprt.action.OpenUrl.Kind`, and
// the `open` logic is ported verbatim. `build_config` maps to `rustty_build_config`.
//
// In Rust the child-process plumbing uses `std::process::Command`; the OS switch
// is `cfg!(target_os = ..)`; the stderr-reaping thread is a `std::thread`.

use rustty_build_config as build_config;

#[allow(dead_code)]
const LOG_SCOPE: &str = "os-open";

/// PORT (R4 seam): the parity image of `apprt.action.OpenUrl.Kind`. The type of
/// the data at the URL to open. This is used as a hint to potentially open the
/// URL in a different way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The type is unknown.
    Unknown,
    /// The URL is known to be a text file.
    Text,
    /// The URL is known to contain HTML content.
    Html,
}

/// PORT: the reference's error union from `open`/`std.process.Child` operations.
#[derive(Debug)]
pub enum OpenError {
    Unimplemented,
    Spawn(std::io::Error),
}

/// Open a URL in the default handling application.
///
/// Any output on stderr is logged as a warning in the application logs.
/// Output on stdout is ignored.
///
/// This function is purposely simple for the sake of providing some portable
/// way to open URLs. If you are implementing an apprt for Rustty, you should
/// consider doing something special-cased for your platform.
#[allow(unused_variables)]
pub fn open(kind: Kind, url: &str) -> Result<(), OpenError> {
    let mut exe: std::process::Command = {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(url);
            c
        }
        #[cfg(target_os = "windows")]
        {
            let mut c = std::process::Command::new("rundll32");
            c.args(["url.dll,FileProtocolHandler", url]);
            c
        }
        #[cfg(target_os = "macos")]
        {
            match kind {
                Kind::Text => {
                    let mut c = std::process::Command::new("open");
                    c.args(["-t", url]);
                    c
                }
                Kind::Html | Kind::Unknown => {
                    let mut c = std::process::Command::new("open");
                    c.arg(url);
                    c
                }
            }
        }
        #[cfg(target_os = "ios")]
        {
            return Err(OpenError::Unimplemented);
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "windows",
            target_os = "macos",
            target_os = "ios"
        )))]
        {
            compile_error!("unsupported OS")
        }
    };

    // Ignore anything from stdout. This must be set before spawning the
    // process.
    exe.stdout(std::process::Stdio::null());
    // Pipe stderr so we can log the stderr from the command. This must be set
    // before spawning the process.
    exe.stderr(std::process::Stdio::piped());

    // In the snap on Linux the launcher exports LD_LIBRARY_PATH pointing at
    // the snap's bundled libraries. Leaking this into child process can can be
    // problematic, so let's drop it from the env
    if build_config::snap() {
        exe.env_remove("LD_LIBRARY_PATH");
    }

    // Spawn the process on our same thread so we can detect failure
    // quickly.
    let mut child = exe.spawn().map_err(OpenError::Spawn)?;

    // Create a thread that handles collecting output and reaping the process.
    // This is done in a separate thread because SOME open implementations block
    // and some do not. It's easier to just spawn a thread to handle this so
    // that we never block.
    let thread = std::thread::spawn(move || openThread(child.stderr.take(), child));
    drop(thread);
    Ok(())
}

#[allow(non_snake_case)]
fn openThread(stderr: Option<std::process::ChildStderr>, exe_: std::process::Child) {
    use std::io::BufRead;

    // Copy the exe so it is non-const. This is necessary because wait()
    // requires a mutable reference and we can't have one as a thread
    // param.
    let mut exe = exe_;
    if let Some(stderr) = stderr {
        let mut reader = std::io::BufReader::new(stderr);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    // Trim the trailing '\n' to match `takeDelimiterExclusive('\n')`.
                    if line.last() == Some(&b'\n') {
                        line.pop();
                    }
                    log_warn(&format!("open stderr={}", String::from_utf8_lossy(&line)));
                }
                Err(_) => break,
            }
        }
    }
    let _ = exe.wait();
}

fn log_warn(msg: &str) {
    eprintln!("[warn] [{LOG_SCOPE}] {msg}");
}
