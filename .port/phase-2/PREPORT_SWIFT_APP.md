# PREPORT_SWIFT_APP.md — Phase-5 pre-port analysis: the macos-swift-app tier

**Tier:** `macos-swift-app` (completion-plan sub-project #2 — reuse the ~32k-LOC Swift
AppKit/SwiftUI frontend by linking it to the ported Rust core over the libghostty C-ABI).
**Oracle:** `/Users/gael/projects/ghostty` pinned @ `4749c4e9` (read-only; all `file:line`
resolves against this pin).
**Inputs read:** `src/apprt/embedded.zig` (2251 loc, 0 tests), `include/ghostty.h` (1209 lines),
`src/main_c.zig` (247), `src/config/CApi.zig` (280), `src/benchmark/CApi.zig`, `src/apprt/action.zig`,
`src/apprt/structs.zig`, `.port/audits/GHOSTTY_LIFETIMES.tsv` (schema + class vocabulary),
`.port/phase-2/EMBEDDED_BLUEPRINT.md`, `.port/phase-2/VALIDATION_REPORT.md §5`,
completion-plan memory.
**Not re-raised:** VALIDATION_REPORT §5/§2/§3 items (gate machinery, status drift); M6
(overflow-checks) is *referenced*, not re-filed.

---

## 0. Surface-count reconciliation (honest coverage)

The tasking says "the 271 exported fns." Measured against the pin, the **macOS app C-ABI is 90
`GHOSTTY_API` entrypoints**, not 271:

| Source file | `export fn ghostty_*` | Role |
|---|---:|---|
| `src/apprt/embedded.zig` | **71** | app/surface/inspector runtime (65 portable + **6 Darwin-only**) |
| `src/config/CApi.zig` | **13** | config lifecycle + `_get`/`_trigger` (already ported: rustty-config C3) |
| `src/main_c.zig` | **5** | `ghostty_init`/`_cli_try_action`/`_info`/`_translate`/`_string_free` |
| `src/benchmark/CApi.zig` | **1** | `ghostty_benchmark_cli` |
| **app C-ABI total** | **90** | == `grep -c '^GHOSTTY_API' include/ghostty.h` → **90** ✅ |

The **271** figure appears to fold in the **separate `libghostty-vt` product**:
`src/lib_vt.zig` `@export`s **168** `ghostty_*` symbols (`lib_vt.zig:140-…`) — that is the
**already-ported rustty-vt-c tier**, and the macOS app does **not** link it (the app library is
rooted at `main_c.zig`, whose `comptime` block at `main_c.zig:33-46` references only
`config.CApi` + `apprt.runtime.CAPI` + `benchmark.CApi` — never `lib_vt`). Whole-tree totals for
context: `grep -rn 'export fn' src` = 113, `grep -rn '@export' src` = 169.

**This does not change any policy below** — the LIFETIMES rows, invariants, and catch_unwind
policy are *per-struct* and *per-export-uniform*, so they hold for whichever superset the shipped
`.xcframework` ultimately re-exports. Where the tasking's "271" matters (catch_unwind §4) the
policy is stated per-export and therefore covers 90 or 271 identically.

The 6 Darwin-only exports (gated by `comptime { if (isDarwin()) _ = Darwin; }`,
`embedded.zig:1392-1396`), which the **iOS-in-scope** lock widens from `target_os="macos"` to
`any(macos, ios)`:
`ghostty_surface_set_display_id` (:2134), `ghostty_surface_quicklook_font` (:2147),
`ghostty_surface_quicklook_word` (:2193), `ghostty_inspector_metal_init` (:2225),
`ghostty_inspector_metal_render` (:2229), `ghostty_inspector_metal_shutdown` (:2243).

---

## 1. LIFETIMES.tsv rows — C-ABI boundary structs

Schema (from `GHOSTTY_LIFETIMES.tsv`): `file⇥struct⇥field⇥zig_type⇥class⇥rust_type⇥evidence`.
Classes in use here: **GHOSTTY_C** (repr(C) opaque ctx / non-null fn-ptr crossing the boundary),
**FFI** (owned foreign objc/Metal/ImGui/CoreText handle, thread-affine), **OWNED**,
**BORROW_PARAM** (host-owned transient, valid only for the call), **BACKREF** (non-owning parent),
**BORROW_FIELD**. Every **GHOSTTY_C** and **FFI** row is a boundary-crossing field; every field
carrying AppKit/Metal/objc/CoreText identity is tagged **[MAIN-THREAD-AFFINE — drop-on-wrong-thread
= UB]** in its rust_type.

```
# ── App.Options == ghostty_runtime_config_s (extern struct; embedded.zig:35-81 / ghostty.h:1019-1028)
src/apprt/embedded.zig	App.Options (extern)	userdata	?*anyopaque = null	GHOSTTY_C	*mut c_void (host-owned opaque; forwarded verbatim to every callback via App.opts; libghostty NEVER derefs or frees it)	embedded.zig:42 doc 'Userdata passed to all callbacks'; returned back out by ghostty_app_userdata :1434
src/apprt/embedded.zig	App.Options (extern)	supports_selection_clipboard	bool = false	GHOSTTY_C	bool (plain ABI value; gates Surface.supportsClipboard :668)	embedded.zig:45; read at :668
src/apprt/embedded.zig	App.Options (extern)	wakeup	*const fn(AppUD) callconv(.c) void	GHOSTTY_C	extern "C" fn(*mut c_void) (NON-NULL required; host-owned thunk; must outlive the App — stored by value in App.opts)	embedded.zig:49; invoked App.wakeup :234
src/apprt/embedded.zig	App.Options (extern)	action	*const fn(*App, apprt.Target.C, apprt.Action.C) callconv(.c) bool	GHOSTTY_C	extern "C" fn(*mut App, Target_C, Action_C) -> bool (NON-NULL; receives raw *App + BY-VALUE Target.C/Action.C unions; host reads active arm by tag)	embedded.zig:52; called performAction :282-286
src/apprt/embedded.zig	App.Options (extern)	read_clipboard	*const fn(SurfaceUD, c_int, *apprt.ClipboardRequest) callconv(.c) bool	GHOSTTY_C	extern "C" fn(*mut c_void, c_int, *mut ClipboardRequest) -> bool (NON-NULL; hands host a lib-heap ClipboardRequest ptr that round-trips back via complete_clipboard_request — see I7)	embedded.zig:58; called :686-690
src/apprt/embedded.zig	App.Options (extern)	confirm_read_clipboard	*const fn(SurfaceUD, [*:0]const u8, *apprt.ClipboardRequest, apprt.ClipboardRequestType) callconv(.c) void	GHOSTTY_C	extern "C" fn(*mut c_void, *const c_char, *mut ClipboardRequest, u8) (NON-NULL; str borrowed for the call; PRESERVES the request ptr, does NOT free it — :731 comment)	embedded.zig:63-68; called :717-722
src/apprt/embedded.zig	App.Options (extern)	write_clipboard	*const fn(SurfaceUD, c_int, [*]const CAPI.ClipboardContent, usize, bool) callconv(.c) void	GHOSTTY_C	extern "C" fn(*mut c_void, c_int, *const ClipboardContent, usize, bool) (NON-NULL; the ClipboardContent array is lib-owned, valid only for the call — freed at :743 defer)	embedded.zig:71-77; called setClipboard :751-757
src/apprt/embedded.zig	App.Options (extern)	close_surface	?*const fn(SurfaceUD, bool) callconv(.c) void = null	GHOSTTY_C	Option<extern "C" fn(*mut c_void, bool)> (NULLABLE — uses null-fn-ptr niche; orelse-guarded at :642)	embedded.zig:80; guarded call :642-647
# ── Surface.Options == ghostty_surface_config_s (extern struct; embedded.zig:425-465 / ghostty.h:467-480)
src/apprt/embedded.zig	Surface.Options (extern)	platform	Platform.C = undefined	GHOSTTY_C	union { macos: {nsview: *mut c_void}, ios: {uiview: *mut c_void} } repr(C) — tag lives in platform_tag; see Platform.C rows	embedded.zig:429; parsed Platform.init :374-391
src/apprt/embedded.zig	Surface.Options (extern)	platform_tag	c_int = 0	GHOSTTY_C	c_int (0 = INVALID sentinel; 1=macos 2=ios; intToEnum rejects garbage — I10)	embedded.zig:428; PlatformTag :394-400
src/apprt/embedded.zig	Surface.Options (extern)	userdata	?*anyopaque = null	GHOSTTY_C	*mut c_void (host-owned; stored on Surface.userdata :413; passed to close/read/write cbs; returned by ghostty_surface_userdata :1565)	embedded.zig:432
src/apprt/embedded.zig	Surface.Options (extern)	working_directory	?[*:0]const u8 = null	BORROW_PARAM	Option<*const c_char> (host-owned transient; sliceTo+opened+finalized during init only, NOT retained — :490-528)	embedded.zig:441; consumed :490
src/apprt/embedded.zig	Surface.Options (extern)	command	?[*:0]const u8 = null	BORROW_PARAM	Option<*const c_char> (transient; sliceTo into config.command.shell during init — arena-copied — :531-537)	embedded.zig:451; consumed :531
src/apprt/embedded.zig	Surface.Options (extern)	env_vars	?[*]EnvVar = null	BORROW_PARAM	Option<*const EnvVar> + count (host-owned array; each key/value dupeZ'd into config arena during init — :540-551)	embedded.zig:454; consumed :542
src/apprt/embedded.zig	Surface.Options (extern)	env_var_count	usize = 0	BORROW_PARAM	usize (paired length for env_vars — no independent lifetime)	embedded.zig:455
src/apprt/embedded.zig	Surface.Options (extern)	initial_input	?[*:0]const u8 = null	BORROW_PARAM	Option<*const c_char> (transient; zig-string-escaped into config.input arena during init — :554-571)	embedded.zig:458
# ── Platform.C == ghostty_platform_u (extern union; embedded.zig:363-371 / ghostty.h:456-459)
src/apprt/embedded.zig	Platform.C (extern union)	macos.nsview	?*anyopaque	FFI	NonNull<c_void> as objc NSView* [MAIN-THREAD-AFFINE — drop-on-wrong-thread = UB; AppKit view has main-thread affinity]; stored as objc.Object in Platform.MacOS :353; null => error.NSViewMustBeSet	embedded.zig:365; Platform.init :379-381; Platform.MacOS :351-354
src/apprt/embedded.zig	Platform.C (extern union)	ios.uiview	?*anyopaque	FFI	NonNull<c_void> as objc UIView* [MAIN-THREAD-AFFINE — UIKit view]; null => error.UIViewMustBeSet	embedded.zig:368; Platform.init :384-388
# ── EnvVar == ghostty_env_var_s (extern struct; embedded.zig:402-408 / ghostty.h:443-446)
src/apprt/embedded.zig	EnvVar (extern)	key	[*:0]const u8	BORROW_PARAM	*const c_char (host-owned; read + dupeZ'd during Surface.init only)	embedded.zig:404; :543-548
src/apprt/embedded.zig	EnvVar (extern)	value	[*:0]const u8	BORROW_PARAM	*const c_char (host-owned; read + dupeZ'd during init only)	embedded.zig:407; :544-549
# ── App (heap handle == ghostty_app_t opaque; embedded.zig:120-125)
src/apprt/embedded.zig	App	core_app	*CoreApp	BORROW_FIELD	*mut rustty_core::App (lifecycle owned by the CAPI layer: created in app_new_ :1413, destroyed in ghostty_app_free :1442 AFTER App.terminate; App holds a non-owning ptr; terminate :149 does NOT free it)	embedded.zig:120; app_new_ :1413-1419; app_free :1438-1443
src/apprt/embedded.zig	App	opts	Options	OWNED	Options by value (retains the 6 host fn-ptrs + userdata for the App's whole life — I4)	embedded.zig:121; stored :141-146
src/apprt/embedded.zig	App	keymap	input.Keymap	OWNED	Keymap (owned; keymap.deinit() in terminate :150)	embedded.zig:122; init :138, deinit :150
src/apprt/embedded.zig	App	config	Config	OWNED	Config by value (doc :124 'owned by this structure'; cloned at init :135; deinit in terminate :151; re-assigned on config_change :313-318)	embedded.zig:125; :135,:151,:313
# ── Surface (heap handle == ghostty_surface_t opaque; embedded.zig:410-423)
src/apprt/embedded.zig	Surface	app	*App	BACKREF	*mut App (non-owning parent; App outlives Surface; used everywhere as self.app.core_app.alloc)	embedded.zig:411; rtApp :637
src/apprt/embedded.zig	Surface	platform	Platform	FFI	Platform enum holding objc nsview/uiview [MAIN-THREAD-AFFINE] — see Platform.C rows	embedded.zig:412
src/apprt/embedded.zig	Surface	userdata	?*anyopaque = null	GHOSTTY_C	*mut c_void (host-owned opaque; passed to close/read/write/confirm cbs)	embedded.zig:413
src/apprt/embedded.zig	Surface	core_surface	CoreSurface	OWNED	rustty_core::Surface<embedded::App> by value — SELF-REFERENTIAL: core Surface.init writes core_surface.rt_surface = &(this Surface); container MUST be heap-boxed + address-stable for life (I2); deinit :608	embedded.zig:414; init :580-586; deinit :608
src/apprt/embedded.zig	Surface	inspector	?*Inspector = null	OWNED	Option<Box<Inspector>> (created initInspector :614-623, freed freeInspector :625-631; single-instance)	embedded.zig:418
src/apprt/embedded.zig	Surface	title	?[:0]const u8 = null	OWNED	Option<CString> (alloc.dupeZ'd on set_title preAction :304; freed on replace :303 and deinit :602; returned borrowed by getTitle :659)	embedded.zig:422; :303-304,:602
src/apprt/embedded.zig	Surface	content_scale	apprt.ContentScale	OWNED	ContentScale value (updated :785; NaN/<1 clamped at update :782-783)	embedded.zig:415
src/apprt/embedded.zig	Surface	size	apprt.SurfaceSize	OWNED	SurfaceSize value (dedupe-guarded update :801)	embedded.zig:416
src/apprt/embedded.zig	Surface	cursor_pos	apprt.CursorPos	OWNED	CursorPos value (init {-1,-1} :478; phantom-move-guarded :883)	embedded.zig:417
# ── Inspector (heap handle == ghostty_inspector_t opaque; embedded.zig:994-1003)
src/apprt/embedded.zig	Inspector	surface	*Surface	BACKREF	*mut Surface (non-owning; Surface owns the Inspector)	embedded.zig:997
src/apprt/embedded.zig	Inspector	ig_ctx	*cimgui.c.ImGuiContext	FFI	*mut ImGuiContext [MAIN-THREAD-AFFINE — ImGui global-context; ImGui_DestroyContext on deinit :1038]; SetCurrentContext before every use	embedded.zig:998; init :1016; deinit :1038
src/apprt/embedded.zig	Inspector	backend	?Backend = null	FFI	Option<MetalBackend> [MAIN-THREAD-AFFINE — ImGui_ImplMetal_Shutdown on deinit :1010]	embedded.zig:999; Backend.deinit :1008-1012; initMetal :1047
src/apprt/embedded.zig	Inspector	content_scale	f64 = 1	OWNED	f64 value (cached for cursor calc :1116)	embedded.zig:1000
src/apprt/embedded.zig	Inspector	instant	?std.time.Instant = null	OWNED	Option<Instant> (delta-time for imgui animation :1233-1239)	embedded.zig:1003
# ── Text == ghostty_text_s (extern struct; embedded.zig:1293-1306 / ghostty.h:408-415) — RETURN-VALUE OWNERSHIP TRANSFER
src/apprt/embedded.zig	CAPI.Text (extern)	text	?[*:0]const u8	OWNED	Option<*mut c_char> — lib-allocated via global.alloc, ownership TRANSFERS to host; host MUST call ghostty_surface_free_text exactly once (deinit frees text[0..text_len:0] :1303 — SAME allocator both ends, I6)	embedded.zig:1298; deinit :1301-1305; produced readTextLocked :1673-1674
src/apprt/embedded.zig	CAPI.Text (extern)	text_len	usize	OWNED	usize (paired length used for the free slice bound — MUST be preserved across the boundary for correct free)	embedded.zig:1299; used :1303
# ── String == ghostty_string_s (extern struct; main_c.zig:63-102 / ghostty.h:402-406) — RETURN-VALUE OWNERSHIP TRANSFER
src/main_c.zig	String (extern)	ptr	?[*]const u8	OWNED	Option<*mut u8> — lib-allocated (state.alloc); host frees via ghostty_string_free; sentinel flag selects [0..len:0] vs [0..len] free (I6)	main_c.zig:64; deinit :94-101; ghostty_string_free :155
src/main_c.zig	String (extern)	len	usize	OWNED	usize (free-slice bound)	main_c.zig:65
src/main_c.zig	String (extern)	sentinel	bool	OWNED	bool (selects sentinel-slice free path — MUST round-trip)	main_c.zig:66; :96-100
# ── ClipboardContent == ghostty_clipboard_content_s (extern struct; embedded.zig:1287-1290 / ghostty.h:76-79)
src/apprt/embedded.zig	CAPI.ClipboardContent (extern)	mime	[*:0]const u8	BORROW_PARAM	*const c_char (in setClipboard the lib builds a temp array pointing at caller-owned contents.mime; valid only for the write_clipboard call — array freed :743)	embedded.zig:1288; :745
src/apprt/embedded.zig	CAPI.ClipboardContent (extern)	data	[*:0]const u8	BORROW_PARAM	*const c_char (same call-scoped borrow as mime)	embedded.zig:1289; :746
# ── Target.CValue == ghostty_target_u (extern union; action.zig:30-33 / ghostty.h:560-562) — surface ptr crossing in action_cb
src/apprt/action.zig	Target.CValue (extern union)	surface	*apprt.Surface	GHOSTTY_C	*mut embedded::Surface (the rt_surface ptr — cval sends v.rt_surface NOT the core surface; MUST be the same heap handle the host was given at ghostty_surface_new, I1/I2)	action.zig:32; cval :47
# ── Action.ConfigChange arm == ghostty_action_config_change_s (ghostty.h:800-803) — borrow-for-call
src/apprt/embedded.zig	Action.config_change	config	*Config (ghostty_config_t)	BORROW_PARAM	*mut Config (borrowed for the callback only; performPreAction CLONES it into App.config :313-318 before returning — host may free its config after — I8)	embedded.zig:308-319
# ── Clipboard-request heap handle (not a struct field, but a boundary lifetime; embedded.zig:681-696)
src/apprt/embedded.zig	Surface.clipboardRequest	state_ptr	*apprt.ClipboardRequest	OWNED	Box<ClipboardRequest> — lib-heap-allocated, ptr handed to host via read_clipboard cb, round-trips back via ghostty_surface_complete_clipboard_request; freed there (:732) OR re-preserved on confirm path (:731 comment); free-on-!started :692 — EXACTLY-ONCE completion or leak (I7)	embedded.zig:682-696; complete :699-733; export :2007-2018
```

**40 rows.** GHOSTTY_C = 16 (opaque userdata ×3, the 6 App.Options fn-ptrs, platform/platform_tag,
Surface.userdata, Target surface). FFI (thread-affine) = 6 (nsview, uiview, Surface.platform,
ig_ctx, backend, + the CTFont return of `ghostty_surface_quicklook_font` :2179-2185 — a `+1`
CTFontRef handed to the host, **[MAIN-THREAD-AFFINE]**, not a struct field so noted here).

---

## 2. Paired invariants across the Swift↔Rust seam

Each invariant is a contract the **Swift caller** and the **Rust callee** must jointly uphold; a
breach is memory-unsafety on one side or the other.

- **I1 — Handle identity.** `ghostty_app_t`/`ghostty_surface_t`/`ghostty_inspector_t` are opaque
  `void*` (ghostty.h:57-60). The Swift app stores the raw pointer and passes it back verbatim;
  Rust must `Box::into_raw`/`from_raw` the **same** address. `ghostty_surface_free` (:1560) and
  `ghostty_app_free` (:1438) consume the handle — no use-after-free on the Swift side.
- **I2 — `rt_surface` self-alias / pin.** After `ghostty_surface_new`, `core_surface.rt_surface`
  points back at the *containing* `embedded::Surface`, and `Target.cval` (action.zig:47) ships
  *that* pointer to the host. ⇒ the `embedded::Surface` must be **heap-boxed and never moved**
  post-init or `rt_surface` (and every action callback's `target.surface`) dangles. (EMBEDDED_
  BLUEPRINT ADV-FIX 8.) Rust: `Pin<Box<Surface>>` or a documented no-move contract.
- **I3 — userdata opacity.** The host owns `userdata` (App.Options:42, Surface.Options:432); Rust
  stores it as `*mut c_void`, forwards it to callbacks, and **never** dereferences or frees it.
  `ghostty_app_userdata`/`ghostty_surface_userdata` return it unchanged.
- **I4 — Callback-ptr lifetime.** The 6 `App.Options` fn-ptrs are copied by value into `App.opts`
  (:141-146) and invoked for the App's whole life. The Swift side must keep its callback thunks
  valid until `ghostty_app_free`. `wakeup`/`action`/`read`/`confirm`/`write` are **non-null**;
  `close_surface` is nullable (orelse-guarded :642).
- **I5 — Main-thread affinity.** Every entrypoint that touches AppKit/Metal/objc/ImGui runs on the
  **main thread** (the Swift app drives them from the main runloop). Corollary the port MUST honor:
  the `nsview`/`uiview` (FFI rows), `ig_ctx`, Metal `backend`, and the CTFont return are
  **!Send** — creating them off-main or **dropping them off-main is UB**. The render `Thread` stays
  in core (completion-plan architecture note) and is a *separate* concern; boundary exports do not
  cross into it except `ghostty_surface_set_display_id` (:2134) which only pushes to the render
  mailbox.
- **I6 — String/Text ownership transfer + allocator identity.** `ghostty_surface_tty_name`
  (String), `ghostty_surface_read_text`/`_read_selection`/`_quicklook_word` (Text) return
  **lib-allocated** buffers; the host must free via `ghostty_string_free` / `ghostty_surface_free_text`
  **exactly once**. Rust must allocate with the **same** global allocator both ends
  (`global.alloc`/`state.alloc`, embedded.zig:1245, main_c.zig:16); the `sentinel`/`text_len`
  fields carry the free-slice bound and must round-trip.
- **I7 — Clipboard-request round-trip (exactly-once).** `clipboardRequest` heap-allocates a
  `*ClipboardRequest`, hands it to the host via `read_clipboard`; the host returns it through
  `ghostty_surface_complete_clipboard_request` (:2007). `complete` frees it (:732) **unless** the
  paste is unsafe/unauthorized, where the request is **preserved** and re-handed via
  `confirm_read_clipboard` (:717-731). Breach = double-free or leak. The `!started` path frees
  eagerly (:692). Rust must model this as a `Box` whose ownership is *transferred out and back*.
- **I8 — Config clone-at-boundary.** `ghostty_config_t` is lib-allocated
  (`ghostty_config_new` CApi.zig:15) and host-freed (`ghostty_config_free` :30). `App.init` **clones**
  the config (:135); `Surface` shallow-copies via `newConfig` (:486, `defer config.deinit()`);
  `config_change` clones in `performPreAction` (:313). ⇒ the Swift app may free its config
  immediately after `ghostty_app_new`/`ghostty_surface_new`/`ghostty_app_update_config`.
- **I9 — ABI layout freeze.** Every `extern struct`/`union`/`enum(c_int)` must match ghostty.h
  field-for-field in **size, align, field order, and enum ordinal**. `Action.Key` (action.zig:351)
  maps *positionally* to `GHOSTTY_ACTION_*` (ghostty.h:885-952) — the doctrine at action.zig:65-77
  ("new actions to the END") is the ABI contract. The port's guard = cbindgen-emitted header
  byte-diffed against `include/ghostty.h` + Rust `const` size/align asserts (the analog of the
  `checkGhosttyHEnum` comptime tests at action.zig:24).
- **I10 — Platform tag sentinel.** `platform_tag == 0` is the reserved INVALID sentinel
  (embedded.zig:395-396); `1=macos, 2=ios`. `Platform.init` `intToEnum`-rejects any other value
  (:375) and rejects a null `nsview` (`error.NSViewMustBeSet` :380). Rust must reproduce the
  sentinel-0 rejection, not `unwrap` a `try_from`.
- **I11 — Mods bit-packing.** `c_int` mods ↔ `input.Mods.Backing` via
  `@bitCast(@truncate(@as(c_uint, @bitCast(x))))` (e.g. :1261-1266, :1767-1770, :1852-1855). The
  packed-struct bit layout must equal `ghostty_input_mods_e` (ghostty.h:127-139, incl. the RIGHT
  variants at bits 6-9). `scroll_mods` is a packed `int` (ghostty.h:125) decoded from `u8`
  (:1885).
- **I12 — Tagged-union arm discipline.** `Target.C`/`Action.C` (action.zig:36-39, 453-478) cross
  **by value**; the host reads `.tag`/`.key` then the matching union arm. Reading the wrong arm =
  UB. The Rust `#[repr(C)]` union must place the tag identically and the host-side Swift switch
  must stay in sync (it is, since Swift is byte-identical to upstream under the reuse fork).
- **I13 — Error → sentinel, never unwind.** Every export catches Zig errors and returns a sentinel
  (`null`/`false`/`.empty`/no-op) + logs; **no error ever propagates across the C-ABI**
  (e.g. :1403, :1547, :765, :1723). This is the seam that maps to §4's catch_unwind/abort policy.
- **I14 — Single lib allocator.** All cross-boundary allocations use one global allocator; the
  host never writes into lib-owned buffers (it only reads Text/String and frees them via the paired
  free fn). Mixed allocators = corruption.
- **I15 — `const char*` transient borrows.** `working_directory`/`command`/`initial_input`/
  `env_vars`/`ghostty_config_load_file(path)`/`_binding_action`/`_text`/`_preedit` args are
  borrowed **for the call only**; the lib copies (`dupeZ`/arena) what it retains (:490-571,
  CApi.zig:72). The Swift app may free the argument after the call returns.
- **I16 — objc retain/release balance.** `ghostty_surface_quicklook_font` returns a CTFont **+1**
  the host must release (:2179 `copyWithAttributes`); `ghostty_inspector_metal_init`/`_render`
  **release** the objc ids passed in (`defer …release`, :1048, :1072-1074); `initMetal` releases
  the device (:1048). Rust must mirror **exact** retain/release counts (CFRetained / objc2 with
  matching ownership).

---

## 3. Decision-#1 surface — the `ghostty_*` symbol fork (LAID OUT, NOT RESOLVED)

**Status tension to surface honestly.** The tasking says this milestone "is gated on **open**
decision #1 … DO NOT resolve it." The completion-plan memory, however, records decision #1 as
**user-locked to Branch A** ("C-ABI symbol identity = `ghostty_*` aliases over a canonical
`rustty_*` ABI … all glue in one generated alias file; mirrors the `identity-ghostty` precedent").
This artifact treats it as **open per the tasking** and makes the pre-port robust to either branch;
the port lead should reconcile the lock vs the "open" framing before the port pass.

### Branch A — `ghostty_*` aliases over a canonical `rustty_*` ABI (the memory-locked direction)
Rust exports canonical `rustty_app_new`… (`#[no_mangle] extern "C"`), and a **generated alias
layer** emits each `ghostty_*` name pointing at the same code. The reused Swift app (~150 files,
~163 hard-coded `ghostty_` symbols) stays byte-identical to upstream → cheap upstream re-sync.
- Alias mechanism options (pick during port, not now): (a) `#[export_name = "ghostty_app_new"]`
  thin wrappers; (b) a generated linker-alias/`.def`/`objcopy --add-symbol`; (c) a cargo
  `identity-ghostty` feature toggling the export names — the direct analog of ghostty's own build
  option the memory cites.
- cbindgen emits **two** headers: `rustty.h` (canonical, the rustty-native product) **and** a
  `ghostty.h` that is **byte-identical to `include/ghostty.h`** so the Swift umbrella header and
  every `ghostty_*_s`/`_e` typedef resolve unchanged (a `diff` gate against the upstream header).

### Branch B — export `ghostty_*` directly (no `rustty_*` canonical)
Rust uses `#[export_name = "ghostty_*"]` on each export; no alias file. Simpler; but freezes
rustty's public C-ABI *name* to `ghostty_` forever and forgoes the `rustty_*` product identity used
elsewhere (and by the umbrella-header story). This is the lower-effort path if the `.xcframework`
is *only ever* consumed by the ghostty-derived Swift app.

### Pre-port work that is IDENTICAL either way (do it now, decision-independent)
1. **This LIFETIMES ledger** (§1) — symbol spelling is irrelevant to field ownership.
2. **The extern struct/enum/union catalog + `const` size/align/ordinal asserts** (§1/I9) — ABI
   layout is name-independent.
3. **The per-export catch_unwind/abort skeleton** (§4) — one wrapper shape for all 90 (or 271).
4. **The umbrella header + `.modulemap` structure.** What Swift *imports* is the **module + header
   content**; what the linker *resolves* is the **symbol name** — these are decoupled. The header
   type/enum/struct declarations are byte-identical to upstream in both branches.

### Pre-port work that DIFFERS by branch
| Concern | Branch A (aliases) | Branch B (direct) |
|---|---|---|
| `#[export_name]` targets | canonical `rustty_*` + generated `ghostty_*` alias file | `ghostty_*` on each export |
| cbindgen headers | **two** (rustty.h + upstream-identical ghostty.h) | one (ghostty.h) |
| Module/umbrella name Swift imports | keep **`GhosttyKit`** + `ghostty.h` → Swift untouched | keep `GhosttyKit`/`ghostty.h` (Swift still needs the ghostty names) |
| Upstream Swift re-sync cost | minimal (byte-identical symbols) | minimal for symbols; but no rustty-native surface |
| New alias-generator to author/gate | **yes** (one file + a symbol-manifest gate) | no |

**Neither branch changes §1/§2/§4.** The only build-time artifact whose *shape* depends on the
decision is the alias generator + whether cbindgen runs once or twice. Everything memory-safety-
relevant is settled by this pre-port regardless.

**Symbol manifest needed for either branch (author now):** the 90 app-C-ABI names with their
signatures, the canonical `rustty_*` spelling, and (Branch A) the `ghostty_*` alias — a machine-
checkable table the alias generator and the header-diff gate both read.

---

## 4. catch_unwind / abort policy for the exported fns

**Reference semantics (Zig side).** Zig has **no unwinding**. Every export already does
`… catch |err| { log.err(...); return <sentinel>; }` (I13) for **recoverable** errors (OOM, parse,
I/O). The **assert-class** — `unreachable`, `quirks.inlineAssert`, integer overflow under
ReleaseSafe — **traps/aborts** the process; it does not unwind. So the ABI contract is: *a
recoverable failure returns a sentinel; an invariant violation aborts; nothing ever unwinds across
the boundary.*

**Rust mirror — the faithful mapping (R1):**

1. **Recoverable → `Result`→sentinel, NOT `catch_unwind`.** Map each Zig `catch |err| return X` to
   `match inner() { Ok(v) => v, Err(e) => { log::error!(...); X } }`. This is ordinary control flow
   and reproduces the sentinel exactly (null/`false`/`String::empty`/no-op). Using `catch_unwind`
   here would be *less* faithful — it would recover an abort-class panic that Zig would have
   aborted on.
2. **Assert-class → `panic = "abort"`.** Set `panic = "abort"` for the shipped
   `.xcframework`/staticlib profile so `assert!`/`unreachable!`/`debug_assert!`/overflow-check
   panics **abort exactly like Zig's trap** — and, as a side effect, an unwind can never cross the
   `extern "C"` boundary (which is UB pre-1.81 and an auto-abort ≥1.81 anyway). This is the primary,
   recommended policy. It also closes the ReleaseSafe-parity gap: overflow in a boundary fn must
   **trap**, not silently wrap — ties to VALIDATION_REPORT **M6** (`overflow-checks = true`); do
   not re-file M6, but this tier *depends* on it.
3. **`catch_unwind`→abort shim ONLY where a crate must stay `panic = "unwind"`.** If any crate on
   the boundary cannot be `panic = "abort"` (e.g. it is shared with a test/render-thread build that
   needs unwinding), wrap **each** export body in `std::panic::catch_unwind(...)` and on `Err`
   **`std::process::abort()`** — **not** recover-to-sentinel. Recovering would turn a Zig abort into
   a silent sentinel = a fidelity fork. The shim's job is purely to convert a would-be
   UB-unwind-across-FFI into the same abort Zig produces.

**Per-export categorization (applies uniformly to the 90 app exports; and identically to the vt-c
168 if the shipped lib re-exports them):**
- **Group R — returns error-sentinel** (`?*T`/`bool`/`String.empty`): `ghostty_app_new` (:1403),
  `ghostty_config_new`/`_clone` (CApi.zig:15,38), `ghostty_surface_new` (:1547),
  `ghostty_surface_tty_name` (:1721), `ghostty_surface_read_text`/`_read_selection`/
  `_binding_action`/`_key`… → `Result`→sentinel, no panic path except the assert-class.
- **Group V — void no-op on error** (the many `catch |err| { log; return; }`): `_tick` (:1427),
  `_draw`/`_refresh`/`_set_size`/`_update_config`/`_split*`/all callbacks → same.
- **Group P — reaches an explicit trap:** `ghostty_surface_quicklook_word` has
  `if (runtime_safety) unreachable` (:2212) on a null pin → **abort** in Rust (must NOT be
  recovered). `render*`/inspector paths inherit assert-class from core.
- **Group M — Rust-only poisoned-mutex hazard (no Zig analog).** The three exports that
  `renderer_state.mutex.lock()` — `ghostty_surface_read_selection` (:1615),
  `ghostty_surface_read_text` (:1635), `ghostty_surface_quicklook_word` (:2198) — have **no Zig
  poisoning** (Zig `Mutex` can't poison). In Rust, a panic *while the lock is held* poisons it, and
  the next `lock().unwrap()` panics → aborts under `panic = "abort"`. Policy: use `lock().unwrap()`
  (abort on poison, matching the "a prior invariant already died" reality) **or** an explicit
  `.unwrap_or_else(PoisonError::into_inner)` if the port wants Zig-identical "keep going" behavior —
  **document whichever as a Rust-only divergence** (there is no reference line to match). Recommend
  `unwrap()`→abort: a poisoned renderer mutex means a prior panic already aborted or corrupted
  state.
- **Group T — thread boundary:** `ghostty_surface_set_display_id` (:2134) only `mailbox.push` +
  `wakeup.notify` — runs on the main thread; the abort/panic must not be *observed* by the render
  thread. No special handling beyond `panic = "abort"`.

**Decision-independence:** this policy is identical under symbol-fork Branch A and Branch B (the
wrapper wraps the same body regardless of the exported name), and it is per-export-uniform so it
covers 90 or 271 exports without change.

---

## 5. comptime inventory (constructs needing comptime/const-generic handling in the port)

- **Darwin export gate** `comptime { if (isDarwin()) _ = Darwin; }` (embedded.zig:1392-1396) → Rust
  `#[cfg(any(target_os="macos", target_os="ios"))]` on the 6 Darwin exports (iOS-in-scope lock
  widens from macos-only).
- **Conditional platform types** `Platform.MacOS/IOS = if isDarwin() struct{…} else void`
  (:351-359) → `#[cfg]` type alias to a ZST off-Darwin.
- **`performAction`/`performPreAction`** `comptime action: apprt.Action.Key` generic dispatch
  (:267, :270, :289) → at each C export the action is a *fixed* tag (e.g. `ghostty_surface_split` ⇒
  `.new_split` :1929), so the generic **collapses per export**; the port hand-writes one call site
  per fixed action — no runtime action generic needed at the boundary.
- **`Action.CValue` reflection union** `@Type(.{ .@"union" = … })` built from `Key` (action.zig:425-451)
  → hand-write the `#[repr(C)] union ghostty_action_u` to match ghostty.h:954-993 exactly (I9).
- **`String.fromSlice` comptime sentinel detection** `@typeInfo` branch on `[:0]` vs `[]`
  (main_c.zig:74-92) → two Rust constructors (`from_slice` / `from_slice_sentinel`) or a
  const-generic `SENTINEL: bool`.
- **`ghostty_config_get`** `@setEvalBranchQuota(10_000)` + `stringToEnum(Key)` + `c_get.get`
  comptime key switch (CApi.zig:99-101) → already ported (rustty-config C3); the export marshals to
  it.
- **`ghostty_info`** build_mode/version from `build_config` comptime (main_c.zig:131-141) → Rust
  `const`/`build.rs`-emitted version.
- **Header-parity comptime tests** `checkGhosttyHEnum(Key, "GHOSTTY_TARGET_")` etc. (action.zig:24)
  → replaced by cbindgen header-diff + Rust `const` size/align/ordinal asserts (I9).
- **`DllMain`** windows-only comptime (main_c.zig:174-207) → `#[cfg(windows)]`; **out of scope**
  for the macOS/iOS tier (noted for completeness).
- **`ghostty_set_window_background_blur`** `comptime { if (os.tag != .macos) return; }`
  (:2113) + private CGS externs (:2129-2130) → `#[cfg(target_os="macos")]` objc/CGS leaf; NOT in
  the runtime-safe set (EMBEDDED_BLUEPRINT ADV-FIX 3).

---

## 6. Constants list (ABI-load-bearing; must be preserved bit/ordinal-exact)

- `GHOSTTY_SUCCESS = 0` (ghostty.h:29); `ghostty_init` returns `0` on success / `1` on failure
  (main_c.zig:105-115).
- `PlatformTag: macos = 1, ios = 2`; **`0` reserved = INVALID sentinel** (embedded.zig:394-400).
- `CAPI.Point.Tag: active=0, viewport=1, screen=2, history=3` (embedded.zig:1315-1320).
- `CAPI.Point.CoordTag: exact=0, top_left=1, bottom_right=2` (:1322-1326).
- `ColorScheme: light=0, dark=1` (structs.zig:96-99 / ghostty.h:117-120).
- `ClipboardRequestType: paste=0, osc_52_read=1, osc_52_write=2` (structs.zig:66-70).
- `Info.BuildMode: debug=0, release_safe=1, release_fast=2, release_small=3` (main_c.zig:54-59 /
  ghostty.h:385-390).
- `Target.Key: app=0, surface=1` (action.zig:20-22 / ghostty.h:555-558).
- `Action.Key` — the full positional enum, `quit=0 … copy_title_to_clipboard=last`
  (action.zig:351 / ghostty.h:885-952); **append-only** ordering is the ABI (action.zig:65-77).
- `input.Mods` bit positions incl. RIGHT variants at bits 6-9 (ghostty.h:127-139).
- Surface default size `{ .width = 800, .height = 600 }` (embedded.zig:477); initial cursor
  `{ .x = -1, .y = -1 }` (:478).
- Inspector default content_scale `1`, delta-time floor `1.0/60.0` and `@max(0.00001, …)`
  (:1000, :1238, :1237); imgui trackpad scroll scale `0.1`/`1.0` (:1169).
- `String.empty = { null, 0, false }` (main_c.zig:68-72); Text no-viewport fallback
  `{ tl_px_x=-1, tl_px_y=-1, 0, 0 }` (:1661-1666).

---

## 7. Risk notes

- **R-A (verify, cite both sides): `GHOSTTY_POINT_SURFACE` ↔ `history` name-skew — preserve, don't
  fix.** ghostty.h:421 declares `GHOSTTY_POINT_SURFACE` as ordinal 3, but the Zig `CAPI.Point.Tag`
  uses `history = 3` (embedded.zig:1319) and `pin` resolves `@field(terminal.point.Tag,
  @tagName(tag))` → `terminal.point.Tag.history` (point.zig:49,57 — the enum has `history`, no
  `surface`). So the C name `SURFACE` and the Zig name `history` are the **same ordinal 3**; the
  port must keep `GHOSTTY_POINT_SURFACE = 3` in the generated header (byte-identity for Swift) while
  the Rust enum arm maps to core `history`. **Do not "correct" the header name** — it is the frozen
  ABI. Flagged because a naive port could rename the arm and break the upstream-header diff gate.
- **R-B — header typo `chostty_ipc_target_s`.** ghostty.h:1043 spells the IPC target struct
  `chostty_ipc_target_s` (leading `c`). It is upstream and unused by the app link path, but the
  header-diff gate must reproduce the typo verbatim (or the diff never converges). Cosmetic; noted
  so the port doesn't "fix" it.
- **R-C — `timetime_ms` typo.** `ghostty_surface_message_childexited_s.timetime_ms` (ghostty.h:834)
  — same "preserve verbatim" note.
- **R-D — mutex poisoning (Group M, §4).** The only *new* failure mode with no Zig analog; the port
  must make an explicit, documented choice (abort-on-poison recommended). Not a bug in the
  reference — a Rust-porting hazard.
- **R-E — self-referential pin (I2) is the sharpest UB trap.** If the `embedded::Surface` is moved
  after `core_surface.init` writes `rt_surface`, every subsequent action callback ships a dangling
  `target.surface` to Swift. Enforce with `Pin`/heap-box + a Miri harness that constructs a surface,
  moves the box, and checks `rt_surface` (mirrors the shaper-hook and W5 SB/TB proof discipline in
  memory).
- **R-F — thread-affine drops (I5/FFI rows).** `nsview`/`ig_ctx`/Metal `backend`/CTFont are
  `!Send`; a `Drop` running on a non-main thread is UB. The port's types must be `!Send` (raw ptr /
  `objc2` / `core-foundation` non-Send handles) so the compiler forbids sending them to the render
  thread; do **not** paper over with `unsafe impl Send`.
- **R-G — overflow-checks dependency (M6).** This tier's §4 abort-parity *requires*
  `overflow-checks = true` on the shipped profile (else ReleaseSafe traps become silent wraps at the
  boundary). Not re-filing M6 — recording that macos-swift-app is a consumer of its fix.
- **R-H — symbol-manifest completeness.** Whichever symbol branch (§3) ships, the alias/export
  manifest must enumerate all 90 app names *and* be diff-gated against a fresh
  `nm`/`grep -c '^GHOSTTY_API'` of the pinned header, so an upstream re-sync that adds an export is
  caught (append-only ABI, I9).
- **R-I — decision-#1 lock vs "open" tension (§3).** Reconcile the memory's user-lock (Branch A)
  against the tasking's "open" framing before the port pass; the pre-port is robust either way, but
  the alias-generator + one-vs-two-headers build shape can't be finalized until it's settled.
