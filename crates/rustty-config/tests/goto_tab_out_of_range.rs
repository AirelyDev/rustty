//! Task R13-B (`UPSTREAM-DEFECTS.md` UD58), through the config loader.
//!
//! `goto_tab` carries a `usize`, but the surface narrows it into
//! `apprt.action.GotoTab = enum(c_int)`, whose -1/-2/-3 are the
//! previous/next/last sentinels. The reference traps (ReleaseSafe) or truncates
//! (ReleaseFast) above `c_int` max. The port rejects the value where the config
//! parses it, as it rejects any other integer argument its type cannot hold, so
//! the user gets a config diagnostic instead.

use rustty_config::Config;

fn diagnostics(text: &[u8]) -> Vec<(String, String)> {
    let mut cfg = Config::default_config();
    let mut iter = rustty_args::args::LineIterator::init(text);
    cfg.load_iter(&mut iter).unwrap();
    cfg._diagnostics
        .items()
        .iter()
        .map(|d| {
            (
                String::from_utf8_lossy(&d.key).into_owned(),
                String::from_utf8_lossy(&d.message).into_owned(),
            )
        })
        .collect()
}

#[test]
fn goto_tab_index_above_c_int_max_is_a_config_diagnostic() {
    // Positive control: the largest index that fits is accepted by both keys.
    assert_eq!(
        Vec::<(String, String)>::new(),
        diagnostics(
            b"keybind = ctrl+a=goto_tab:2147483647\n\
              command-palette-entry = title:Tab,action:goto_tab:2147483647\n"
        )
    );

    // 4294967295 is the value the reference's ReleaseFast truncates onto
    // `.previous`; 2147483648 is the first value that does not fit.
    for v in ["2147483648", "4294967295"] {
        let keybind = diagnostics(format!("keybind = ctrl+a=goto_tab:{v}\n").as_bytes());
        assert_eq!(1, keybind.len(), "keybind goto_tab:{v}: {keybind:?}");
        assert_eq!("keybind", keybind[0].0);

        let entry = diagnostics(
            format!("command-palette-entry = title:Tab,action:goto_tab:{v}\n").as_bytes(),
        );
        assert_eq!(
            1,
            entry.len(),
            "command-palette-entry goto_tab:{v}: {entry:?}"
        );
        assert_eq!("command-palette-entry", entry[0].0);
    }

    // The same diagnostic an integer argument out of range for its own type
    // gets (`jump_to_prompt` is an `i16`).
    let jump = diagnostics(b"keybind = ctrl+a=jump_to_prompt:70000\n");
    let goto = diagnostics(b"keybind = ctrl+a=goto_tab:4294967295\n");
    assert_eq!(1, jump.len(), "{jump:?}");
    assert_eq!(
        jump[0].1.replace("jump_to_prompt:70000", "<action>"),
        goto[0].1.replace("goto_tab:4294967295", "<action>"),
    );
}
