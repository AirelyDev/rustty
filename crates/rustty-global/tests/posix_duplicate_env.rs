//! N381 (task W-5b): a process whose `environ` holds two entries with the same
//! name. Zig 0.16's `getPosix` returns the FIRST (`std/process/Environ.zig:606-619`),
//! and `createMap` `put`s every entry (`:184-196`, `:256-271`), so the map, and
//! `getAlloc`/`containsUnempty` through it, see the LAST.
//!
//! The parent re-runs this test binary with `posix_spawn` and an `envp` of
//! exactly `[RUSTTY_N381_CHILD=1, K=first, E=, K=second, E=later]`:
//! `std::process::Command` cannot produce it, because its environment is a map
//! and drops the duplicate. The child arm asserts on the real process
//! `environ` (`Environ::from_process` / the global `environ_map()`).
//!
//! Its own test binary: the child arm reads the process-global environment.
#![cfg(unix)]

use std::ffi::CString;

const CHILD: &str = "posix_duplicate_env_child_reads_first_and_last";

#[test]
#[ignore = "child arm of posix_duplicate_env_parent_spawns_the_child; selected by argv"]
fn posix_duplicate_env_child_reads_first_and_last() {
    assert_eq!(
        std::env::var_os("RUSTTY_N381_CHILD").as_deref(),
        Some(std::ffi::OsStr::new("1")),
        "run only by posix_duplicate_env_parent_spawns_the_child (it sets the duplicate envp)"
    );
    let e = rustty_global::Environ::from_process();
    // The child really has both entries (precondition, else the checks below
    // prove nothing).
    let ks: Vec<&[u8]> = e
        .entries()
        .iter()
        .filter(|x| x.starts_with(b"K="))
        .map(Vec::as_slice)
        .collect();
    assert_eq!(ks, [&b"K=first"[..], &b"K=second"[..]], "duplicate envp");

    assert_eq!(e.get_posix(b"K"), Some(&b"first"[..]), "getPosix: first");
    assert!(e.contains_unempty_constant(b"K"));
    assert!(
        !e.contains_unempty_constant(b"E"),
        "containsUnemptyConstant: first"
    );

    assert_eq!(
        e.get_alloc(b"K"),
        Some(b"second".to_vec()),
        "getAlloc: last"
    );
    assert_eq!(
        e.create_map().get(b"K").map(Vec::as_slice),
        Some(&b"second"[..]),
        "createMap: last"
    );
    assert!(e.contains_unempty(b"E"), "containsUnempty: last");
    assert_eq!(
        rustty_global::environ_map().get(b"K").map(Vec::as_slice),
        Some(&b"second"[..]),
        "environMap: last"
    );
}

#[test]
fn posix_duplicate_env_parent_spawns_the_child() {
    let exe = std::env::current_exe().expect("current_exe");
    let argv_owned: Vec<CString> = [
        exe.to_str().expect("utf-8 test path"),
        CHILD,
        "--exact",
        "--ignored",
        "--nocapture",
    ]
    .iter()
    .map(|s| CString::new(*s).unwrap())
    .collect();
    let envp_owned: Vec<CString> = [
        "RUSTTY_N381_CHILD=1",
        "K=first",
        "E=",
        "K=second",
        "E=later",
    ]
    .iter()
    .map(|s| CString::new(*s).unwrap())
    .collect();
    let mut argv: Vec<*mut libc::c_char> =
        argv_owned.iter().map(|c| c.as_ptr() as *mut _).collect();
    argv.push(std::ptr::null_mut());
    let mut envp: Vec<*mut libc::c_char> =
        envp_owned.iter().map(|c| c.as_ptr() as *mut _).collect();
    envp.push(std::ptr::null_mut());

    // The child's stdout goes to a pipe the parent reads, so the parent can
    // tell that the child arm actually ran.
    let mut fds = [0 as libc::c_int; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe");
    let (rd, wr) = (fds[0], fds[1]);

    let mut pid: libc::pid_t = 0;
    let rc = unsafe {
        let mut fa: libc::posix_spawn_file_actions_t = std::mem::zeroed();
        assert_eq!(libc::posix_spawn_file_actions_init(&mut fa), 0);
        assert_eq!(libc::posix_spawn_file_actions_adddup2(&mut fa, wr, 1), 0);
        assert_eq!(libc::posix_spawn_file_actions_adddup2(&mut fa, wr, 2), 0);
        assert_eq!(libc::posix_spawn_file_actions_addclose(&mut fa, rd), 0);
        let rc = libc::posix_spawn(
            &mut pid,
            argv_owned[0].as_ptr(),
            &fa,
            std::ptr::null(),
            argv.as_ptr(),
            envp.as_ptr(),
        );
        libc::posix_spawn_file_actions_destroy(&mut fa);
        rc
    };
    unsafe { libc::close(wr) };
    assert_eq!(rc, 0, "posix_spawn");

    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = unsafe { libc::read(rd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    unsafe { libc::close(rd) };
    let mut status: libc::c_int = 0;
    assert_eq!(
        unsafe { libc::waitpid(pid, &mut status, 0) },
        pid,
        "waitpid"
    );
    let out = String::from_utf8_lossy(&out);

    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "the child arm failed:\n{out}"
    );
    assert!(
        out.contains("1 passed"),
        "the child arm did not run (vacuous):\n{out}"
    );
}
