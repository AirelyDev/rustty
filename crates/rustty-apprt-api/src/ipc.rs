//! Inter-process Communication to a running Ghostty instance from a separate
//! process.
//!
//! PORTS: ghostty `src/apprt/ipc.zig`.
//!
//! PORT NOTE (acronyms): `Errors::IPCFailed` keeps the reference name
//! (identity/parity); the module allow silences clippy's `upper_case_acronyms`.
#![allow(clippy::upper_case_acronyms)]

use core::ptr::NonNull;

use allocator_api2::alloc::{AllocError, Allocator, Layout};

pub enum Errors {
    /// The IPC failed. If a function returns this error, it's expected that
    /// an a more specific error message will have been written to stderr (or
    /// otherwise shown to the user in an appropriate way).
    IPCFailed,
}

pub enum Target<'a> {
    /// Open up a new window in a custom instance of Ghostty.
    //
    // PORT NOTE: reference `class: [:0]const u8` -> borrowed raw bytes `&'a [u8]`.
    Class(&'a [u8]),

    /// Detect which instance to open a new window in.
    Detect,
}

// Sync with: rustty_ipc_target_tag_e
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum TargetKey {
    Class,
    Detect,
}

// Sync with: rustty_ipc_target_u
#[allow(dead_code)]
#[repr(C)]
pub union TargetCValue {
    // PORT NOTE: reference `[*:0]const u8` -> `*const u8` (sentinel guarantee is
    // part of the deferred C-ABI pinning).
    pub class: *const u8,
    pub detect: (),
}

// Sync with: rustty_ipc_target_s
#[repr(C)]
pub struct TargetC {
    pub key: TargetKey,
    pub value: TargetCValue,
}

impl<'a> Target<'a> {
    /// The active tag (`@as(Key, self)`).
    pub fn key(&self) -> TargetKey {
        match self {
            Target::Class(_) => TargetKey::Class,
            Target::Detect => TargetKey::Detect,
        }
    }

    /// Convert to rustty_ipc_target_s.
    pub fn cval(&self) -> TargetC {
        TargetC {
            key: self.key(),
            value: match self {
                Target::Class(class) => TargetCValue {
                    class: class.as_ptr(),
                },
                Target::Detect => TargetCValue { detect: () },
            },
        }
    }
}

pub enum Action<'a> {
    // A GUIDE TO ADDING NEW ACTIONS:
    //
    // 1. Add the action to the `Key` enum. The order of the enum matters
    //    because it maps directly to the libghostty C enum. For ABI
    //    compatibility, new actions should be added to the end of the enum.
    //
    // 2. Add the action and optional value to the Action union.
    //
    // 3. If the value type is not void, ensure the value is C ABI
    //    compatible (extern). If it is not, add a `C` decl to the value
    //    and a `cval` function to convert to the C ABI compatible value.
    //
    // 4. Update `include/rustty.h`: add the new key, value, and union
    //    entry. If the value type is void then only the key needs to be
    //    added. Ensure the order matches exactly with the Zig code.
    /// Create a new window.
    NewWindow(NewWindow<'a>),

    /// Create a new tab.
    NewTab(NewTab<'a>),

    /// Toggle the quick terminal.
    ToggleQuickTerminal,
}

/// PORT NOTE: reference `arguments: ?[][:0]const u8` -> borrowed
/// `Option<&'a [&'a [u8]]>` (a borrowed slice of borrowed raw-byte args).
pub struct NewWindow<'a> {
    /// A list of configuration entries that will override the configuration
    /// of the first surface of the new window. If this is `null` there
    /// are no overrides. Note that not all configuration entries may be
    /// overridden.
    ///
    /// It is an error for this to be non-`null`, but zero length.
    pub arguments: Option<&'a [&'a [u8]]>,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct NewWindowC {
    /// A list of configuration entries that will override the
    /// configuration of the first surface of the new tab. If this is
    /// `null` there are no overrides. Note that not all configuration
    /// entries may be overridden.
    ///
    /// It is an error for this to be non-`null`, but zero length.
    //
    // PORT NOTE: reference `?[*]?[*:0]const u8` -> `*mut *const u8` (null array
    // pointer == None; null element == the NUL terminator / a None argument).
    // (The "new tab" wording above is the reference's own text, verbatim —
    // `ipc.zig` `NewWindow.C` says "new tab"; R1 keeps it.)
    pub arguments: *mut *const u8,
}

impl NewWindowC {
    /// # Safety
    /// `alloc` must be the same allocator that produced `self.arguments` (via
    /// [`NewWindow::cval`]).
    pub unsafe fn deinit<A: Allocator>(&mut self, alloc: A) {
        // PORT NOTE: reference `if (self.arguments) |arguments| alloc.free(arguments);`.
        // Zig frees a length-tracked slice; Rust `deallocate` needs the `Layout`,
        // so the element count is recovered from the NUL terminator that `cval`
        // writes (exactly what a C consumer would scan for).
        if let Some(base) = NonNull::new(self.arguments) {
            let mut n = 0usize;
            // SAFETY: `cval` NUL-terminates the array.
            while unsafe { !(*base.as_ptr().add(n)).is_null() } {
                n += 1;
            }
            n += 1; // include the NUL terminator slot
            let layout = Layout::array::<*const u8>(n).unwrap();
            // SAFETY: `base`/`layout` match the `cval` allocation.
            unsafe { alloc.deallocate(base.cast::<u8>(), layout) };
        }
    }
}

impl<'a> NewWindow<'a> {
    pub fn cval<A: Allocator>(&self, alloc: A) -> Result<NewWindowC, AllocError> {
        match self.arguments {
            Some(arguments) => {
                let n = arguments.len() + 1;
                let layout = Layout::array::<*const u8>(n).map_err(|_| AllocError)?;
                let ptr = alloc.allocate(layout)?;
                let base = ptr.cast::<*const u8>().as_ptr();
                // SAFETY: `base` points to `n` freshly-allocated, aligned,
                // uninitialized `*const u8` slots.
                unsafe {
                    for (i, argument) in arguments.iter().enumerate() {
                        base.add(i).write(argument.as_ptr());
                    }
                    // add null terminator
                    base.add(arguments.len()).write(core::ptr::null());
                }
                Ok(NewWindowC { arguments: base })
            }
            None => Ok(NewWindowC {
                arguments: core::ptr::null_mut(),
            }),
        }
    }
}

/// PORT NOTE: reference `arguments: ?[][:0]const u8` -> borrowed
/// `Option<&'a [&'a [u8]]>` (same treatment as [`NewWindow`]).
pub struct NewTab<'a> {
    /// The unique ID of a surface. If the ID is zero the currently focused
    /// surface is assumed. This will be used to identify the window to add
    /// the tab to.
    pub surface_id: u64,

    /// A list of configuration entries that will override the configuration
    /// of the first surface of the new tab. If this is `null` there
    /// are no overrides. Note that not all configuration entries may be
    /// overridden.
    ///
    /// It is an error for this to be non-`null`, but zero length.
    pub arguments: Option<&'a [&'a [u8]]>,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct NewTabC {
    /// The unique ID of a surface. If the ID is zero the currently
    /// focused surface is assumed. This will be used to identify the
    /// window to add the tab to.
    pub surface_id: u64,
    /// A list of configuration entries that will override the
    /// configuration of the first surface of the new tab. If this is
    /// `null` there are no overrides. Note that not all configuration
    /// entries may be overridden.
    ///
    /// It is an error for this to be non-`null`, but zero length.
    //
    // PORT NOTE: reference `?[*]?[*:0]const u8` -> `*mut *const u8` (null array
    // pointer == None; null element == the NUL terminator / a None argument).
    pub arguments: *mut *const u8,
}

impl NewTabC {
    /// # Safety
    /// `alloc` must be the same allocator that produced `self.arguments` (via
    /// [`NewTab::cval`]).
    //
    // UPSTREAM QUIRK (034506f14, ipc.zig `NewTab.C`): the reference declares
    // this as `pub fn deinit(self: *NewWindow.C, alloc: Allocator)` — a
    // copy-paste of the `NewWindow.C` method that names the WRONG receiver
    // type. It compiles upstream only because Zig analyzes functions lazily
    // and nothing ever calls `NewTab.C.deinit` (or `Action.cval` at all — the
    // apprts consume the Zig union directly). Rust compiles every item, so
    // this is the same body with the correct receiver. Recorded in
    // `.port/phase-4/UPSTREAM_BUGS.md`.
    pub unsafe fn deinit<A: Allocator>(&mut self, alloc: A) {
        // PORT NOTE: reference `if (self.arguments) |arguments| alloc.free(arguments);`
        // (see `NewWindowC::deinit` for the length-recovery rationale).
        if let Some(base) = NonNull::new(self.arguments) {
            let mut n = 0usize;
            // SAFETY: `cval` NUL-terminates the array.
            while unsafe { !(*base.as_ptr().add(n)).is_null() } {
                n += 1;
            }
            n += 1; // include the NUL terminator slot
            let layout = Layout::array::<*const u8>(n).unwrap();
            // SAFETY: `base`/`layout` match the `cval` allocation.
            unsafe { alloc.deallocate(base.cast::<u8>(), layout) };
        }
    }
}

impl<'a> NewTab<'a> {
    // UPSTREAM QUIRK (034506f14, ipc.zig `NewTab.cval`): the reference's
    // signature is `pub fn cval(self: *NewWindow, alloc) !NewWindow.C` — the
    // `NewWindow` copy-paste again, and the body never writes `surface_id`.
    // Dead comptime code upstream (never instantiated); Rust must compile it,
    // so this is the evidently-intended conversion: the same argument-array
    // marshalling PLUS the `surface_id` passthrough the C struct declares.
    // Recorded in `.port/phase-4/UPSTREAM_BUGS.md`.
    pub fn cval<A: Allocator>(&self, alloc: A) -> Result<NewTabC, AllocError> {
        match self.arguments {
            Some(arguments) => {
                let n = arguments.len() + 1;
                let layout = Layout::array::<*const u8>(n).map_err(|_| AllocError)?;
                let ptr = alloc.allocate(layout)?;
                let base = ptr.cast::<*const u8>().as_ptr();
                // SAFETY: `base` points to `n` freshly-allocated, aligned,
                // uninitialized `*const u8` slots.
                unsafe {
                    for (i, argument) in arguments.iter().enumerate() {
                        base.add(i).write(argument.as_ptr());
                    }
                    // add null terminator
                    base.add(arguments.len()).write(core::ptr::null());
                }
                Ok(NewTabC {
                    surface_id: self.surface_id,
                    arguments: base,
                })
            }
            None => Ok(NewTabC {
                surface_id: self.surface_id,
                arguments: core::ptr::null_mut(),
            }),
        }
    }
}

/// Sync with: rustty_ipc_action_tag_e
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ActionKey {
    NewWindow,
    NewTab,
    ToggleQuickTerminal,
}

/// Sync with: rustty_ipc_action_u
#[allow(dead_code)]
#[repr(C)]
pub union ActionCValue {
    pub new_window: NewWindowC,
    pub new_tab: NewTabC,
    pub toggle_quick_terminal: (),
}

/// Sync with: rustty_ipc_action_s
#[repr(C)]
pub struct ActionC {
    pub key: ActionKey,
    pub value: ActionCValue,
}

// For ABI compatibility, we expect that this is our union size.
// At the time of writing, we don't promise ABI compatibility
// so we can change this but I want to be aware of it.
//
// PORT NOTE: reference `comptime { assert(@sizeOf(CValue) == switch(@sizeOf(usize))
// { 4 => 12, 8 => 16, else => unreachable }); }` (034506f14 widened 4/8 -> 12/16
// when `NewTabC` — `{u64, ptr}` — joined the union). Byte-exact `rustty.h` ABI
// pinning is DEFERRED to the embedded C-API milestone.
const _: () = assert!(
    core::mem::size_of::<ActionCValue>()
        == match core::mem::size_of::<usize>() {
            4 => 12,
            8 => 16,
            _ => unreachable!(),
        }
);

impl<'a> Action<'a> {
    /// The active tag (`@as(Key, self)`).
    pub fn key(&self) -> ActionKey {
        match self {
            Action::NewWindow(_) => ActionKey::NewWindow,
            Action::NewTab(_) => ActionKey::NewTab,
            Action::ToggleQuickTerminal => ActionKey::ToggleQuickTerminal,
        }
    }

    // PORT NOTE: reference `pub fn Value(comptime key: Key) type` is a comptime
    // type-level function with no runtime Rust equivalent (see action.rs).

    /// Convert to rustty_ipc_action_s.
    pub fn cval<A: Allocator>(&self, alloc: A) -> Result<ActionC, AllocError> {
        // PORT NOTE: reference `inline else => |v, tag| @unionInit(CValue,
        // @tagName(tag), if hasDecl(v, "cval") v.cval(alloc) else v)`.
        let value = match self {
            Action::NewWindow(v) => ActionCValue {
                new_window: v.cval(alloc)?,
            },
            Action::NewTab(v) => ActionCValue {
                new_tab: v.cval(alloc)?,
            },
            Action::ToggleQuickTerminal => ActionCValue {
                toggle_quick_terminal: (),
            },
        };
        Ok(ActionC {
            key: self.key(),
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionKey, TargetKey};
    use rustty_lib::c_header::header_decls;
    use rustty_lib::r#enum::{check_ghostty_henum, enum_type_values};

    /// The checked-in public C header, embedded at compile time.
    ///
    /// PORT NOTE: the reference `lib.checkGhosttyHEnum` does
    /// `const c = @import("ghostty.h")` — it reflects over the REAL generated
    /// header. Rust has no `@import` for a C header, so these tests parse the
    /// named enum block out of the checked-in header's text. The C declarations
    /// come from the header, never from the descriptor under test (S45-F1).
    const HEADER_SRC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../include/rustty-embed.h"
    ));

    /// PORT NOTE (N29): the `fields` VALUES are `<RustEnum>::Variant as i64`,
    /// read out of the enum, not the key's index in a hand-typed list. Without
    /// that the check compares the header to a literal and cannot observe the
    /// port's enums at all.
    fn run_henum_src(
        src: &str,
        header_enum: &str,
        prefix: &str,
        fields: &[(&str, i64)],
    ) -> Result<(), String> {
        let t = enum_type_values(fields);
        let decls = header_decls(src, header_enum);
        check_ghostty_henum(&t, prefix, |name| decls.get(name).copied())
    }

    /// Self-check that the two probes below can actually FAIL — i.e. that they
    /// read the header and not the descriptor.
    ///
    /// PORT NOTE: no reference analogue; the oracle gets this for free because
    /// `@import("ghostty.h")` is the real header.
    #[test]
    fn ghostty_h_probe_can_fail() {
        let fields = [
            ("class", TargetKey::Class as i64),
            ("detect", TargetKey::Detect as i64),
        ];
        let run = |src: &str| {
            run_henum_src(
                src,
                "rustty_ipc_target_tag_e",
                "RUSTTY_IPC_TARGET_",
                &fields,
            )
        };
        run(HEADER_SRC).unwrap();

        let dropped = HEADER_SRC.replace("  RUSTTY_IPC_TARGET_DETECT,\n", "");
        assert_ne!(dropped, HEADER_SRC);
        assert!(run(&dropped).is_err(), "dropped enumerator must fail");

        let renamed = HEADER_SRC.replace("RUSTTY_IPC_TARGET_DETECT", "RUSTTY_IPC_TARGET_DETECTS");
        assert_ne!(renamed, HEADER_SRC);
        assert!(run(&renamed).is_err(), "renamed enumerator must fail");

        let revalued = HEADER_SRC.replace(
            "  RUSTTY_IPC_TARGET_CLASS,\n",
            "  RUSTTY_IPC_TARGET_CLASS = 7,\n",
        );
        assert_ne!(revalued, HEADER_SRC);
        assert!(run(&revalued).is_err(), "revalued enumerator must fail");
    }

    // test "ghostty.h Target.Key" {
    //     try lib.checkGhosttyHEnum(Key, "GHOSTTY_IPC_TARGET_");
    // }
    #[test]
    fn ghostty_h_target_key() {
        let _ = (TargetKey::Class, TargetKey::Detect);
        run_henum_src(
            HEADER_SRC,
            "rustty_ipc_target_tag_e",
            "RUSTTY_IPC_TARGET_",
            &[
                ("class", TargetKey::Class as i64),
                ("detect", TargetKey::Detect as i64),
            ],
        )
        .unwrap();
    }

    // test "ghostty.h Action.Key" {
    //     try lib.checkGhosttyHEnum(Key, "GHOSTTY_IPC_ACTION_");
    // }
    #[test]
    fn ghostty_h_action_key() {
        let _ = (
            ActionKey::NewWindow,
            ActionKey::NewTab,
            ActionKey::ToggleQuickTerminal,
        );
        run_henum_src(
            HEADER_SRC,
            "rustty_ipc_action_tag_e",
            "RUSTTY_IPC_ACTION_",
            &[
                ("new_window", ActionKey::NewWindow as i64),
                ("new_tab", ActionKey::NewTab as i64),
                (
                    "toggle_quick_terminal",
                    ActionKey::ToggleQuickTerminal as i64,
                ),
            ],
        )
        .unwrap();
    }
}
