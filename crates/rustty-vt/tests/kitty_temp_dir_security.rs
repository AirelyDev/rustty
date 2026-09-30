//! `P3-W11 T1` (`P3-W6 T4` Step 5) — the security negative control for the
//! kitty temp-file medium.
//!
//! ORACLE: `e8525c0fd`(iii) removed `isPathInTempDir`'s
//! `temp_dir.allocTmpDir()` call — a read of the process `TMPDIR`/`TMP`
//! environment at *validation* time — in favour of an explicit directory
//! threaded down from `Limits.temporary_file.enabled.directory`.
//!
//! WHY THIS IS ITS OWN TEST BINARY: it mutates the process environment
//! (`TMPDIR`). `cargo test` runs the tests inside one binary on many threads,
//! and `std::env::set_var` is process-global; a file containing exactly one
//! `#[test]` is the only way to make that sound.
//!
//! WHY NOT `rustty_os::tempdir::TempDir`: `TempDir::init` creates its directory
//! *inside the process temp dir*, which on Linux is `/tmp`. `isPathInTempDir`
//! accepts anything under `/tmp` unconditionally (logic copied from kitty), so
//! a `TempDir`-based attacker path would be accepted for a reason that has
//! nothing to do with the code under test — the control would be **vacuous on
//! Linux** while passing on macOS (where the temp dir is `/var/folders/...`).
//! Both directories below are therefore created under `CARGO_TARGET_TMPDIR`,
//! which lives under `target/` and starts with neither `/tmp` nor `/dev/shm`.

use rustty_vt::kitty::graphics_command as command;
use rustty_vt::kitty::graphics_image::{Error, Limits, LoadingImage};

const IMAGE_DATA: &[u8] =
    include_bytes!("../src/kitty/testdata/image-rgb-none-20x15-2147483647-raw.data");

const FILE_NAME: &str = "tty-graphics-protocol-image.data";

#[cfg(not(windows))]
fn path_bytes(p: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn path_bytes(p: &std::path::Path) -> Vec<u8> {
    p.to_string_lossy().into_owned().into_bytes()
}

fn transmit_cmd(path: &std::path::Path) -> command::Command {
    command::Command {
        control: command::Control::Transmit(command::Transmission {
            format: command::Format::Rgb,
            medium: command::Medium::TemporaryFile,
            compression: command::Compression::None,
            width: 20,
            height: 15,
            image_id: 31,
            ..Default::default()
        }),
        quiet: command::Quiet::No,
        data: rustty_vt::alloc::heap_bytes(&path_bytes(path)),
    }
}

/// Create `<CARGO_TARGET_TMPDIR>/<name>` fresh and return its canonical path.
fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The DOS form on Windows (`C:\..`), as the reference's `realPath` gives
    // it: `std::fs::canonicalize`'s verbatim `\\?\C:\..` is a device-namespace
    // spelling that `readFile` refuses before the open since `0b5446364`
    // (measured on rwin, P7-W5 T2).
    rustty_os::path::realpath(&dir).unwrap()
}

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt, parity tail C-1): file-system calls on a real temporary directory are forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn kitty_temp_file_medium_ignores_the_process_tmpdir() {
    // REBUILD SENTINEL: this assert exists so that a mutation of the code under
    // test which is *not* rebuilt can be detected. See the lane report — it was
    // verified to fire when tripped.
    assert_eq!(2 + 2, 4, "REBUILD SENTINEL");

    let allowed = scratch_dir("p3w11-kitty-allowed");
    let attacker = scratch_dir("p3w11-kitty-attacker");

    // Sanity: neither scratch directory may be reachable through the two
    // unconditional kitty prefixes, or the control below proves nothing.
    for d in [&allowed, &attacker] {
        let b = path_bytes(d);
        assert!(
            !b.starts_with(b"/tmp") && !b.starts_with(b"/dev/shm"),
            "VACUOUS CONTROL: {:?} is under an unconditionally-accepted prefix",
            d
        );
    }

    let attacker_file = attacker.join(FILE_NAME);
    std::fs::write(&attacker_file, IMAGE_DATA).unwrap();
    let allowed_file = allowed.join(FILE_NAME);
    std::fs::write(&allowed_file, IMAGE_DATA).unwrap();

    // Named so this buffer outlives every later use of `limits` below:
    // `Limits::all_with_temp_dir` (P7-W2 T3, oracle `105d0a545`) borrows its
    // argument instead of copying it (see the `TemporaryFileLimit` PORT
    // NOTE), matching upstream's own zero-copy slice.
    let allowed_bytes = path_bytes(&allowed);
    let limits = Limits::all_with_temp_dir(&allowed_bytes);

    // Point the process temp dir AT THE ATTACKER'S DIRECTORY. Before
    // `e8525c0fd` this is exactly what `isPathInTempDir` consulted, so the
    // transmission below would have been ACCEPTED.
    //
    // SAFETY: this test file contains a single `#[test]`, so it is the only
    // thread in its binary that can observe the environment.
    unsafe {
        std::env::set_var("TMPDIR", &attacker);
        std::env::set_var("TMP", &attacker);
    }

    // The attacker's file is outside the configured directory: REJECTED.
    assert_eq!(
        Err(Error::TemporaryFileNotInTempDir),
        LoadingImage::init(
            rustty_vt::alloc::Global,
            &transmit_cmd(&attacker_file),
            &limits
        )
        .map(|_| ()),
        "a temp-file transmission outside the configured directory must be \
         rejected even when the process TMPDIR names that directory"
    );

    // And the rejection happens BEFORE the unlink guard is armed, so the file
    // is untouched.
    assert!(
        attacker_file.exists(),
        "rejected transmission must not delete the file"
    );

    // The positive arm: a file inside the configured directory still loads,
    // and is unlinked on success (the `defer posix.unlink` behaviour).
    let mut loading = LoadingImage::init(
        rustty_vt::alloc::Global,
        &transmit_cmd(&allowed_file),
        &limits,
    )
    .expect("in-directory transmission must be accepted");
    let img = loading
        .complete(rustty_vt::alloc::Global)
        .expect("complete");
    assert_eq!(command::Compression::None, img.compression);
    assert!(
        !allowed_file.exists(),
        "an accepted temporary-file transmission unlinks the file"
    );

    // Cross-check the converse: with the directory configured to the
    // ATTACKER's dir, that same path is accepted — proving the rejection above
    // was caused by the configured directory and not by some unrelated
    // property of the path (a self-check against a vacuous control).
    std::fs::write(&attacker_file, IMAGE_DATA).unwrap();
    let attacker_bytes = path_bytes(&attacker);
    let attacker_limits = Limits::all_with_temp_dir(&attacker_bytes);
    assert!(
        LoadingImage::init(
            rustty_vt::alloc::Global,
            &transmit_cmd(&attacker_file),
            &attacker_limits
        )
        .is_ok(),
        "the same path must be accepted when the configured directory allows it"
    );
}
