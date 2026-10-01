//! PORTS: ghostty src/config/Config.zig — `Keybinds` (the keybind set wrapping
//! `inputpkg.Binding.Set`, its key tables and chain target), with its
//! `parseCLI`, `clone`, `equal` and `formatEntry`.

use super::*;

/// Stores a set of keybinds.
///
/// PORTS: `Keybinds` (Config.zig:6415). The big keybind set wrapping
/// `inputpkg.Binding.Set`. Skeleton with the field set + method sigs.
// pub const Keybinds = struct { set, tables, chain_target: union(enum) {...} };
//
// PORT NOTE: no `#[derive(Clone)]` — the reference's deep copy is the explicit
// `clone` method (`clone_in`, via `Set::clone_set` + the table map's deep clone);
// a derived shallow `Clone` would be wrong (and the ordered-map's deep clone is an
// inherent `clone()`, not the `Clone` trait). `Default` gives the `.{}` zero value.
#[derive(Default)]
pub struct Keybinds {
    // set: inputpkg.Binding.Set = .{},
    pub set: rustty_input::binding::Set,

    /// Defined key tables. The default key table is always the root "set",
    /// which allows all table names to be available without reservation.
    // tables: std.StringArrayHashMapUnmanaged(inputpkg.Binding.Set) = .empty,
    //
    // PORT NOTE: `StringArrayHashMapUnmanaged` is an insertion-ordered string map.
    // Its rustty analogue is `AutoArrayHashMapUnmanaged<Vec<u8>, Set>` (the same
    // ordered map `RepeatableStringMap` uses), so `clone`/`equal`/`formatEntryDocs`
    // iterate tables in insertion order exactly as the reference does.
    pub tables: rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged<
        Vec<u8>,
        rustty_input::binding::Set,
    >,

    /// The most recent binding target for `chain=` additions.
    ///
    /// This is intentionally tracked at the Keybinds level so that chains can
    /// apply across table boundaries according to parse order.
    // chain_target: union(enum) { root, table: []const u8 } = .root,
    pub chain_target: KeybindsChainTarget,
}

/// PORTS: `Keybinds.chain_target`'s anonymous `union(enum) { root, table: []const u8 }`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum KeybindsChainTarget {
    #[default]
    Root,
    Table(Vec<u8>),
}

/// PORTS: the inferred error set of `Keybinds.parseCLI` — the reference returns
/// `error.ValueRequired`, `error.InvalidFormat`, plus the errors of
/// `Binding.Set.parseAndPut` (`Allocator.Error || error{InvalidFormat,
/// InvalidAction}`) and `Keybinds.init` (`Allocator.Error`). `Allocator.Error`
/// (`OutOfMemory`) aborts in the Rust port (no recoverable allocation failure),
/// and `parseAndPut`'s `InvalidAction` is also surfaced verbatim. The ported
/// tests assert the *specific* error (`error.InvalidFormat`), so this dedicated
/// type carries that variant rather than collapsing into `args::Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindsParseError {
    // error.ValueRequired
    ValueRequired,
    // error.InvalidFormat
    InvalidFormat,
    // error.InvalidAction (from Binding.Set.parseAndPut)
    InvalidAction,
}

// PORT NOTE: `Binding.Set.parseAndPut` returns `ParseAndPutError {InvalidFormat,
// InvalidAction}`. The reference `try`s it inside `parseCLI`, so each variant
// propagates into `parseCLI`'s error union; this mapping reproduces that.
impl From<rustty_input::binding::ParseAndPutError> for KeybindsParseError {
    fn from(e: rustty_input::binding::ParseAndPutError) -> KeybindsParseError {
        match e {
            rustty_input::binding::ParseAndPutError::InvalidFormat => {
                KeybindsParseError::InvalidFormat
            }
            rustty_input::binding::ParseAndPutError::InvalidAction => {
                KeybindsParseError::InvalidAction
            }
        }
    }
}

impl Keybinds {
    // pub fn init(self: *Keybinds, alloc: Allocator) !void
    pub fn init(&mut self) {
        use rustty_input::binding::{Action, Flags, Trigger, TriggerKey};
        use rustty_input::ctrl_or_super;
        use rustty_vt::input::key::Key;
        use rustty_vt::input::key_mods::Mods;

        // PORT NOTE: `inputpkg.ctrlOrSuper(m)` — re-exported at the input package
        // surface (lib.rs). `inputpkg.Mods` is `rustty_vt::input::key_mods::Mods`.
        // Triggers are built with `Trigger { key, mods }`; `.key = .{ .unicode = c }`
        // -> `TriggerKey::Unicode(c as u21)` and `.key = .{ .physical = .x }` ->
        // `TriggerKey::Physical(Key::X)`. `performable = true` -> `Flags { performable:
        // true, ..Default }`. The `builtin.target.os.tag.isDarwin()` comptime/runtime
        // checks map to `cfg!(target_vendor = "apple")` (same observable behavior).

        // Helper to build a unicode trigger.
        fn uni(c: char, mods: Mods) -> Trigger {
            Trigger {
                key: TriggerKey::Unicode(c as u32),
                mods,
            }
        }
        // Helper to build a physical trigger.
        fn phys(k: Key, mods: Mods) -> Trigger {
            Trigger {
                key: TriggerKey::Physical(k),
                mods,
            }
        }
        // .{ .performable = true }
        let performable = Flags {
            performable: true,
            ..Flags::default()
        };

        // We don't clear the memory because it's in the arena and unlikely
        // to be free-able anyways (since arenas can only clear the last
        // allocated value). This isn't a memory leak because the arena
        // will be freed when the config is freed.
        // self.set = .{};
        self.set = rustty_input::binding::Set::default();
        // self.tables = .empty;
        self.tables = rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged::new();
        // self.chain_target = .root;
        self.chain_target = KeybindsChainTarget::Root;

        // keybinds for opening and reloading config
        self.set.put(
            uni(
                ',',
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::ReloadConfig,
        );
        // .{ .open_config = .default },
        self.set.put(
            uni(',', ctrl_or_super(Mods::default())),
            Action::OpenConfig(rustty_input::binding::OpenConfig::DEFAULT),
        );

        {
            self.set.put(
                phys(Key::Copy, Mods::default()),
                Action::CopyToClipboard(rustty_input::binding::CopyToClipboard::Mixed),
            );
            self.set.put(
                phys(Key::Paste, Mods::default()),
                Action::PasteFromClipboard,
            );

            // On non-MacOS desktop envs (Windows, KDE, Gnome, Xfce), ctrl+insert is an
            // alt keybinding for Copy and shift+ins is an alt keybinding for Paste
            //
            // The order of these blocks is important. The *last* added keybind for a given action is
            // what will display in the menu. We want the more typical keybinds after this block to be
            // the standard
            if !cfg!(target_vendor = "apple") {
                self.set.put(
                    phys(
                        Key::Insert,
                        Mods {
                            ctrl: true,
                            ..Mods::default()
                        },
                    ),
                    Action::CopyToClipboard(rustty_input::binding::CopyToClipboard::Mixed),
                );
                self.set.put(
                    phys(
                        Key::Insert,
                        Mods {
                            shift: true,
                            ..Mods::default()
                        },
                    ),
                    Action::PasteFromClipboard,
                );
            }

            // On macOS we default to super but Linux ctrl+shift since
            // ctrl+c is to kill the process.
            let mods: Mods = if cfg!(target_vendor = "apple") {
                Mods {
                    super_: true,
                    ..Mods::default()
                }
            } else {
                Mods {
                    ctrl: true,
                    shift: true,
                    ..Mods::default()
                }
            };

            self.set.put_flags(
                uni('c', mods),
                Action::CopyToClipboard(rustty_input::binding::CopyToClipboard::Mixed),
                performable,
            );
            self.set
                .put_flags(uni('v', mods), Action::PasteFromClipboard, performable);
        }

        // Increase font size mapping for keyboards with dedicated plus keys (like german)
        // Note: this order matters below because the C API will only return
        // the last keybinding for a given action. The macOS app uses this to
        // set the expected keybind for the menu.
        self.set.put(
            uni('=', ctrl_or_super(Mods::default())),
            Action::IncreaseFontSize(1.0),
        );
        self.set.put(
            uni('+', ctrl_or_super(Mods::default())),
            Action::IncreaseFontSize(1.0),
        );

        self.set.put(
            uni('-', ctrl_or_super(Mods::default())),
            Action::DecreaseFontSize(1.0),
        );
        self.set.put(
            uni('0', ctrl_or_super(Mods::default())),
            Action::ResetFontSize,
        );

        self.set.put(
            uni(
                'j',
                Mods {
                    shift: true,
                    ctrl: true,
                    super_: true,
                    ..Mods::default()
                },
            ),
            Action::WriteScreenFile(rustty_input::binding::WriteScreen::COPY),
        );

        self.set.put(
            uni(
                'j',
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::WriteScreenFile(rustty_input::binding::WriteScreen::PASTE),
        );

        self.set.put(
            uni(
                'j',
                ctrl_or_super(Mods {
                    shift: true,
                    alt: true,
                    ..Mods::default()
                }),
            ),
            Action::WriteScreenFile(rustty_input::binding::WriteScreen::OPEN),
        );

        // Expand Selection
        self.set.put_flags(
            phys(
                Key::ArrowLeft,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Left),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::ArrowRight,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Right),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::ArrowUp,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Up),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::ArrowDown,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Down),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::PageUp,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::PageUp),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::PageDown,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::PageDown),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::Home,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Home),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::End,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::End),
            performable,
        );

        // Tabs common to all platforms
        self.set.put(
            phys(
                Key::Tab,
                Mods {
                    ctrl: true,
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::PreviousTab,
        );
        self.set.put(
            phys(
                Key::Tab,
                Mods {
                    ctrl: true,
                    ..Mods::default()
                },
            ),
            Action::NextTab,
        );

        // Windowing
        if !cfg!(target_vendor = "apple") {
            self.set.put(
                uni(
                    'n',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewWindow,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseSurface,
            );
            self.set.put(
                uni(
                    'q',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::Quit,
            );
            self.set.put(
                phys(
                    Key::F4,
                    Mods {
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseWindow,
            );
            self.set.put(
                uni(
                    't',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewTab,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseTab(rustty_input::binding::CloseTabMode::This),
            );
            self.set.put_flags(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PreviousTab,
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowRight,
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NextTab,
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::PageUp,
                    Mods {
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::PreviousTab,
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::PageDown,
                    Mods {
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::NextTab,
                performable,
            );
            self.set.put(
                uni(
                    'o',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Right),
            );
            self.set.put(
                uni(
                    'e',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Down),
            );
            self.set.put_flags(
                uni(
                    '[',
                    Mods {
                        ctrl: true,
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Previous),
                performable,
            );
            self.set.put_flags(
                uni(
                    ']',
                    Mods {
                        ctrl: true,
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Next),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowUp,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Up),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowDown,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Down),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Left),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowRight,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Right),
                performable,
            );

            // Resizing splits
            self.set.put_flags(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Up,
                    10,
                )),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Down,
                    10,
                )),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Left,
                    10,
                )),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Right,
                    10,
                )),
                performable,
            );

            // Viewport scrolling
            self.set.put(
                phys(
                    Key::Home,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToTop,
            );
            self.set.put(
                phys(
                    Key::End,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToBottom,
            );
            self.set.put(
                phys(
                    Key::PageUp,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageUp,
            );
            self.set.put(
                phys(
                    Key::PageDown,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageDown,
            );

            // Semantic prompts
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(-1),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(1),
            );

            // Move tab
            self.set.put_flags(
                phys(
                    Key::PageUp,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::MoveTab(-1),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::PageDown,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::MoveTab(1),
                performable,
            );

            // Search
            self.set.put_flags(
                uni(
                    'f',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::StartSearch,
                performable,
            );
            self.set.put_flags(
                phys(Key::Escape, Mods::default()),
                Action::EndSearch,
                performable,
            );

            // Inspector, matching Chromium
            self.set.put(
                uni(
                    'i',
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::Inspector(rustty_input::binding::InspectorMode::Toggle),
            );

            // Terminal
            self.set.put(
                uni(
                    'a',
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::SelectAll,
            );

            // Selection clipboard paste
            self.set.put(
                phys(
                    Key::Insert,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PasteFromSelection,
            );
        }
        {
            // On macOS we default to super but everywhere else
            // is alt.
            let mods: Mods = if cfg!(target_vendor = "apple") {
                Mods {
                    super_: true,
                    ..Mods::default()
                }
            } else {
                Mods {
                    alt: true,
                    ..Mods::default()
                }
            };

            // Cmd/Alt+N for goto tab N
            // const start: u21 = '1';
            // const end: u21 = '8';
            // comptime var i: u21 = start; inline while (i <= end) : (i += 1) { ... }
            //
            // PORT NOTE: the reference uses `@field(inputpkg.Key,
            // comptimePrint("digit_{u}", .{i}))` to look up the physical key. Here
            // we enumerate the (char, physical-digit-key) pairs '1'..='8' directly,
            // preserving the same iteration order (and thus the same
            // last-binding-wins behavior).
            let start: u21 = '1' as u32;
            for (i, digit_key) in [
                ('1', Key::Digit1),
                ('2', Key::Digit2),
                ('3', Key::Digit3),
                ('4', Key::Digit4),
                ('5', Key::Digit5),
                ('6', Key::Digit6),
                ('7', Key::Digit7),
                ('8', Key::Digit8),
            ] {
                let icp: u21 = i as u32;
                // We register BOTH the physical `digit_N` key and the unicode
                // `N` key. This allows most keyboard layouts to work with
                // this shortcut. Namely, AZERTY doesn't produce unicode `N`
                // for their digit keys (they're on shifted keys on the same
                // physical keys).

                self.set.put_flags(
                    phys(digit_key, mods),
                    Action::GotoTab(((icp - start) + 1) as usize),
                    Flags {
                        // On macOS we keep this not performable so that the
                        // keyboard shortcuts in tabs work. In the future the
                        // correct fix is to fix the reverse mapping lookup
                        // to allow us to lookup performable keybinds
                        // conditionally.
                        performable: !cfg!(target_vendor = "apple"),
                        ..Flags::default()
                    },
                );

                // Important: this must be the LAST binding set so that the
                // libghostty trigger API returns this one for the action,
                // so that things like the macOS tab bar key equivalent label
                // work properly.
                self.set.put_flags(
                    uni(i, mods),
                    Action::GotoTab(((icp - start) + 1) as usize),
                    Flags {
                        performable: !cfg!(target_vendor = "apple"),
                        ..Flags::default()
                    },
                );
            }
            self.set.put_flags(
                uni('9', mods),
                Action::LastTab,
                Flags {
                    // See comment above with the numeric goto_tab
                    performable: !cfg!(target_vendor = "apple"),
                    ..Flags::default()
                },
            );
        }

        // Toggle fullscreen
        self.set.put(
            phys(Key::Enter, ctrl_or_super(Mods::default())),
            Action::ToggleFullscreen,
        );

        // Toggle zoom a split
        self.set.put(
            phys(
                Key::Enter,
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::ToggleSplitZoom,
        );

        // Toggle command palette, matches VSCode
        self.set.put(
            uni(
                'p',
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::ToggleCommandPalette,
        );

        // Mac-specific keyboard bindings.
        if cfg!(target_vendor = "apple") {
            self.set.put(
                uni(
                    'q',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Quit,
            );
            self.set.put_flags(
                uni(
                    'k',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ClearScreen,
                performable,
            );
            self.set.put(
                uni(
                    'a',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::SelectAll,
            );

            // Undo/redo
            self.set.put_flags(
                uni(
                    't',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::Undo,
                performable,
            );
            self.set.put_flags(
                uni(
                    'z',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Undo,
                performable,
            );
            self.set.put_flags(
                uni(
                    'z',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::Redo,
                performable,
            );

            // Viewport scrolling
            self.set.put(
                phys(
                    Key::Home,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToTop,
            );
            self.set.put(
                phys(
                    Key::End,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToBottom,
            );
            self.set.put(
                phys(
                    Key::PageUp,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageUp,
            );
            self.set.put(
                phys(
                    Key::PageDown,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageDown,
            );
            self.set.put_flags(
                uni(
                    'j',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToSelection,
                performable,
            );

            // Semantic prompts
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(-1),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(1),
            );

            // Mac windowing
            self.set.put(
                uni(
                    'n',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NewWindow,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseSurface,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseTab(rustty_input::binding::CloseTabMode::This),
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseWindow,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        shift: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseAllWindows,
            );
            self.set.put(
                uni(
                    't',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NewTab,
            );
            self.set.put(
                uni(
                    '[',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PreviousTab,
            );
            self.set.put(
                uni(
                    ']',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NextTab,
            );
            self.set.put(
                uni(
                    'd',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Right),
            );
            self.set.put(
                uni(
                    'd',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Down),
            );
            self.set.put(
                uni(
                    '[',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Previous),
            );
            self.set.put(
                uni(
                    ']',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Next),
            );
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Up),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Down),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Left),
            );
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Right),
            );
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Up,
                    10,
                )),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Down,
                    10,
                )),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Left,
                    10,
                )),
            );
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Right,
                    10,
                )),
            );
            self.set.put(
                uni(
                    '=',
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::EqualizeSplits,
            );

            // Jump to prompt, matches Terminal.app
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(-1),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(1),
            );

            // Search
            self.set.put_flags(
                uni(
                    'f',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::StartSearch,
                performable,
            );
            self.set.put_flags(
                uni(
                    'e',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::SearchSelection,
                performable,
            );
            self.set.put_flags(
                uni(
                    'f',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::EndSearch,
                performable,
            );
            self.set.put_flags(
                phys(Key::Escape, Mods::default()),
                Action::EndSearch,
                performable,
            );
            self.set.put_flags(
                uni(
                    'g',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NavigateSearch(rustty_input::binding::NavigateSearch::Next),
                performable,
            );
            self.set.put_flags(
                uni(
                    'g',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NavigateSearch(rustty_input::binding::NavigateSearch::Previous),
                performable,
            );

            // Inspector, matching Chromium
            self.set.put(
                uni(
                    'i',
                    Mods {
                        alt: true,
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Inspector(rustty_input::binding::InspectorMode::Toggle),
            );

            // Alternate keybind, common to Mac programs
            self.set.put(
                uni(
                    'f',
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ToggleFullscreen,
            );

            // Selection clipboard paste, matches Terminal.app
            self.set.put(
                uni(
                    'v',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PasteFromSelection,
            );

            // "Natural text editing" keybinds. This forces these keys to go back
            // to legacy encoding (not fixterms). It seems macOS users more than
            // others are used to these keys so we set them as defaults. If
            // people want to get back to the fixterm encoding they can set
            // the keybinds to `unbind`.
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Text(b"\\x05".to_vec()),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Text(b"\\x01".to_vec()),
            );
            self.set.put(
                phys(
                    Key::Backspace,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Text(b"\\x15".to_vec()),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::Esc(b"b".to_vec()),
            );
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::Esc(b"f".to_vec()),
            );
        }
    }

    // pub fn parseCLI(self: *Keybinds, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), KeybindsParseError> {
        // var copy: ?[]u8 = null;
        // const value = value: {
        //     const value = input orelse return error.ValueRequired;
        //
        //     // If we don't have a colon, use the value as-is, no copy
        //     if (std.mem.indexOf(u8, value, ":") == null)
        //         break :value value;
        //
        //     // If we have a colon, we copy the whole value for now. We could
        //     // do this more efficiently later if we wanted to.
        //     const buf = try alloc.alloc(u8, value.len);
        //     copy = buf;
        //
        //     @memcpy(buf, value);
        //     break :value buf;
        // };
        // errdefer if (copy) |v| alloc.free(v);
        //
        // PORT NOTE: the reference conditionally copies `value` into the arena when
        // it contains a colon (so the parsed binding can reference arena-stable
        // bytes). In the Rust port the parse APIs take `&[u8]` and copy any bytes
        // they retain (owned `Vec<u8>` payloads), so no pre-copy / `errdefer free`
        // is needed; we operate on the borrowed `value` directly.
        let value = match input {
            // const value = input orelse return error.ValueRequired;
            None => return Err(KeybindsParseError::ValueRequired),
            Some(v) => v,
        };

        // Check for special values
        if value.is_empty() {
            // log.info("config has 'keybind =', using default keybinds", .{});
            crate::config_struct::log_info(
                "config has 'keybind =', using default keybinds".to_string(),
            );
            self.init();
            return Ok(());
        }

        if value == b"clear" {
            // We don't clear the memory because its in the arena and unlikely
            // to be free-able anyways (since arenas can only clear the last
            // allocated value). This isn't a memory leak because the arena
            // will be freed when the config is freed.
            // log.info("config has 'keybind = clear', all keybinds cleared", .{});
            crate::config_struct::log_info(
                "config has 'keybind = clear', all keybinds cleared".to_string(),
            );
            self.set = rustty_input::binding::Set::default();
            self.tables = rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged::new();
            self.chain_target = KeybindsChainTarget::Root;
            return Ok(());
        }

        // Check for table syntax: "name/" or "name/binding"
        // We look for '/' only before the first '=' to avoid matching
        // action arguments like "foo=text:/hello".
        // const eq_idx = std.mem.indexOfScalar(u8, value, '=') orelse value.len;
        let eq_idx = value.iter().position(|&c| c == b'=').unwrap_or(value.len());
        // if (std.mem.indexOfScalar(u8, value[0..eq_idx], '/')) |slash_idx| table: {
        if let Some(slash_idx) = value[0..eq_idx].iter().position(|&c| c == b'/') {
            'table: {
                let table_name = &value[0..slash_idx];

                // Length zero is valid, so you can set `/=action` for the slash key
                if table_name.is_empty() {
                    break 'table;
                }

                // Ignore '+', '>' because they can be part of sequences and
                // triggers. This lets things like `ctrl+/=action` work.
                if table_name.iter().any(|&c| c == b'+' || c == b'>') {
                    break 'table;
                }

                let binding = &value[slash_idx + 1..];

                // Get or create the table
                // const gop = try self.tables.getOrPut(alloc, table_name);
                let gop = self
                    .tables
                    .get_or_put_with(table_name.to_vec(), rustty_input::binding::Set::default);
                if !gop.found_existing {
                    // We need to copy our table name into the arena
                    // for valid lookups later.
                    // gop.key_ptr.* = try alloc.dupe(u8, table_name);
                    // gop.value_ptr.* = .{};
                    //
                    // PORT NOTE: the key (`Vec<u8>`) is already owned by the map
                    // (`get_or_put_with` took an owned copy), so the `dupe` is
                    // implicit; the value was initialized to an empty `Set`.
                }

                // If there's no binding after the slash, this is a table
                // definition/clear command
                if binding.is_empty() {
                    // log.debug("config has 'keybind = {s}/', table cleared", .{table_name});
                    crate::config_struct::log_debug(format!(
                        "config has 'keybind = {}/', table cleared",
                        String::from_utf8_lossy(table_name)
                    ));
                    // gop.value_ptr.* = .{};
                    *self.tables.value_at_mut(gop.index) = rustty_input::binding::Set::default();
                    self.chain_target = KeybindsChainTarget::Root;
                    return Ok(());
                }

                // Chains are only allowed at the root level. Their target is
                // tracked globally by parse order in `self.chain_target`.
                if binding.starts_with(b"chain=") {
                    return Err(KeybindsParseError::InvalidFormat);
                }

                // Parse and add the binding to the table
                // try gop.value_ptr.parseAndPut(alloc, binding);
                self.tables.value_at_mut(gop.index).parse_and_put(binding)?;
                // self.chain_target = .{ .table = gop.key_ptr.* };
                self.chain_target =
                    KeybindsChainTarget::Table(self.tables.entry_at(gop.index).unwrap().0.clone());
                return Ok(());
            }
        }

        if value.starts_with(b"chain=") {
            match &self.chain_target {
                // .root => try self.set.parseAndPut(alloc, value),
                KeybindsChainTarget::Root => self.set.parse_and_put(value)?,
                // .table => |table_name| { ... }
                KeybindsChainTarget::Table(table_name) => {
                    // const table = self.tables.getPtr(table_name) orelse { ... };
                    let table_name = table_name.clone();
                    match self.tables.get_ptr(&table_name) {
                        None => {
                            self.chain_target = KeybindsChainTarget::Root;
                            return Err(KeybindsParseError::InvalidFormat);
                        }
                        Some(table) => table.parse_and_put(value)?,
                    }
                }
            }
            return Ok(());
        }

        // Parse into default set
        // try self.set.parseAndPut(alloc, value);
        self.set.parse_and_put(value)?;
        self.chain_target = KeybindsChainTarget::Root;
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Keybinds, alloc: Allocator) Allocator.Error!Keybinds
    pub fn clone_in(&self) -> Keybinds {
        // var tables: std.StringArrayHashMapUnmanaged(inputpkg.Binding.Set) = .empty;
        // try tables.ensureTotalCapacity(alloc, @intCast(self.tables.count()));
        let mut tables: rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged<
            Vec<u8>,
            rustty_input::binding::Set,
        > = rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged::new();
        // var it = self.tables.iterator();
        // while (it.next()) |entry| {
        for (key, value) in self.tables.iter() {
            // const key = try alloc.dupe(u8, entry.key_ptr.*);
            // tables.putAssumeCapacity(key, try entry.value_ptr.clone(alloc));
            tables.put(key.clone(), value.clone_set());
        }

        // return .{ .set = try self.set.clone(alloc), .tables = tables };
        //
        // PORT NOTE: the reference's returned struct leaves `chain_target` at its
        // default `.root` (it is not a copied field); the Rust `..Default::default()`
        // does the same.
        Keybinds {
            set: self.set.clone_set(),
            tables,
            ..Keybinds::default()
        }
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Keybinds, other: Keybinds) bool
    pub fn equal(&self, other: &Keybinds) -> bool {
        // if (!equalSet(&self.set, &other.set)) return false;
        if !Keybinds::equal_set(&self.set, &other.set) {
            return false;
        }

        // Compare tables
        // if (self.tables.count() != other.tables.count()) return false;
        if self.tables.count() != other.tables.count() {
            return false;
        }

        // var it = self.tables.iterator();
        // while (it.next()) |entry| {
        for (key, value) in self.tables.iter() {
            // const other_set = other.tables.get(entry.key_ptr.*) orelse return false;
            let other_set = match other.tables.get(key) {
                Some(s) => s,
                None => return false,
            };
            // if (!equalSet(entry.value_ptr, &other_set)) return false;
            if !Keybinds::equal_set(value, other_set) {
                return false;
            }
        }

        true
    }

    // fn equalSet(self: *const inputpkg.Binding.Set, other: *const inputpkg.Binding.Set) bool
    fn equal_set(self_: &rustty_input::binding::Set, other: &rustty_input::binding::Set) -> bool {
        use rustty_input::binding::Value;

        // Two keybinds are considered equal if their primary bindings
        // are the same. We don't compare reverse mappings and such.
        // const self_map = &self.bindings;
        // const other_map = &other.bindings;

        // If the count of mappings isn't identical they can't be equal
        // if (self_map.count() != other_map.count()) return false;
        if self_.bindings_count() != other.bindings_count() {
            return false;
        }

        // var it = self_map.iterator();
        // while (it.next()) |self_entry| {
        for (self_key, self_value) in self_.iter_bindings() {
            // If the trigger isn't in the other map, they can't be equal
            // const other_entry = other_map.getEntry(self_entry.key_ptr.*) orelse return false;
            let other_value = match other.get_value(*self_key) {
                Some(v) => v,
                None => return false,
            };

            // If the entry types are different, they can't be equal
            // if (std.meta.activeTag(self_entry.value_ptr.*) !=
            //     std.meta.activeTag(other_entry.value_ptr.*)) return false;
            if std::mem::discriminant(self_value) != std::mem::discriminant(other_value) {
                return false;
            }

            match (self_value, other_value) {
                // They're equal if both leader sets are equal.
                // .leader => if (!equalSet(self_entry.value_ptr.*.leader,
                //                          other_entry.value_ptr.*.leader)) return false,
                (Value::Leader(self_leader), Value::Leader(other_leader)) => {
                    if !Keybinds::equal_set(self_leader, other_leader) {
                        return false;
                    }
                }

                // Actions are compared by field directly
                // .leaf => { const self_leaf = ...; const other_leaf = ...;
                //     if (!deepEqual(Set.Leaf, self_leaf, other_leaf)) return false; },
                //
                // PORT NOTE: `deepEqual(Set.Leaf, ...)` deep-compares both fields of
                // the leaf (`action` and `flags`). `Action::equal` is the action
                // deep-compare; `Flags` is a copy type compared by `==`.
                (Value::Leaf(self_leaf), Value::Leaf(other_leaf)) => {
                    if !(self_leaf.action.equal(&other_leaf.action)
                        && self_leaf.flags == other_leaf.flags)
                    {
                        return false;
                    }
                }

                // .leaf_chained => { ... }
                (Value::LeafChained(self_chain), Value::LeafChained(other_chain)) => {
                    // if (self_chain.flags != other_chain.flags) return false;
                    if self_chain.flags != other_chain.flags {
                        return false;
                    }
                    // if (self_chain.actions.items.len != other_chain.actions.items.len) return false;
                    if self_chain.actions.len() != other_chain.actions.len() {
                        return false;
                    }
                    // for (self_chain.actions.items, other_chain.actions.items) |a1, a2| {
                    //     if (!deepEqual(Action, a1, a2)) return false; }
                    for (a1, a2) in self_chain.actions.iter().zip(other_chain.actions.iter()) {
                        if !a1.equal(a2) {
                            return false;
                        }
                    }
                }

                // The active tags matched above, so any remaining pairing is
                // impossible; preserved as the exhaustive match arm.
                _ => unreachable!(),
            }
        }

        true
    }

    /// Like formatEntry but has an option to include docs.
    // pub fn formatEntryDocs(self: Keybinds, formatter: formatterpkg.EntryFormatter, docs: bool) !void
    //
    // PORT NOTE: the reference threads `formatterpkg.EntryFormatter` and writes a
    // top-level `{f}` trigger prefix into a fixed buffer before delegating to
    // `v.formatEntries(&writer, formatter)`. The Rust `EntryFormatter` + the
    // `Binding.Set` value's `format_entries(&mut RewindBuffer, formatter)` provide
    // exactly these pieces; the trigger/table-name prefix is written into the
    // shared `RewindBuffer` via the public `write_trigger_to_buffer`/`write_all`
    // helpers. The `docs` branch's `help_strings.KeybindAction` lookup is the
    // build-generated help artifact (modeled by `helpgen_actions::HelpStrings`);
    // `RealHelpStrings` delegates to the generated `keybind_help` table.
    //
    // CORRECTION (LEDGER S76-F1): this note used to say `docs = true`
    // (list-keybinds/show-config `--docs`) "emits the real per-action doc text".
    // The lookup key is `@tagName(v)` of the `Set.Value` union (`v_tag_name`),
    // which never names an action, so, as in the reference, `docs = true` adds
    // only a blank line per root binding (test
    // `keybinds_format_entry_docs_writes_only_the_blank_line`).
    pub fn format_entry_docs<W: Writer>(
        &self,
        formatter: &mut rustty_cfg_fmt::EntryFormatter<W>,
        docs: bool,
    ) -> Result<(), FormatError<W::Error>> {
        use rustty_input::binding::{write_trigger_to_buffer, RewindBuffer};
        use rustty_input::helpgen_actions::{HelpStrings, RealHelpStrings};

        // if (self.set.bindings.count() == 0 and self.tables.count() == 0) {
        //     try formatter.formatEntry(void, {});
        //     return;
        // }
        if self.set.bindings_count() == 0 && self.tables.count() == 0 {
            // try formatter.formatEntry(void, {});
            return format_entry_void(formatter.name, formatter.writer);
        }

        // var buf: [1024]u8 = undefined;
        //
        // PORT NOTE: the reference's fixed 1024-byte stack buffer is the
        // `RewindBuffer` (its `format_entries` callers rewind via the `end`
        // cursor), which is capped at the same `REWIND_BUFFER_CAPACITY` = 1024 so
        // an overlong prefix fails here exactly as `std.Io.Writer.fixed` does
        // (`catch return error.OutOfMemory`).
        let help_strings = RealHelpStrings;

        // Format root set bindings
        // var iter = self.set.bindings.iterator();
        // while (iter.next()) |next| {
        for (k, v) in self.set.iter_bindings() {
            // const k = next.key_ptr.*;
            // const v = next.value_ptr.*;
            if docs {
                // try formatter.writer.writeAll("\n");
                formatter.writer.write_all(b"\n")?;
                // const name = @tagName(v);
                let name = v_tag_name(v);
                // inline for (@typeInfo(help_strings.KeybindAction).@"struct".decls) |decl| {
                //     if (std.mem.eql(u8, decl.name, name)) { ... break; } }
                for decl_name in rustty_input::binding::Action::FIELD_NAMES {
                    if decl_name.as_bytes() == name && help_strings.has_decl(decl_name) {
                        // const help = @field(help_strings.KeybindAction, decl.name);
                        let help = help_strings.field(decl_name);
                        // try formatter.writer.writeAll("# " ++ decl.name ++ "\n");
                        formatter.writer.write_all(b"# ")?;
                        formatter.writer.write_all(decl_name.as_bytes())?;
                        formatter.writer.write_all(b"\n")?;
                        // var lines = std.mem.splitScalar(u8, help, '\n');
                        // while (lines.next()) |line| { ... }
                        for line in help.split(|&c| c == b'\n') {
                            // try formatter.writer.writeAll("#   ");
                            formatter.writer.write_all(b"#   ")?;
                            // try formatter.writer.writeAll(line);
                            formatter.writer.write_all(line)?;
                            // try formatter.writer.writeAll("\n");
                            formatter.writer.write_all(b"\n")?;
                        }
                        break;
                    }
                }
            }

            // var writer: std.Io.Writer = .fixed(&buf);
            // writer.print("{f}", .{k}) catch return error.OutOfMemory;
            let mut writer = RewindBuffer::new();
            write_trigger_to_buffer(k, &mut writer).map_err(|_| FormatError::OutOfMemory)?;
            // try v.formatEntries(&writer, formatter);
            format_entries_into(v, &mut writer, formatter)?;
        }

        // Format table bindings
        // var table_iter = self.tables.iterator();
        // while (table_iter.next()) |table_entry| {
        for (table_name, table_set) in self.tables.iter() {
            // const table_name = table_entry.key_ptr.*;
            // const table_set = table_entry.value_ptr.*;

            // var binding_iter = table_set.bindings.iterator();
            // while (binding_iter.next()) |next| {
            for (k, v) in table_set.iter_bindings() {
                // const k = next.key_ptr.*;
                // const v = next.value_ptr.*;

                // var writer: std.Io.Writer = .fixed(&buf);
                // writer.print("{s}/{f}", .{ table_name, k }) catch return error.OutOfMemory;
                let mut writer = RewindBuffer::new();
                writer
                    .write_all(table_name)
                    .map_err(|_| FormatError::OutOfMemory)?;
                writer
                    .write_all(b"/")
                    .map_err(|_| FormatError::OutOfMemory)?;
                write_trigger_to_buffer(k, &mut writer).map_err(|_| FormatError::OutOfMemory)?;
                // try v.formatEntries(&writer, formatter);
                format_entries_into(v, &mut writer, formatter)?;
            }
        }

        Ok(())
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Keybinds, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        formatter: &mut rustty_cfg_fmt::EntryFormatter<W>,
    ) -> Result<(), FormatError<W::Error>> {
        // try self.formatEntryDocs(formatter, false);
        self.format_entry_docs(formatter, false)
    }
}

// `@tagName(v)` where `v` is the `Binding.Set.Value` union (`Binding.zig:2138-2148`,
// `union(enum) { leader, leaf, leaf_chained }`).
//
// CORRECTION (LEDGER S76-F1): this used to return the leaf's ACTION tag (or the
// first chained action's), with a note saying that is what "the reference's docs
// path meaningfully matches against `help_strings` decls". It is not: the
// reference takes the tag of the VALUE union, which never equals a
// `help_strings.KeybindAction` decl (those are action names), so the reference
// prints no help block. Returning the action tag made `+list-keybinds --docs`
// print `# <action>` blocks the reference never prints. The reference's own
// behaviour is recorded upstream as UD180 (latent). It stays faithful under R13
// as extended 2026-09-23: the flag's own help (`list_keybinds.zig:50-51`) says
// `--docs` "currently does nothing", and finishing it would mean inventing
// output for chained and leader bindings (reasons in UD180).
fn v_tag_name(v: &rustty_input::binding::Value) -> &'static [u8] {
    use rustty_input::binding::Value;
    match v {
        Value::Leader(_) => b"leader",
        Value::Leaf(_) => b"leaf",
        Value::LeafChained(_) => b"leaf_chained",
    }
}

// PORT NOTE: adapter for `v.formatEntries(&writer, formatter)` — the reference
// passes the fixed writer + formatter and bubbles `error.OutOfMemory`/writer
// errors. The Rust `Value::format_entries` returns `FormatEntriesError<W::Error>`;
// both variants fold into the formatter error union (`FormatError`) variant-for-
// variant, exactly the reference's inferred-error-set merge at the `try` site.
fn format_entries_into<W: Writer>(
    v: &rustty_input::binding::Value,
    buffer: &mut rustty_input::binding::RewindBuffer,
    formatter: &mut rustty_cfg_fmt::EntryFormatter<W>,
) -> Result<(), FormatError<W::Error>> {
    use rustty_input::binding::FormatEntriesError;
    match v.format_entries(buffer, formatter) {
        Ok(()) => Ok(()),
        Err(FormatEntriesError::Writer(e)) => Err(FormatError::Writer(e)),
        Err(FormatEntriesError::OutOfMemory) => Err(FormatError::OutOfMemory),
    }
}
