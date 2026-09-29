//! PORTS: ghostty `pkg/opengl/glad.zig` (44 loc) — the loaded OpenGL function
//! table every `pkg/opengl` wrapper calls through (`glad.context.<Fn>.?(…)`).
//!
//! ```zig
//! pub const Context = c.GladGLContext;
//!
//! /// This is the current context. Set this var manually prior to calling
//! /// any of this package's functions. I know its nasty to have a global but
//! /// this makes it match OpenGL API styles where it also operates on a
//! /// threadlocal global.
//! pub threadlocal var context: Context = undefined;
//! ```
//!
//! P7-W7 T1 (DECISIONS B9): the port used to call the `gl` crate's
//! process-global entry points, filled once by `gl::load_with`. Ghostty's glad
//! context is a `threadlocal var`: each render thread loads its own table in
//! `threadEnter` (`OpenGL.zig:222-227`) and drops it in `threadExit`
//! (`OpenGL.zig:232-235`, `gl.glad.unload()`). One global table would be written by
//! every surface's render thread and read by the GTK inspector's widget at the same
//! time, with no `unload` analogue. This module is the per-thread table:
//!
//! - [`Context`] is a `gl_generator` `StructGenerator` binding (`build.rs`'s
//!   `generate_gl_struct_bindings`), one function pointer per GL 4.5 core entry
//!   point — the analogue of glad's `GladGLContext`;
//! - [`load`] fills the CALLING thread's table (`glad.load`), [`unload`] drops it
//!   (`glad.unload`);
//! - [`context`] lends the calling thread's table to a closure. Every GL call site
//!   in `crate::opengl` goes through it, as every reference call site goes through
//!   `glad.context`.
//!
//! The `gl` crate stays a dependency for its constants (`gl::COLOR_BUFFER_BIT`) and
//! type aliases (`gl::types::GLint`), which are identical to the generated ones.
//! Nothing loads its global function table any more, so a stray `gl::Foo(…)` call
//! would panic with "function was not loaded"; `tests::no_global_gl_entry_point_calls`
//! keeps the crate free of them.

use std::cell::RefCell;
use std::ffi::c_void;

/// The generated struct bindings (`$OUT_DIR/gl_struct_bindings.rs`).
///
/// PORT NOTE: the analogue of `pkg/opengl/c.zig`'s glad import, restricted to what
/// this module needs: the `Gl` struct. The generated file's own constants and
/// `types` module duplicate the `gl` crate's and are not used.
#[allow(
    clippy::all,
    unsafe_op_in_unsafe_fn,
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    unused_imports,
    missing_docs
)]
mod c {
    include!(concat!(env!("OUT_DIR"), "/gl_struct_bindings.rs"));
}

/// `pub const Context = c.GladGLContext;`
pub type Context = c::Gl;

thread_local! {
    /// `pub threadlocal var context: Context = undefined;`
    ///
    /// PORT NOTE (`undefined` → `None`): calling through an unloaded glad context
    /// is undefined behaviour in the reference. Here the unloaded state is `None`,
    /// and [`context`] panics on it with a message naming the cause.
    static CONTEXT: RefCell<Option<Box<Context>>> = const { RefCell::new(None) };
}

/// Initialize the calling thread's function table from `get_proc_address`.
///
/// PORTS: `glad.zig:16-33`.
///
/// ```zig
/// /// Initialize Glad. This is guaranteed to succeed if no errors are returned.
/// /// The getProcAddress param is an anytype so that we can accept multiple
/// /// forms of the function depending on what we're interfacing with.
/// pub fn load(getProcAddress: anytype) !c_int {
///     ...
///     if (res == 0) return error.GLInitFailed;
///     return res;
/// }
/// ```
///
/// PORT NOTE: glad's loader returns the GL version it detected, and fails when it
/// cannot read one. `Gl::load_with` does neither: it resolves every symbol it can
/// and leaves the rest unloaded. The version read and its failure live in
/// `sys::prepare_context` (`gl_version` → `Error::VersionUnavailable`), which runs
/// straight after this, so the failure the reference reports here is still
/// reported, one call later. Loading replaces any table this thread already had,
/// as `gladLoadGLContext` overwrites `context`.
pub fn load(get_proc_address: impl FnMut(&'static str) -> *const c_void) {
    drop(load_replacing(get_proc_address));
}

/// [`load`], returning the table this thread had before (if any) so a caller
/// can put it back with [`restore`].
///
/// RUSTTY-ONLY: glad has no such pair. `sys::prepare_context` uses it for its
/// `errdefer`: on the GTK path every surface and the inspector share the main
/// thread's one table, so a failed prep must leave the table it found, not an
/// empty slot (see `prepare_context`).
pub fn load_replacing(
    get_proc_address: impl FnMut(&'static str) -> *const c_void,
) -> Option<Box<Context>> {
    let table = Box::new(Context::load_with(get_proc_address));
    CONTEXT.with_borrow_mut(|slot| slot.replace(table))
}

/// Put back a table [`load_replacing`] returned (`None` unloads).
pub fn restore(previous: Option<Box<Context>>) {
    CONTEXT.with_borrow_mut(|slot| *slot = previous);
}

/// Drop the calling thread's function table.
///
/// PORTS: `glad.zig:35-38`.
///
/// ```zig
/// pub fn unload() void {
///     c.gladLoaderUnloadGLContext(&context);
///     context = undefined;
/// }
/// ```
pub fn unload() {
    CONTEXT.with_borrow_mut(|slot| *slot = None);
}

/// Whether the calling thread has a loaded function table.
///
/// RUSTTY-ONLY: glad has no such query (its unloaded context is `undefined`). It
/// exists so tests can observe [`load`]/[`unload`], and so a caller can check
/// before calling rather than panic.
pub fn is_loaded() -> bool {
    CONTEXT.with_borrow(Option::is_some)
}

/// Lends the calling thread's function table to `f`: the Rust spelling of every
/// `glad.context.<Fn>.?(…)` call in `pkg/opengl`.
///
/// # Panics
///
/// If this thread has no loaded table — the reference's call through an
/// `undefined` context. Every entry point a loaded table failed to resolve panics
/// on call as well (the `gl_generator` stub), which is the reference's `.?`
/// force-unwrap of a null slot.
#[inline]
pub fn context<R>(f: impl FnOnce(&Context) -> R) -> R {
    CONTEXT.with_borrow(|slot| {
        let table = slot.as_deref().unwrap_or_else(|| {
            panic!(
                "no OpenGL function table is loaded on this thread \
                 (glad.load must run on the thread that makes GL calls)"
            )
        });
        f(table)
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::{context, is_loaded, load, unload};

    /// A loader that resolves nothing: every slot stays unloaded, which is all
    /// the table-lifecycle tests need (they never call through it).
    fn null_loader(_: &'static str) -> *const c_void {
        std::ptr::null()
    }

    /// B9's contract: `load` fills the CALLING thread's table only, and `unload`
    /// drops it — glad's `threadlocal var context`. No GL context is needed: the
    /// table's presence is what is under test, not its entry points.
    #[test]
    fn function_table_is_per_thread() {
        assert!(!is_loaded(), "a fresh test thread starts unloaded");
        load(null_loader);
        assert!(is_loaded());

        // Another thread sees no table, even while this one holds one.
        let other = std::thread::spawn(is_loaded).join().expect("join");
        assert!(!other, "a load on one thread must not reach another");

        // A load on another thread does not touch this one's, and dies with it.
        std::thread::spawn(|| {
            load(null_loader);
            assert!(is_loaded());
        })
        .join()
        .expect("join");
        assert!(is_loaded());

        unload();
        assert!(!is_loaded());
    }

    /// `load_replacing` + `restore` put back exactly the table that was there.
    #[test]
    fn restore_puts_back_the_previous_table() {
        load(null_loader);
        let first: *const super::Context = context(|gl| gl as *const _);
        let previous = super::load_replacing(null_loader);
        assert!(previous.is_some());
        super::restore(previous);
        let after: *const super::Context = context(|gl| gl as *const _);
        assert_eq!(first, after, "restore must reinstate the same table");
        unload();

        // From an empty slot, a restore of the returned `None` unloads again.
        let previous = super::load_replacing(null_loader);
        assert!(previous.is_none());
        super::restore(previous);
        assert!(!is_loaded());
    }

    /// An unresolved entry point is reported through the table, as glad's null
    /// slot is: `is_loaded()` false, never a dangling call.
    #[test]
    fn unresolved_entry_points_report_unloaded() {
        load(null_loader);
        let loaded = context(|gl| gl.Clear.is_loaded());
        unload();
        assert!(!loaded);
    }

    /// Calling through an unloaded table panics with a message naming the cause
    /// (the reference's call through an `undefined` context).
    #[test]
    #[should_panic(expected = "no OpenGL function table is loaded on this thread")]
    fn context_panics_when_unloaded() {
        context(|_| ());
    }

    /// Every GL entry-point call in this crate goes through [`context`]. A call
    /// through the `gl` crate's global table (`gl::Foo(…)`) compiles — the crate
    /// is still a dependency for its constants — but that table is never loaded,
    /// so it would panic at run time on whichever path reached it. This scan keeps
    /// the source free of them. `global_call_scan_flags_a_global_call` is its
    /// negative control.
    #[test]
    fn no_global_gl_entry_point_calls() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    let text = std::fs::read_to_string(&path).expect("read source");
                    for (i, line) in global_gl_calls(&text) {
                        offenders.push(format!("{}:{}: {line}", path.display(), i + 1));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "calls through the unloaded global `gl` table:\n{}",
            offenders.join("\n")
        );
    }

    #[test]
    fn global_call_scan_flags_a_global_call() {
        // Spelled with `GL::` and lowered at run time, so this test's own source
        // does not trip the scan above.
        let src = "fn f() {\n    unsafe { GL::Clear(GL::COLOR_BUFFER_BIT) };\n}\n\
                   // GL::Finish() in a comment is fine\n\
                   fn g() { let _ = GL::types::GLint::default(); gl.Clear(0); }\n\
                   fn h() { GL::DebugMessageCallback::is_loaded(); }\n"
            .replace("GL::", "gl::");
        let hits: Vec<usize> = global_gl_calls(&src).into_iter().map(|(i, _)| i).collect();
        assert_eq!(hits, vec![1, 5]);
    }

    /// The `(line index, line)` pairs of `text` that call a `gl::` entry point
    /// (`gl::` + an upper-case name + `(`, or `::is_loaded`), skipping `//` comment
    /// lines and the text after a `//` on a code line.
    fn global_gl_calls(text: &str) -> Vec<(usize, &str)> {
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let code = match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            };
            let bytes = code.as_bytes();
            let mut from = 0;
            while let Some(at) = code[from..].find("gl::") {
                let start = from + at;
                let preceded_by_ident = start > 0
                    && (bytes[start - 1].is_ascii_alphanumeric()
                        || bytes[start - 1] == b'_'
                        || bytes[start - 1] == b':');
                let rest = &code[start + 4..];
                let name_len = rest
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    .count();
                let name = &rest[..name_len];
                let after = &rest[name_len..];
                let is_call = name.starts_with(|c: char| c.is_ascii_uppercase())
                    && name.chars().any(|c| c.is_ascii_lowercase())
                    && (after.starts_with('(') || after.starts_with("::is_loaded"));
                if !preceded_by_ident && is_call {
                    out.push((i, line));
                    break;
                }
                from = start + 4;
            }
        }
        out
    }
}
