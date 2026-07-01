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
    /// The arguments to pass to Ghostty as the command.
    NewWindow(NewWindow<'a>),

    /// Toggle the quick terminal.
    ToggleQuickTerminal,
}

/// PORT NOTE: reference `arguments: ?[][:0]const u8` -> borrowed
/// `Option<&'a [&'a [u8]]>` (a borrowed slice of borrowed raw-byte args).
pub struct NewWindow<'a> {
    /// A list of command arguments to launch in the new window. If this is
    /// `null` the command configured in the config or the user's default
    /// shell should be launched.
    ///
    /// It is an error for this to be non-`null`, but zero length.
    pub arguments: Option<&'a [&'a [u8]]>,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct NewWindowC {
    /// null terminated list of arguments
    /// it will be null itself if there are no arguments
    //
    // PORT NOTE: reference `?[*]?[*:0]const u8` -> `*mut *const u8` (null array
    // pointer == None; null element == the NUL terminator / a None argument).
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

/// Sync with: rustty_ipc_action_tag_e
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ActionKey {
    NewWindow,
    ToggleQuickTerminal,
}

/// Sync with: rustty_ipc_action_u
#[allow(dead_code)]
#[repr(C)]
pub union ActionCValue {
    pub new_window: NewWindowC,
    pub toggle_quick_terminal: (),
}

/// Sync with: rustty_ipc_action_s
#[repr(C)]
pub struct ActionC {
    pub key: ActionKey,
    pub value: ActionCValue,
}

// PORT NOTE: reference `comptime { assert(@sizeOf(CValue) == switch(@sizeOf(usize))
// { 4 => 4, 8 => 8 }); }`. Reproduced against the ACTUAL Rust layout (the union is
// a single `*mut *const u8` = one pointer). Byte-exact `rustty.h` ABI pinning is
// DEFERRED to the embedded C-API milestone.
const _: () = assert!(core::mem::size_of::<ActionCValue>() == core::mem::size_of::<usize>());

impl<'a> Action<'a> {
    /// The active tag (`@as(Key, self)`).
    pub fn key(&self) -> ActionKey {
        match self {
            Action::NewWindow(_) => ActionKey::NewWindow,
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
    use rustty_lib::r#enum::{check_ghostty_henum, enum_type};
    use rustty_lib::target::Target as EnumTarget;

    // test "ghostty.h Target.Key" {
    //     try lib.checkGhosttyHEnum(Key, "GHOSTTY_IPC_TARGET_");
    // }
    #[test]
    fn ghostty_h_target_key() {
        let _ = (TargetKey::Class, TargetKey::Detect);
        let t = enum_type(EnumTarget::C, &[Some("class"), Some("detect")]);
        let fields = t.fields.clone();
        check_ghostty_henum(&t, "RUSTTY_IPC_TARGET_", |name| {
            fields.iter().find_map(|(n, v)| {
                let expected = format!("RUSTTY_IPC_TARGET_{}", n.to_ascii_uppercase());
                if expected == name {
                    Some(*v as i64)
                } else {
                    None
                }
            })
        })
        .unwrap();
    }

    // test "ghostty.h Action.Key" {
    //     try lib.checkGhosttyHEnum(Key, "GHOSTTY_IPC_ACTION_");
    // }
    #[test]
    fn ghostty_h_action_key() {
        let _ = (ActionKey::NewWindow, ActionKey::ToggleQuickTerminal);
        let t = enum_type(
            EnumTarget::C,
            &[Some("new_window"), Some("toggle_quick_terminal")],
        );
        let fields = t.fields.clone();
        check_ghostty_henum(&t, "RUSTTY_IPC_ACTION_", |name| {
            fields.iter().find_map(|(n, v)| {
                let expected = format!("RUSTTY_IPC_ACTION_{}", n.to_ascii_uppercase());
                if expected == name {
                    Some(*v as i64)
                } else {
                    None
                }
            })
        })
        .unwrap();
    }
}
