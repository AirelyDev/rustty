//! `GhosttyBench.zig:16-31` names the synthetic data generator executable
//! `ghostty-gen`; per the naming transform the port's is `rustty-gen`, the
//! name the bench docs tell users to run (S63-F12).
//!
//! `CARGO_BIN_EXE_rustty-gen` exists only when this package builds a binary
//! with exactly that name, so the test does not compile without it. Running
//! the binary then proves it is the generator (`main_gen.rs`), not some other
//! target that happens to carry the name.

use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn the_generator_binary_is_named_rustty_gen() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustty-gen"))
        .arg("+ascii")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn rustty-gen");
    let mut buf = [0u8; 64];
    child
        .stdout
        .as_mut()
        .expect("piped stdout")
        .read_exact(&mut buf)
        .expect("rustty-gen +ascii writes a stream");
    let _ = child.kill();
    let _ = child.wait();
    // `+ascii` draws from printable ASCII minus space, plus line breaks.
    assert!(
        buf.iter()
            .all(|&b| (0x21..0x7f).contains(&b) || b == b'\n' || b == b'\r'),
        "{buf:?}"
    );
}
