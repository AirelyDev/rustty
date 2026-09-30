# N452: effect callbacks that re-enter the Terminal C API

This is an isolated, **non-FFI** reproduction of the borrow shapes behind NEW-FINDINGS **N452**,
fixed in P7-W2 T9. It is used to PROVE the fix. It follows the doctrine in memories
`rust-mut-args-strongly-protected` and `shaper-hook-sb-clean-interleave`.

## The bug

`rustty_terminal_vt_write` runs `wrapper.stream.next_slice(input)`, and effect callbacks run
synchronously inside it. The reference headers forbid only `vt_write`/`vt_write_until_ground` on the
same terminal from a callback, so a callback may call `resize`, `set` (the handler-field options),
`get` (`VT_GROUND`, `SEMANTIC_FAILURE`, `KITTY_CLIPBOARD_WRITE_MAX_BYTES`) and the continuation APIs.

In the shipped port:

- every `Stream` driver took `&mut self`: a protected Unique over every stream byte (parser, UTF-8
  decoder, continuation tracker, handler) for the whole feed;
- the handler dispatch (`stream::Handler` trait methods, `&mut self`) and the vt-c trampoline
  (`&mut Handler`) are protected Uniques over the handler bytes;
- the C callback gets the wrapper (recovered by the D3 `offset_of!` seam over exposed provenance), and
  the re-entered API reaches `wrapper.stream.*` through it: a foreign access to protected bytes.

Zig's `*Stream`/`*Handler` carry no protector: port soundness only, no UD.

## The fix

1. **Raw-self stream drivers.** `next_slice_raw`, `next_slice_until_ground_raw`, `next_raw` and the
   private drivers they reach take `this: *mut Stream` (the reference's `self: *Self`) with narrow
   place accesses; the pure dispatchers (`print`, `execute`, `csi_dispatch`, `esc_dispatch`,
   `configure_charset`, `osc_dispatch`) take `handler: &mut H` only. Across a handler action only the
   handler bytes are protected. `ground_raw` / `write_continuation_raw` read by place.
2. **`live_handler`.** Each vt-c trampoline derives ONE raw pointer from its own `&mut Handler`
   argument (a child of every live handler protector) and publishes it in the wrapper's
   `live_handler` for exactly the callback's duration (saved and restored, so nested effects nest).
   Every C API that touches handler bytes goes through `handler_ptr(w)`: that pointer when set, else
   `stream.handler` by place.
3. **By-place stream reads** in the C API: never a whole-`Stream` `&self` (it would retag the
   protected handler bytes).

The `stream::Handler` trait keeps `&mut self` (the rejected alternative, a raw-self trait, would
rewrite ~100 methods across every handler impl and drop `noalias` from the print hot path).

## The model

`src/main.rs` models `Stream`/`Handler`/`TerminalWrapper` with the same field nesting, the same D3
seam (`offset_of!` + `with_exposed_provenance_mut`, whole-wrapper `expose_provenance` at creation),
the same trampoline shape, and a feed where a BEL executes mid-CSI (parser not at ground) and the
sequence then completes. The handler keeps using `self` after the effect returns, as the real
handler methods do. `resize` fires a nested `write_pty` effect whose callback re-enters again.
Three switches select the shape: `RAW_STREAM` (fix 1), `STASH` (fix 2), `PLACE_READS` (fix 3).

## Results (`bash run-all.sh`, 16 seeds each, foreground)

| candidate | SB | TB |
|---|---|---|
| `n452_shipped_resize` | CLEAN | UB |
| `n452_shipped_set` | UB | UB |
| `n452_shipped_get_ground` | UB | UB |
| `n452_shipped_get_handler` | UB | CLEAN |
| `n452_shipped_continuation` | UB | UB |
| `n452_raw_stream_only_set` (fix 1+3, no `live_handler`) | UB | UB |
| `n452_stash_only_get_ground` (fix 2+3, `&mut self` stream) | UB | UB |
| `n452_raw_stream_whole_ground` (fix 1+2, whole-stream `ground(&self)`) | UB | UB |
| `n452_fixed_resize` | CLEAN | CLEAN |
| `n452_fixed_set` | CLEAN | CLEAN |
| `n452_fixed_get_ground` | CLEAN | CLEAN |
| `n452_fixed_get_handler` | CLEAN | CLEAN |
| `n452_fixed_continuation` | CLEAN | CLEAN |
| `n452_fixed_all` (every class, nested effect) | CLEAN | CLEAN |

- Every shipped API class is UB under at least one model; `set`, `get(VT_GROUND)` and the
  continuation APIs under both.
- `n452_shipped_resize` under SB: the re-entered `resize` retags `&mut Handler` from a wildcard
  (exposed-provenance) pointer, and Miri's SB does not pop protected items on a wildcard retag, so
  it misses this one; TB reports the foreign write to the protected handler.
- `n452_shipped_get_handler` under TB: a foreign READ of a protected tag that has not yet been
  written is allowed (Reserved); SB rejects it.
- Each fix part alone is insufficient (the three partial candidates), and together they are clean
  under both models.

The real code is proven by the vt-c Miri targets `reentrant_callback_resize`,
`reentrant_callback_set_handler_option`, `reentrant_callback_get_vt_ground` and
`reentrant_callback_continuation` (`crates/rustty-vt-c/src/terminal.rs`).

## N454: the C paste reader re-entering the terminal (P7-W2 T10)

`rustty_terminal_paste` pulls the text through the embedder's `RusttyMimeReader` synchronously, and
neither `io.h` nor `paste.h` (as the reference's at 12752b2ac) forbids that reader from re-entering the
terminal. In the shipped port the paste held, across the reader call:

- `Handler::paste(&mut self)`: a protected Unique over every handler byte;
- `paste::Context { terminal: &Terminal, kitty_clipboard: Some(KittyClipboard { grants: &mut Grants }) }`,
  a by-value argument of `paste::paste`, whose reference fields are protected for the whole call.

The reader is not an effect trampoline, so `live_handler` was null. A reader calling `set`, `resize`,
`reset` or `vt_write` wrote protected handler, terminal or grants bytes.

The fix: `Handler::paste_raw(this: *mut Handler)` (the reference's `self: *Handler`) with place
accesses; `Context.terminal: *const Terminal` and `KittyClipboard.grants: *mut Grants` (the
reference's pointer types); vt-c publishes the paste's handler pointer in `live_handler` for the call.

The model adds `Terminal.bracketed`, `Handler.kitty_grants`, a `full_reset`, `Handler::paste` /
`paste_raw`, a `PasteCtx` whose `terminal`/`grants` are references or raw per switch, `paste_pkg` (mode
read, reader, encode, as the real text path), `c_reset`, `c_paste` and a `reader_cb` that re-enters
`set` (+`get`), `resize` (nested `write_pty`), `reset`, and `vt_write` (nested BEL). Switches:
`PASTE_RAW_SELF`, `PASTE_RAW_TERMINAL`, `PASTE_RAW_GRANTS`; the N452 fix is on in every `n454_*` run.

| candidate | SB | TB |
|---|---|---|
| `n454_shipped_set` | UB | UB |
| `n454_shipped_resize` | UB | UB |
| `n454_shipped_reset` | UB | UB |
| `n454_shipped_vt_write` | UB | UB |
| `n454_raw_ctx_only_set` (raw Context, `&mut self` paste) | UB | UB |
| `n454_raw_self_ref_terminal_resize` (raw-self, `&Terminal` Context) | UB | UB |
| `n454_raw_self_ref_grants_reset` (raw-self, `&mut Grants` Context) | UB | UB |
| `n454_fixed_set` | CLEAN | CLEAN |
| `n454_fixed_resize` | CLEAN | CLEAN |
| `n454_fixed_reset` | CLEAN | CLEAN |
| `n454_fixed_vt_write` | CLEAN | CLEAN |
| `n454_fixed_all` (every class, nested effects) | CLEAN | CLEAN |

The real code is proven by the vt-c Miri target `reentrant_callback_paste_reader`
(`crates/rustty-vt-c/src/terminal.rs`).
