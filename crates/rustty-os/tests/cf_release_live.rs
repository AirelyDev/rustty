//! W0 live-loop KAT (imitates rustty-termio/tests/live_shell.rs): the cf_release
//! Thread runs its REAL rustty-xev kqueue loop; a Release message drains and
//! CFReleases a CFString. Real mach-port/kqueue syscalls -> Miri-excluded.
#![cfg(target_os = "macos")]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::thread;
use std::time::Duration;

use objc2_core_foundation::{CFGetRetainCount, CFRetained, CFString};
use rustty_datastruct::blocking_queue::Timeout;
use rustty_os::CFReleaseThread::{Message, Thread};

#[test]
fn cf_release_thread_releases_on_real_xev_loop() {
    // Build the thread on the real xev loop (Loop::init + two Async::init).
    let mut t = Thread::init().expect("Thread::init on real xev");

    // Copy the mach-port Async handles out (Async is Copy; u32 port => Send) so a
    // notifier thread can wake + stop the loop while threadMain runs here.
    let wakeup = t.wakeup;
    let stop = t.stop;

    // A heap CFString (retain count 1). Clone -> raw +1 handed to the thread; the
    // original `kept` still holds +1. After the thread CFReleases the raw ref, the
    // count drops from 2 back to 1 => proves drainMailbox -> CFRelease fired.
    let kept: CFRetained<CFString> = CFString::from_str("rustty-cf-release-kat");
    let raw: *mut c_void = CFRetained::into_raw(kept.clone()).as_ptr().cast();
    assert_eq!(CFGetRetainCount(Some(kept.as_ref())), 2);

    // Publish the Release message BEFORE the loop drains it. The Vec<*mut c_void>
    // never crosses a thread boundary (push + drain both run on THIS thread).
    t.mailbox
        .push(Message::Release { refs: vec![raw] }, Timeout::Instant);

    // Notifier thread: wake the loop (drain -> CFRelease), then stop it so
    // run(.until_done) returns. Async::notify coalesces + the qlimit-1 mach port
    // holds a pending notify even if it races threadMain arming the wait.
    let notifier = thread::spawn(move || {
        wakeup.notify().expect("wakeup.notify");
        thread::sleep(Duration::from_millis(50));
        stop.notify().expect("stop.notify");
    });

    // Run the REAL loop until stop. If the loop were the old unimplemented!()
    // placeholder this would panic; a clean return proves the live kqueue ran.
    t.threadMain();
    notifier.join().expect("notifier join");

    // The +1 handed to the thread was CFReleased => count back to 1.
    assert_eq!(CFGetRetainCount(Some(kept.as_ref())), 1);

    // Clean up the thread (stop/wakeup/loop deinit + mailbox destroy).
    t.deinit();
    drop(kept);
}
