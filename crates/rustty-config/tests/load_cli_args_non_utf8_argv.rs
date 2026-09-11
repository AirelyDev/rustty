//! Task R13-B, NEW-FINDINGS N45: a non-UTF-8 command-line argument must not
//! panic `Config::load_cli_args` on Linux and FreeBSD.
//!
//! The xdg-terminal-exec check there used to build its argv with
//! `std::env::args()`, which panics on the first non-UTF-8 element. It ran
//! whatever argv[0] was, so `rustty -e cat $'\xff'` aborted at startup.
//!
//! WHY `harness = false`: libtest's own `main` calls `std::env::args()` before
//! any test runs, so a libtest binary cannot be started with a non-UTF-8 argv
//! at all (observed and recorded in `rustty-args/tests/os_args_seam.rs`). This
//! binary has its own `main`: it re-executes itself with the argv under test,
//! and the child reports what `load_cli_args` built.

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn main() {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::process::CommandExt;

    const CHILD: &str = "RUSTTY_R13B_LOAD_CLI_ARGS_CHILD";

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    if std::env::var_os(CHILD).is_some() {
        let mut cfg = rustty_config::Config::default_config();
        cfg.load_cli_args().expect("load_cli_args");
        let args = match &cfg.initial_command {
            Some(rustty_config::command::Command::Direct(args)) => args.clone(),
            other => panic!("expected a direct initial command, got {other:?}"),
        };
        let args: Vec<String> = args.iter().map(|a| hex(a)).collect();
        println!(
            "xdg-terminal-exec={} initial-command={}",
            cfg._xdg_terminal_exec,
            args.join(",")
        );
        return;
    }

    let exe = std::env::current_exe().expect("current_exe");
    let bad = OsString::from_vec(vec![0xff, 0xfe]);
    assert!(
        std::str::from_utf8(bad.as_bytes()).is_err(),
        "the argument must be non-UTF-8 or this test is vacuous"
    );

    // (argv[0], the remaining arguments, the child's expected report)
    let cases: [(&str, Vec<OsString>, &str); 2] = [
        (
            "xdg-terminal-exec",
            vec!["cat".into(), bad.clone()],
            "xdg-terminal-exec=true initial-command=636174,fffe",
        ),
        (
            "rustty",
            vec!["-e".into(), "cat".into(), bad.clone()],
            "xdg-terminal-exec=false initial-command=636174,fffe",
        ),
    ];

    for (argv0, args, expected) in cases {
        let out = std::process::Command::new(&exe)
            .arg0(argv0)
            .args(&args)
            .env(CHILD, "1")
            .output()
            .expect("spawn the child");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "argv[0]={argv0}: load_cli_args failed in the child ({:?})\n\
             stdout: {stdout}\nstderr: {stderr}",
            out.status
        );
        assert_eq!(
            stdout.lines().last().unwrap_or(""),
            expected,
            "argv[0]={argv0}: the child built the wrong command\nstderr: {stderr}"
        );
    }

    println!("load_cli_args_non_utf8_argv: 2 cases passed");
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn main() {
    // The xdg-terminal-exec check is Linux/FreeBSD-only code; elsewhere
    // `load_cli_args` reads argv through the byte seam only.
}
