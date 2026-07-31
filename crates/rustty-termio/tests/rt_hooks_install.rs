//! V13 — the `rt_*` apprt-hook INSTALL REGISTRY (`rustty_termio::command`).
//!
//! WHY AN INTEGRATION TEST (its own process): `install_rt_hooks` writes a
//! PROCESS-GLOBAL — the runtime stand-in for the reference's comptime
//! `apprt.runtime` selection (`Command.zig:139-155`, `Exec.zig:1034-1037`; see
//! the seam PORT NOTE above `command::RtPreExecInfo`). Its contract is
//! order-dependent ("nothing installed" -> "installed" -> "a DIFFERENT install
//! panics"), and the unit-test binary already installs a pair of its own in the
//! `rt_hooks_reach_the_spawned_command` KAT. A separate binary gives this file a
//! virgin global; the single `#[test]` below then walks the whole contract in a
//! fixed order.

use rustty_termio::command::{
    install_rt_hooks, rt_hooks, Command, PostForkError, PreExecFn, RtHooks,
};

fn hook_a_pre(_cmd: &mut Command<'_>) -> Option<u8> {
    None
}
fn hook_a_post(_cmd: &mut Command<'_>) -> Result<(), PostForkError> {
    Ok(())
}
fn hook_b_pre(_cmd: &mut Command<'_>) -> Option<u8> {
    Some(127)
}

#[test]
fn install_rt_hooks_contract() {
    // 1. Nothing installed == the reference's non-GTK apprt: both `@hasDecl`
    //    branches yield `null` (`Exec.zig:1034,1036`).
    assert!(rt_hooks().pre_exec.is_none());
    assert!(rt_hooks().post_fork.is_none());

    // 2. The apprt installs its pair (what `gtk::App::init` does).
    let a = RtHooks {
        pre_exec: Some(hook_a_pre as PreExecFn),
        post_fork: Some(hook_a_post as _),
    };
    install_rt_hooks(a);
    let got = rt_hooks();
    assert!(
        core::ptr::fn_addr_eq(
            got.pre_exec.expect("pre_exec installed"),
            hook_a_pre as PreExecFn
        ),
        "rt_hooks() returns the installed pre_exec"
    );
    assert!(
        got.post_fork.is_some(),
        "rt_hooks() returns the installed post_fork"
    );

    // 3. Re-installing the SAME pair is a no-op, not a panic (an apprt whose
    //    init runs twice must stay fine).
    install_rt_hooks(a);
    assert!(rt_hooks().pre_exec.is_some());

    // 4. THE FAIL ARM: a DIFFERENT pair is a hard error. The reference's choice
    //    is a build-time constant, so "two apprts in one process" is a state it
    //    cannot represent.
    let b = RtHooks {
        pre_exec: Some(hook_b_pre as PreExecFn),
        post_fork: Some(hook_a_post as _),
    };
    let panicked = std::panic::catch_unwind(|| install_rt_hooks(b)).is_err();
    assert!(panicked, "installing a different apprt's hooks must panic");

    // 5. ...and the panic left the first install intact.
    assert!(
        core::ptr::fn_addr_eq(
            rt_hooks().pre_exec.expect("still installed"),
            hook_a_pre as PreExecFn
        ),
        "a rejected install must not have overwritten the first one"
    );
}
