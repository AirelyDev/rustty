//! Isolated, non-FFI Miri harness that faithfully models the 3b borrow structure:
//! the run-iterator/hook/shape/add_glyph interleave (rebuild_row) and the
//! add_cursor raw-into-self alias (rebuild_cells). Each candidate is selected by
//! argv[1] so Miri only interprets one executed path. Goal: find the structure
//! that is clean under BOTH Stacked Borrows and Tree Borrows.
#![allow(dead_code)]

type Cell = u32;

#[derive(Clone, Copy)]
struct Run {
    offset: usize,
    cells: usize,
}

// ---- Shaper (models rustty_font_shaper::coretext/harfbuzz::Shaper) ----
struct Shaper {
    cell_buf: Vec<Cell>, // shape() returns a slice borrowing this
    tick: u32,
}
impl Shaper {
    fn new() -> Self {
        Shaper { cell_buf: vec![0; 8], tick: 0 }
    }
    // pub fn run_iterator(&mut self, ...) -> RunIterator { hooks: Hook{shaper:self}, .. }
    fn run_iterator(&mut self) -> RunIterator {
        RunIterator { hooks: Hook { shaper: self as *mut Shaper }, i: 0 }
    }
    // pub fn shape(&mut self, run) -> &[Cell] : mutates internal buffers, returns a
    // slice borrowing self.cell_buf ("valid until the next shape call").
    fn shape(&mut self, _run: Run) -> &[Cell] {
        self.tick = self.tick.wrapping_add(1);
        self.cell_buf[0] = self.tick;
        &self.cell_buf[0..2]
    }
}

// RunIteratorHook { pub shaper: *mut Shaper }
struct Hook {
    shaper: *mut Shaper,
}
// RunIterator<'a, H> { pub hooks: H, .. }
struct RunIterator {
    hooks: Hook,
    i: usize,
}
impl RunIterator {
    // next(&mut self, alloc) reads/writes the Shaper THROUGH the hook's raw ptr
    // (the reference next() calls hooks.prepare()/add_codepoint which deref *shaper).
    fn next(&mut self) -> Option<Run> {
        if self.i >= 3 {
            return None;
        }
        let sh: &mut Shaper = unsafe { &mut *self.hooks.shaper };
        sh.tick = sh.tick.wrapping_add(10); // touch the shaper through the hook
        let r = Run { offset: self.i * 2, cells: 2 };
        self.i += 1;
        Some(r)
    }
}

// ---- Renderer (models generic.rs Renderer) ----
struct Cursor {
    vp: u32,
}
struct State {
    cursor: Cursor,
}
struct Grid {
    counter: u32,
}
struct Config {
    thicken: bool,
    strength: u8,
}
struct Renderer {
    font_shaper: Shaper,
    cells: Vec<u8>,       // Contents
    state: State,         // terminal_state
    focused: bool,
    font_grid: *mut Grid, // shared grid handle (separate allocation)
    config: Config,
    grid_metrics: u32,
}
impl Renderer {
    fn new() -> Self {
        Renderer {
            font_shaper: Shaper::new(),
            cells: Vec::new(),
            state: State { cursor: Cursor { vp: 7 } },
            focused: true,
            font_grid: Box::into_raw(Box::new(Grid { counter: 0 })),
            config: Config { thicken: true, strength: 128 },
            grid_metrics: 42,
        }
    }

    // ============ CANDIDATE A: current 3b structure (expect UB) ============
    fn rebuild_row_current(&mut self) {
        let mut it = self.font_shaper.run_iterator(); // hook = &mut self.font_shaper as *mut
        let mut run = it.next();
        while let Some(r) = run {
            // shape via a fresh &mut self.font_shaper (SIBLING reborrow of the hook)
            let cells_ptr: *const [Cell] = self.font_shaper.shape(r) as *const [Cell];
            // add_glyph via &mut *self (whole-renderer reborrow)
            self.add_glyph_mut(unsafe { &*cells_ptr });
            run = it.next(); // re-deref hook -> UB if the hook tag was popped
        }
    }
    fn add_glyph_mut(&mut self, cells: &[Cell]) {
        self.cells.push(cells[0] as u8);
    }

    // ==== CANDIDATE B: raw self_ptr + shape via hook + raw-self add_glyph ====
    fn rebuild_row_rawself(&mut self) {
        let self_raw: *mut Renderer = self;
        // create the iterator via a narrow reborrow of font_shaper through self_raw
        let mut it = unsafe { (*self_raw).font_shaper.run_iterator() };
        let mut run = it.next();
        while let Some(r) = run {
            // shape THROUGH the hook's own raw ptr (child reborrow of the hook,
            // not a sibling that pops it)
            let cells_ptr: *const [Cell] =
                unsafe { (*it.hooks.shaper).shape(r) } as *const [Cell];
            // add_glyph via raw self with a NARROW field reborrow (never &mut *self_raw whole)
            unsafe { Renderer::add_glyph_raw(self_raw, &*cells_ptr) };
            run = it.next();
        }
    }
    unsafe fn add_glyph_raw(self_raw: *mut Renderer, cells: &[Cell]) {
        unsafe {
            (*self_raw).cells.push(cells[0] as u8);
        }
    }

    // ==== CANDIDATE C: mirror the shaper-crate idiom: raw *mut Shaper, shape
    // via (*sp), add_glyph &mut self (tests whether that idiom alone is clean) ====
    fn rebuild_row_shaper_idiom(&mut self) {
        let sp: *mut Shaper = &mut self.font_shaper;
        let mut it = unsafe { (*sp).run_iterator() };
        let mut run = it.next();
        while let Some(r) = run {
            let cells_ptr: *const [Cell] = unsafe { (*sp).shape(r) } as *const [Cell];
            self.add_glyph_mut(unsafe { &*cells_ptr });
            run = it.next();
        }
    }

    // ==== CANDIDATE D: raw self_ptr + shape via hook + raw add_glyph, but
    // font_shaper accessed for shape via a SIBLING raw *mut Shaper (not the hook) ====
    fn rebuild_row_rawsibling(&mut self) {
        let self_raw: *mut Renderer = self;
        let sp: *mut Shaper = unsafe { &mut (*self_raw).font_shaper };
        let mut it = unsafe { (*sp).run_iterator() };
        let mut run = it.next();
        while let Some(r) = run {
            let cells_ptr: *const [Cell] = unsafe { (*sp).shape(r) } as *const [Cell];
            unsafe { Renderer::add_glyph_raw(self_raw, &*cells_ptr) };
            run = it.next();
        }
    }

    // ==== CANDIDATE E: MINIMAL SURGICAL fix — keep &mut self receiver on
    // rebuild_row; mix narrow `self.field` writes with hook-routed shape +
    // raw-self add_glyph; hold shaper_cells across add_glyph and next(). ====
    fn rebuild_row_surgical(&mut self) {
        // hook born via a narrow &mut self.font_shaper (transient)
        let mut it = self.font_shaper.run_iterator();
        let mut run = it.next();
        let mut shaper_cells: Option<*const [Cell]> = None;
        while let Some(r) = run {
            // narrow self.field write in the loop (different byte range from font_shaper)
            self.cells.push(0xAA);
            if shaper_cells.is_none() {
                // shape THROUGH the hook's own raw ptr; hold the returned slice
                let sc = unsafe { (*it.hooks.shaper).shape(r) } as *const [Cell];
                shaper_cells = Some(sc);
            }
            let cells: &[Cell] = unsafe { &*shaper_cells.unwrap() };
            // add_glyph via raw self (self as *mut, fresh each call) — narrow inside
            unsafe { Renderer::add_glyph_raw(self as *mut Renderer, cells) };
            // another narrow self.field write after add_glyph, before next()
            self.focused = !self.focused;
            shaper_cells = None; // nulled before next shape
            run = it.next(); // re-deref hook
        }
    }

    // ==== CANDIDATE F: PERSISTENT self_raw mixed with narrow self.X writes ====
    fn rebuild_row_persistent(&mut self) {
        let self_raw: *mut Renderer = self;
        let mut it = self.font_shaper.run_iterator();
        let mut run = it.next();
        while let Some(r) = run {
            self.cells.push(0xAA); // narrow self.X via the &mut self receiver (U0)
            let cells_ptr: *const [Cell] = unsafe { (*it.hooks.shaper).shape(r) } as *const [Cell];
            unsafe { Renderer::add_glyph_raw(self_raw, &*cells_ptr) }; // persistent self_raw
            run = it.next();
        }
    }

    // ==== CANDIDATE G: realistic add_glyph_raw — MANY narrow field accesses via
    // self_raw (font_grid deref to a separate alloc, config, grid_metrics, cells) +
    // shape via hook + narrow self.field writes in the loop. ====
    fn rebuild_row_realistic(&mut self) {
        let self_raw: *mut Renderer = self;
        let mut it = self.font_shaper.run_iterator();
        let mut run = it.next();
        let mut shaper_cells: Option<*const [Cell]> = None;
        while let Some(r) = run {
            self.cells.push(0xAA); // narrow self.X via receiver
            let _ = self.grid_metrics; // narrow self read via receiver
            if shaper_cells.is_none() {
                let sc = unsafe { (*it.hooks.shaper).shape(r) } as *const [Cell];
                shaper_cells = Some(sc);
            }
            let cells: &[Cell] = unsafe { &*shaper_cells.unwrap() };
            unsafe { Renderer::add_glyph_realistic(self_raw, cells) };
            shaper_cells = None;
            run = it.next();
        }
    }
    unsafe fn add_glyph_realistic(self_raw: *mut Renderer, cells: &[Cell]) {
        unsafe {
            // narrow read of the font_grid ptr field, then reborrow the GRID (separate alloc)
            let g: &mut Grid = &mut *(*self_raw).font_grid;
            g.counter = g.counter.wrapping_add(1);
            // narrow reads of config + grid_metrics
            let _ = (*self_raw).config.thicken;
            let _ = (*self_raw).config.strength;
            let _ = (*self_raw).grid_metrics;
            // narrow write to cells
            (*self_raw).cells.push(cells[0] as u8 ^ g.counter as u8);
        }
    }

    // ============ add_cursor candidates ============
    // A: current (expect SB UB): raw-into-self ptr popped by &mut *self receiver
    fn addcursor_current(&mut self) {
        // (a nested &mut *self earlier, e.g. rebuild_row, has run)
        let cursor_ptr: *const Cursor = &self.state.cursor;
        self.add_cursor_mut(cursor_ptr);
    }
    fn add_cursor_mut(&mut self, cursor_state: *const Cursor) {
        let v = unsafe { (*cursor_state).vp }; // use-after-pop under SB
        self.cells.push(v as u8);
    }

    // B: hoist the needed scalar by value before the call
    fn addcursor_hoist(&mut self) {
        let vp = self.state.cursor.vp;
        self.add_cursor_hoisted(vp);
    }
    fn add_cursor_hoisted(&mut self, vp: u32) {
        self.cells.push(vp as u8);
    }

    // C: raw self ptr (no &mut *self receiver reborrow)
    fn addcursor_rawself(&mut self) {
        let self_raw: *mut Renderer = self;
        let cursor_ptr: *const Cursor = unsafe { &(*self_raw).state.cursor };
        unsafe { Renderer::add_cursor_raw(self_raw, cursor_ptr) };
    }
    unsafe fn add_cursor_raw(self_raw: *mut Renderer, cursor_state: *const Cursor) {
        let v = unsafe { (*cursor_state).vp };
        unsafe {
            (*self_raw).cells.push(v as u8);
        }
    }
}

// ============================================================================
// Chunk 4: draw_frame + completion-trampoline aliasing model.
//
// Models the three GPU_RD_CHUNK4_DESIGN §5.2 hazards that do NOT appear in the 3b
// add_cursor pattern:
//   (H1) `frame: *mut FrameSt` (a raw root into `self.swap_chain.frames[i]`) is
//        re-deref'd across `&mut self` method calls (upload_background_image /
//        update_csu). A whole `&mut *self` reborrow pops the frame tag.
//   (H2) the completion trampoline derefs `self_ptr: *mut Rndr` (== self) to call
//        `&mut (*self).api.present(...)` (+ `&self frame_completed`) WHILE
//        draw_frame's raw roots (self_ptr, frame) are still live on the stack.
//   (mix) between creating the raw roots and firing the trampoline, the render-pass
//        steps take SHARED `&self` reborrows (self.shaders / self.images.draw(&self.api))
//        interleaved with the live self_ptr + frame raw roots (which are re-read as
//        (*frame).uniforms / (*frame).target). The candidate reproduces this shared-vs-
//        raw mix, not just `&mut`-only interleaving.
//
// Pass criterion: clean under BOTH -Zmiri-stacked-borrows and -Zmiri-tree-borrows.
// ============================================================================

use std::cell::Cell as ICell;

struct Tex {
    width: usize,
    data: Vec<u8>,
}
struct FrameSt {
    uniforms: Vec<u8>,
    cells_bg: Vec<u8>,
    fg_cells: Vec<u8>,
    grayscale: Tex,
    target: u32,
    grayscale_modified: usize,
}
struct Api {
    started: bool,
}
impl Api {
    fn draw_frame_start(&mut self) {
        self.started = true;
    }
    fn draw_frame_end(&mut self) {
        self.started = false;
    }
    fn present(&mut self, _sync: bool) {
        let _ = self.started;
    }
    fn surface_size(&self) -> (u32, u32) {
        (2, 2)
    }
    fn init_atlas_texture(&self) -> Tex {
        Tex { width: 4, data: vec![0u8; 16] }
    }
}
struct Sh {
    pipelines: u32,
    defunct: bool,
}
struct Im {
    count: u32,
}
impl Im {
    fn upload(&mut self, _api: &Api) {
        self.count = self.count.wrapping_add(1);
    }
    // draw is &self on image::State; reads &self + &api (both narrow renderer fields)
    fn draw(&self, _api: &Api) -> u32 {
        self.count
    }
}
struct GridAtlas {
    modified: usize,
    size: u32,
    data: Vec<u8>,
}
struct Grid2 {
    atlas_grayscale: GridAtlas,
}
struct Cells {
    bg: Vec<u8>,
    fg: Vec<Vec<u8>>,
}

struct Rndr2 {
    swap_chain: [FrameSt; 3],
    frame_index: usize,
    api: Api,
    shaders: Sh,
    images: Im,
    font_grid: *mut Grid2,
    health: ICell<u32>,
    sema: ICell<u32>,
    cells: Cells,
    cells_rebuilt: bool,
    other: u32,
    size_screen: u32,
}
impl Rndr2 {
    fn new() -> Self {
        let mk = || FrameSt {
            uniforms: Vec::new(),
            cells_bg: Vec::new(),
            fg_cells: Vec::new(),
            grayscale: Tex { width: 1, data: vec![0u8; 1] },
            target: 0,
            grayscale_modified: 0,
        };
        Rndr2 {
            swap_chain: [mk(), mk(), mk()],
            frame_index: 0,
            api: Api { started: false },
            shaders: Sh { pipelines: 7, defunct: false },
            images: Im { count: 0 },
            font_grid: Box::into_raw(Box::new(Grid2 {
                atlas_grayscale: GridAtlas { modified: 5, size: 4, data: vec![1u8; 16] },
            })),
            health: ICell::new(0),
            sema: ICell::new(3),
            cells: Cells { bg: vec![0u8; 4], fg: vec![vec![1u8, 2], vec![3u8]] },
            cells_rebuilt: true,
            other: 0,
            size_screen: 2,
        }
    }

    // whole-&mut-self methods (upload_background_image / update_csu / init_shaders)
    fn upload_background_image(&mut self) {
        self.other = self.other.wrapping_add(1);
    }
    fn update_csu(&mut self) {
        self.other = self.other.wrapping_add(2);
    }
    // whole-&self method (has_animations)
    fn has_animations(&self) -> bool {
        self.shaders.pipelines == 0
    }

    // frame_completed: reference `self: *Self` (raw). Ported &self; accesses are
    // interior-mutable (health Cell / sema Cell) so a narrow &self is faithful.
    fn frame_completed(&self, health: u32) {
        if self.health.get() != health {
            self.health.set(health);
        }
        self.sema.set(self.sema.get() + 1); // release_frame (post)
    }

    // sync_atlas_texture: reference `self: *const Self, atlas: *const Atlas,
    // texture: *Texture` — ALL raw. `texture` aliases into self.swap_chain, so a
    // `&self` receiver + `&mut Texture` arg would freeze-then-mutate the same bytes
    // (SB UB). Ported raw-self + raw-texture (the faithful Zig shape); it reborrows
    // ONLY self.api (narrow, disjoint from swap_chain) and writes *texture.
    unsafe fn sync_atlas_texture_raw(
        self_raw: *const Rndr2,
        atlas: *const GridAtlas,
        texture: *mut Tex,
    ) {
        unsafe {
            if (*atlas).size as usize > (*texture).width {
                // init_atlas_texture reads only self.api (narrow &self.api)
                *texture = (*self_raw).api.init_atlas_texture();
            }
            // replace_region: copy atlas.data into texture.data
            (*texture).data.clear();
            (*texture).data.extend_from_slice(&(*atlas).data);
        }
    }

    // A `&self`-RECEIVER atlas sync (the design §6.3 literal signature) writing a
    // `*mut Tex` that aliases into self.swap_chain — EVIDENCE for whether the `&self`
    // form is SB-clean or whether raw-self is required.
    fn sync_atlas_self(&self, atlas: *const GridAtlas, texture: *mut Tex) {
        unsafe {
            if (*atlas).size as usize > (*texture).width {
                *texture = self.api.init_atlas_texture();
            }
            (*texture).data.clear();
            (*texture).data.extend_from_slice(&(*atlas).data);
        }
    }
    // Isolated: `&self` receiver whole-freeze + `&mut`-into-self texture write.
    fn atlas_self_isolated(&mut self) {
        let self_ptr: *mut Rndr2 = self;
        unsafe {
            let idx = 1usize;
            let frame = std::ptr::addr_of_mut!((*self_ptr).swap_chain[idx]);
            let atlas: *const GridAtlas = &(*(*self_ptr).font_grid).atlas_grayscale;
            (*self_ptr).sync_atlas_self(atlas, std::ptr::addr_of_mut!((*frame).grayscale));
            let _ = (*frame).grayscale.data.len();
        }
    }

    // ---- SHIPPED draw_frame structure (single raw root + re-derived frame) ----
    fn draw_frame_shipped(&mut self, sync: bool) {
        let self_ptr: *mut Rndr2 = self;
        unsafe {
            (*self_ptr).api.draw_frame_start(); // narrow &mut api
            let (w, h) = (*self_ptr).api.surface_size(); // narrow &api
            if w == 0 || h == 0 {
                return;
            }
            let size_changed = (*self_ptr).size_screen != w;
            let needs_redraw =
                size_changed || (*self_ptr).cells_rebuilt || (*self_ptr).has_animations() || sync;
            if !needs_redraw {
                return;
            }
            (*self_ptr).cells_rebuilt = false; // narrow &mut field

            // next_frame side effects: sema wait + index bump; keep only the index.
            (*self_ptr).sema.set((*self_ptr).sema.get() - 1);
            (*self_ptr).frame_index = ((*self_ptr).frame_index + 1) % 3;
            let idx = (*self_ptr).frame_index;

            // target-resize: fresh narrow frame reborrow + &self.api
            {
                let frame = std::ptr::addr_of_mut!((*self_ptr).swap_chain[idx]);
                if (*frame).target != (*self_ptr).size_screen {
                    (*frame).target = (*self_ptr).size_screen;
                }
            }

            // images.upload: narrow &mut self.images + narrow &self.api (disjoint fields)
            (*self_ptr).images.upload(&(*self_ptr).api);

            // WHOLE &mut self calls — these pop any live narrow frame tag, so frame
            // is re-derived AFTER them (the "reborrow-per-cluster" idiom).
            (*self_ptr).upload_background_image();
            (*self_ptr).update_csu();

            // Re-derive the frame raw root fresh, hold it through the sync/atlas/
            // render-pass/complete region (which takes ONLY narrow field reborrows).
            let frame = std::ptr::addr_of_mut!((*self_ptr).swap_chain[idx]);

            // syncs (serialization boundary): re-read frame + narrow &self.cells
            (*frame).uniforms.clear();
            (*frame).uniforms.push(0xAB);
            (*frame).cells_bg.clear();
            (*frame).cells_bg.extend_from_slice(&(*self_ptr).cells.bg);
            let flat: Vec<u8> = (*self_ptr).cells.fg.iter().flatten().copied().collect();
            (*frame).fg_cells.clear();
            (*frame).fg_cells.extend_from_slice(&flat);

            // atlas sync: atlas is in *font_grid (separate alloc); sync via raw self.
            let atlas: *const GridAtlas = &(*(*self_ptr).font_grid).atlas_grayscale;
            if (*atlas).modified > (*frame).grayscale_modified {
                (*frame).grayscale_modified = (*atlas).modified;
                Rndr2::sync_atlas_texture_raw(self_ptr, atlas, &mut (*frame).grayscale);
            }

            // begin_frame: &self.api + type-erased self_ptr + raw addr of frame.target
            let ctx: *mut () = self_ptr as *mut ();
            let _target_ptr: *mut u32 = std::ptr::addr_of_mut!((*frame).target);

            // render pass: shared &self.shaders + self.images.draw(&self.api) mixed
            // with the live raw frame + self_ptr roots (re-read (*frame).*).
            let _ = (*self_ptr).shaders.pipelines; // narrow &self.shaders
            let _drawn = (*self_ptr).images.draw(&(*self_ptr).api); // &self.images + &self.api
            let _u = (*frame).uniforms.len(); // raw frame re-read
            let _c = (*frame).fg_cells.len();
            let _g = (*frame).grayscale.data.len();
            let _t = (*frame).target; // raw frame re-read

            // complete(sync): the SYNC path invokes the trampoline directly, while
            // self_ptr + frame raw roots are still live.
            trampoline2(ctx);
            let _ = sync;

            // frame is NOT used after complete (matches the reference: frame_ctx.complete
            // is the last frame use; drawFrameEnd/scrollbar/unlock touch only self).
            (*self_ptr).api.draw_frame_end(); // narrow &mut api
            (*self_ptr).other = (*self_ptr).other.wrapping_add(10); // scrollbar (narrow)
        }
    }

    // ---- NAIVE structure (expect UB): frame held (as a &mut-derived raw) across
    // the whole-&mut-self calls, then re-deref'd. ----
    fn draw_frame_naive(&mut self, sync: bool) {
        unsafe {
            self.api.draw_frame_start();
            let idx = 1usize;
            // frame derived from a &mut self reborrow, held across whole-self calls
            let frame: *mut FrameSt = &mut self.swap_chain[idx];
            self.upload_background_image(); // &mut *self — pops frame's tag
            self.update_csu();
            (*frame).uniforms.push(0xAB); // use-after-pop under SB
            let _ = sync;
            self.api.draw_frame_end();
        }
    }
}

// The completion trampoline (models metal_buffer_completed). ctx is the type-erased
// *mut Rndr2 (== self_ptr). Reborrows &mut (*r).api (present) + calls frame_completed
// (&self) — while the outer draw_frame raw roots are live.
unsafe fn trampoline2(ctx: *mut ()) {
    let r = ctx as *mut Rndr2;
    let health = 0u32; // buffer.status() -> healthy
    unsafe {
        if health == 0 {
            (*r).api.present(true); // narrow &mut api
        }
        (*r).frame_completed(health); // &self (whole shared, but frame unused after)
    }
}

// ============================================================================
// SEARCH_BINDING_ARM_DESIGN §3.4: the construct-then-spawn-then-produce shape.
//
// This is NOT the GPU cell-fill / draw-frame borrow structure — it is the
// Surface `.search` arm's DETACHED store-then-spawn-then-join-LATER lifetime:
//   * a `Search { state: T, thread: Option<JoinHandle>, mailbox: *const Mbx }`
//     is stored in a PINNED slot (a Box; the reference's `self.search.?` inline
//     field of the pinned Surface);
//   * the search thread runs `thread_main(&mut state)` via a raw `*mut T`
//     (`state_ptr`) held for its ENTIRE life — the handle is STORED and joined
//     LATER at deinit (NOT `std::thread::scope`, which the green vt test uses);
//   * AFTER the spawn returns, the MAIN thread PRODUCES to the running thread
//     (the `.search`-else push / `.navigate_search`) and, in the reverse
//     direction, the thread re-enters the surface via a `searchCallback`.
//
// Two directions, each a naive/shipped pair. A deterministic mpsc handshake
// sequences the two threads' borrow operations so the SB verdict is stable
// (Miri only checks executed borrow ops; the channels give happens-before, so
// there is no data race to mask the tag violation).
//
//   search_naive / search_shipped   — MAIN→child producer (§3.2)
//   callback_naive / callback_shipped — child→MAIN re-entry (§3.3a)
//
// Pass criterion (both shipped): CLEAN under BOTH -Zmiri-stacked-borrows and
// -Zmiri-tree-borrows. Only then do the core arm + the retyped callback land.
// ============================================================================

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering as AOrd};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex as SMutex};

// ---- MAIN→child producer model (search_naive / search_shipped) ----

// BlockingQueue<Message,64> analogue: internally synchronized, `push(&self)`.
struct Mbx {
    inner: SMutex<Vec<u32>>,
}
impl Mbx {
    fn push(&self, v: u32) {
        self.inner.lock().unwrap().push(v);
    }
    fn drain_len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

// `terminal.search.Thread` analogue: the child mutates `counter` through
// `&mut self` (== `state_ptr`) and drains its OWN mailbox (the `Box<Mbx>`
// pointee, a heap address OUTSIDE this struct's SB coverage).
struct Tstate {
    mailbox: Box<Mbx>,
    counter: u32,
}
impl Tstate {
    // thread_main's per-iteration work: a `&mut self` touch of `counter`
    // (the held-state mutation the naive main-thread reborrow would pop) PLUS a
    // drain of the mailbox — the real `thread_main_` loop does
    // `while let Some(m) = self.mailbox.pop()` (thread.rs:370/493), a SHARED
    // (`&self`) `pop` reached through the `Box<Mailbox>` while `&mut self` is held.
    fn tick(&mut self) {
        self.counter = self.counter.wrapping_add(1);
        let _ = self.mailbox.drain_len();
    }
    fn deinit(&mut self) {
        self.counter = 0;
    }
}

// `Surface.Search` analogue (§2.3): stored whole in a pinned Box.
struct Search {
    st: Tstate,
    thread: Option<std::thread::JoinHandle<()>>,
    // decoupled producer handle — the Box<Mbx> pointee (move-stable, outside
    // the `Tstate` struct's SB coverage). The shipped producer reaches the
    // running thread ONLY through this.
    mailbox: *const Mbx,
}

struct SendT(*mut Tstate);
unsafe impl Send for SendT {}

fn run_search(shipped: bool) {
    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel(); // main -> child
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel(); // child -> main

    // Build `state` as a LOCAL and move it whole into the pinned Search
    // (`thread: None` == the reference `.thread = undefined`).
    let st = Tstate {
        mailbox: Box::new(Mbx { inner: SMutex::new(Vec::new()) }),
        counter: 0,
    };
    // NOTE (harness finding — corrects design §2.4): the decoupled `*const Mbx`
    // must NOT be captured off the PRE-MOVE local `st.mailbox`
    // (`addr_of!(*st.mailbox)`). `Mailbox` is a `Box`, which asserts noalias; a
    // pointer with the pre-move Box provenance is INVALIDATED when the child
    // later derefs `self.mailbox` through its held `&mut self` (thread_main),
    // popping that provenance (→ SB+TB UB, proven by `search_naive`-style runs).
    // The decoupled pointer must instead be derived from the POST-MOVE
    // `state_ptr` lineage — the SAME lineage the child derefs the Box through —
    // so the two are compatible raw siblings. So `mailbox` is null at
    // construction and set from `state_ptr` below.
    let mut boxed: Box<Search> = Box::new(Search {
        st,
        thread: None,
        mailbox: core::ptr::null(),
    });

    // ONE transient &mut taken at birth, lowered immediately to raw (§2.4).
    let sp: *mut Search = &raw mut *boxed;
    let state_ptr: *mut Tstate = unsafe { &raw mut (*sp).st };
    // Derive the decoupled mailbox from the post-move `state_ptr` lineage (the
    // Box<Mbx> pointee, reached through the same provenance the child uses).
    unsafe { (*sp).mailbox = &raw const *(*state_ptr).mailbox }

    let send = SendT(state_ptr);
    let handle = std::thread::Builder::new()
        .name("search".into())
        .spawn(move || {
            let s = send;
            let go = go_rx;
            let ack = ack_tx;
            // `thread_main(&mut self)` takes `&mut self` ONCE and HOLDS it for the
            // whole thread life (the reference `threadMain(self: *Thread)` → the
            // loop). Model that: a single long-lived `&mut Tstate` reborrow of
            // `state_ptr`, reused across every phase.
            let st: &mut Tstate = unsafe { &mut *s.0 };
            // phase 1: first use of the held `&mut st`.
            let _ = go.recv();
            st.tick(); // held-&mut use #1
            ack.send(1).unwrap();
            // phase 2: reuse the SAME held `&mut st`, AFTER main's producer access.
            let _ = go.recv();
            st.tick(); // held-&mut use #2 — NAIVE: use-after-pop
            ack.send(2).unwrap();
            // phase 3: stop.
            let _ = go.recv();
        })
        .unwrap();
    // s.thread = handle — written through its OWN disjoint raw projection; no
    // &mut Search reborrow, so the child's live state_ptr tag is not popped.
    unsafe { (*sp).thread = Some(handle) }

    // let the child establish its tag (phase 1).
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();

    // PRODUCER (the `.search`-else push / `.navigate_search`), issued between the
    // child's two state uses.
    if shipped {
        // decoupled *const Mbx (Box pointee): a Copy raw field read + a deref of
        // memory OUTSIDE `st` — NO reborrow of `st`.
        let mailbox: *const Mbx = unsafe { (*sp).mailbox };
        unsafe { (*mailbox).push(42) }
    } else {
        // NAIVE: a whole `&mut Search` reborrow reaching `st.mailbox`
        // (`self.search.as_mut().unwrap().state.mailbox…`). The `&mut *sp` retag
        // pops the child's state_ptr Unique.
        let s: &mut Search = unsafe { &mut *sp };
        s.st.mailbox.push(42);
    }

    // release child phase 2 (touches state_ptr again → NAIVE use-after-pop).
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();

    // deinit (§3.3): release phase 3, join (child now dead → no alias), THEN
    // exclusive access to `st`.
    go_tx.send(0).unwrap();
    unsafe { (*sp).thread.take().unwrap().join().unwrap() }
    unsafe { (*sp).st.deinit() }

    let counter = unsafe { (*sp).st.counter };
    println!(
        "search_{}: counter={counter}",
        if shipped { "shipped" } else { "naive" }
    );
    drop(boxed);
}

// ---- child→MAIN re-entry model (callback_naive / callback_shipped) ----

// The renderer mailbox (external heap alloc, reached via a raw `*const`).
struct Ext {
    inner: SMutex<Vec<u32>>,
}
impl Ext {
    fn push(&self, v: u32) {
        self.inner.lock().unwrap().push(v);
    }
}

// `xev.Async` analogue: `notify(&self)` reads only `self.shared` (an Arc, heap-
// backed). Modeled as an INLINE field of the surface so `notify` forms a narrow
// `&(*sp).wakeup` shared ref INTO the surface (the real `renderer_thread.wakeup`).
struct Wakeup {
    shared: Arc<AtomicU32>,
}
impl Wakeup {
    fn notify(&self) {
        self.shared.fetch_add(1, AOrd::SeqCst);
    }
}

// `Surface` analogue for the callback direction.
struct S2 {
    field: u32,          // the main thread's `&mut self` (performBindingAction) touches this
    wakeup: Wakeup,      // inline (renderer_thread.wakeup)
    mailbox: *const Ext, // raw → external (renderer_thread.mailbox)
}
impl S2 {
    // NAIVE `searchCallback_(&mut self, …)`: a whole `&mut Surface` receiver.
    fn callback_naive_method(&mut self) {
        self.field = self.field.wrapping_add(100);
        unsafe { (*self.mailbox).push(7) }
        self.wakeup.notify();
    }
}

struct SendS(*mut S2);
unsafe impl Send for SendS {}

fn run_callback(shipped: bool) {
    let ext = Box::new(Ext { inner: SMutex::new(Vec::new()) });
    let ext_ptr: *const Ext = &*ext;
    let mut boxed: Box<S2> = Box::new(S2 {
        field: 0,
        wakeup: Wakeup { shared: Arc::new(AtomicU32::new(0)) },
        mailbox: ext_ptr,
    });
    let sp: *mut S2 = &raw mut *boxed;

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();

    let send = SendS(sp);
    let handle = std::thread::spawn(move || {
        let s = send;
        let go = go_rx;
        let ack = ack_tx;
        // fires WHILE the main thread is holding its &mut self (performBindingAction).
        let _ = go.recv();
        if shipped {
            // SHIPPED searchCallback_(self_: *mut Surface): narrow raw field
            // projections only — never a whole `&mut Surface`/`&Surface`.
            unsafe {
                let mb: *const Ext = (*s.0).mailbox; // Copy raw field read
                (*mb).push(7); // renderer_thread.mailbox.push (external)
                let wk: &Wakeup = &(*s.0).wakeup; // narrow &(*sp).renderer_thread.wakeup
                wk.notify();
            }
        } else {
            // NAIVE: `unsafe { &mut *self_ }.searchCallback_(event)` — a whole
            // `&mut Surface` reborrow, concurrent with the main thread's `&mut self`.
            unsafe { (*s.0).callback_naive_method() }
        }
        ack.send(1).unwrap();
        let _ = go.recv(); // stop
    });

    // MAIN: performBindingAction holds `&mut self` ACROSS the arm body.
    let main_ref: &mut S2 = unsafe { &mut *sp };
    main_ref.field = main_ref.field.wrapping_add(1); // arm body use #1
    // fire the callback WHILE main_ref is live.
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();
    // main CONTINUES using its `&mut self` after the callback.
    main_ref.field = main_ref.field.wrapping_add(1); // arm body use #2 — NAIVE: use-after-pop
    let out = main_ref.field;

    go_tx.send(0).unwrap();
    handle.join().unwrap();

    println!(
        "callback_{}: field={out}",
        if shipped { "shipped" } else { "naive" }
    );
    drop(boxed);
    drop(ext);
}

// ============================================================================
// W3 §2.2: renderer-Thread construct-then-spawn + xev-loop callback re-entry.
//
//   * A `RThread` is stored in a PINNED Box (the reference `self.renderer_thread`
//     field of the pinned Surface), lowered to raw `tp: *mut RThread` at birth.
//   * The child runs `thread_main(tp)` holding `tp` for its whole life; the
//     stubbed loop fires callbacks with the SAME raw `tp` (xev userdata).
//     Callbacks re-deref `tp` and call `(*renderer).draw_frame()` + drain the
//     external mailbox + narrow field writes.
//   * CONCURRENTLY the MAIN thread produces (wakeup.notify + mailbox.push) and,
//     at deinit, does stop.notify — touching ONLY Copy handles + the *const
//     mailbox + a narrow `&(*tp).stop`. Sequenced by mpsc for a stable verdict.
//
// naive: thread_main binds `let t = &mut *tp` (whole) held across the loop; a
//        callback reborrows `&mut *tp` (sibling) -> pops t; t is re-used after
//        -> use-after-pop. (Also: the main producer reborrows `&mut *tp` whole.)
// shipped: thread_main + callbacks use raw `tp` with NARROW field access only;
//          main uses narrow `&(*tp).wakeup/.stop` + `*const mailbox`.
//
// Pass: shipped CLEAN under BOTH -Zmiri-stacked-borrows and -Zmiri-tree-borrows;
//       naive reports UB under at least one.
// ============================================================================

struct RRenderer {
    draws: u32,
}
impl RRenderer {
    fn draw_frame(&mut self) {
        self.draws = self.draws.wrapping_add(1);
    }
    #[allow(dead_code)]
    fn has_vsync(&self) -> bool {
        false
    }
}

// xev.Async analogue (Copy handle; notify(&self) reads only an Arc). Inline
// fields of RThread == renderer_thread.wakeup/.stop/.draw_now.
#[derive(Clone, Copy)]
struct RAsync {
    shared: *const AtomicU32,
}
impl RAsync {
    fn notify(&self) {
        unsafe {
            (*self.shared).fetch_add(1, AOrd::SeqCst);
        }
    }
}

struct RThread {
    // xev handles (Copy) — inline; main touches wakeup/stop/draw_now only.
    wakeup: RAsync,
    stop: RAsync,
    draw_now: RAsync,
    // external mailbox (BlockingQueue) reached via *const (outside RThread's
    // SB/TB coverage) — the decoupled-producer lineage (search_shipped finding).
    mailbox: *const Ext,
    renderer: *mut RRenderer, // *rendererpkg.Renderer
    draw_active: bool,
    cursor_blink_visible: bool,
    visible: bool,
    field: u32, // stand-in for misc narrow field writes
}

// ---- callbacks: raw self_, NARROW field access (SHIPPED shape) ----
unsafe fn render_cb_shipped(self_: *mut RThread) {
    unsafe {
        if !(*self_).visible {
            return;
        }
        let r = (*self_).renderer; // narrow field read
        (*r).draw_frame(); // (*renderer).draw_frame()
        // drawFrame helper: narrow reads
        if !(*self_).visible {
            return;
        }
        let _ = (*(*self_).renderer).has_vsync();
        (*self_).field = (*self_).field.wrapping_add(1); // narrow write
    }
}
unsafe fn wakeup_cb_shipped(self_: *mut RThread) {
    unsafe {
        // drainMailbox: pop the EXTERNAL mailbox via *const (narrow copy of the ptr)
        let mb: *const Ext = (*self_).mailbox;
        (*mb).push(0); // model a drain touch
        render_cb_shipped(self_); // renderCallback(t, undefined, …)
    }
}
unsafe fn cursor_cb_shipped(self_: *mut RThread) {
    unsafe {
        (*self_).cursor_blink_visible = !(*self_).cursor_blink_visible; // narrow
        (*self_).wakeup.notify(); // Copy handle
    }
}

// ---- callbacks: whole &mut *self_ reborrow (NAIVE shape) ----
unsafe fn wakeup_cb_naive(self_: *mut RThread) {
    unsafe {
        let t: &mut RThread = &mut *self_; // SIBLING whole reborrow -> pops outer
        let mb = t.mailbox;
        (*mb).push(0);
        if t.visible {
            (*t.renderer).draw_frame();
            t.field = t.field.wrapping_add(1);
        }
    }
}

struct SendRT(*mut RThread);
unsafe impl Send for SendRT {}

fn run_renderthread(shipped: bool) {
    let wake = Arc::new(AtomicU32::new(0));
    let stopf = Arc::new(AtomicU32::new(0));
    let dnow = Arc::new(AtomicU32::new(0));
    let ext = Box::new(Ext { inner: SMutex::new(Vec::new()) });
    let ext_ptr: *const Ext = &*ext;
    let mut rndr = Box::new(RRenderer { draws: 0 });
    let rndr_ptr: *mut RRenderer = &raw mut *rndr;

    let mut boxed: Box<RThread> = Box::new(RThread {
        wakeup: RAsync { shared: Arc::as_ptr(&wake) },
        stop: RAsync { shared: Arc::as_ptr(&stopf) },
        draw_now: RAsync { shared: Arc::as_ptr(&dnow) },
        mailbox: ext_ptr,
        renderer: rndr_ptr,
        draw_active: false,
        cursor_blink_visible: false,
        visible: true,
        field: 0,
    });
    // ONE transient &mut at birth, lowered to raw (§2.4 / search_shipped).
    let tp: *mut RThread = &raw mut *boxed;

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel(); // main -> child ticks
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();

    let send = SendRT(tp);
    let handle = std::thread::Builder::new()
        .name("renderer".into())
        .spawn(move || {
            let s = send;
            let go = go_rx;
            let ack = ack_tx;
            unsafe {
                if shipped {
                    // thread_main(self_: *mut RThread): NO whole reborrow. Loop the
                    // stubbed xev: each tick fires a callback with the raw userdata.
                    loop {
                        match go.recv() {
                            Ok(0) => wakeup_cb_shipped(s.0),
                            Ok(1) => cursor_cb_shipped(s.0),
                            _ => break,
                        }
                        // (No DCE-defeat write here: the callbacks already mutate
                        // observable state — renderer.draws, mailbox, (*self_).field —
                        // so DCE cannot elide them. A raw `(s.0 as *mut u8)` byte write
                        // would clobber RThread's first field (`wakeup: RAsync`), a
                        // mis-narrowed access that corrupts the very Copy handle the
                        // shipped shape is proving CLEAN — deliberately omitted.)
                        ack.send(1).unwrap();
                    }
                } else {
                    // NAIVE: whole &mut held across the loop; callback reborrows &mut
                    // *s.0 (sibling) -> pops t; t re-used after -> use-after-pop.
                    let t: &mut RThread = &mut *s.0;
                    loop {
                        match go.recv() {
                            Ok(0) => {
                                wakeup_cb_naive(s.0); // pops t
                                t.field = t.field.wrapping_add(1); // UAP
                            }
                            Ok(1) => {
                                t.cursor_blink_visible = !t.cursor_blink_visible;
                            }
                            _ => break,
                        }
                        ack.send(1).unwrap();
                    }
                }
            }
        })
        .unwrap();

    // let the child arm (establish its tag) — first tick.
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();

    // MAIN producer (steady state) — between child ticks.
    if shipped {
        let wk: RAsync = unsafe { (*tp).wakeup }; // Copy handle read (narrow field copy)
        wk.notify();
        let mb: *const Ext = unsafe { (*tp).mailbox }; // Copy *const read
        unsafe { (*mb).push(42) };
    } else {
        let m: &mut RThread = unsafe { &mut *tp }; // whole reborrow -> pops child tag
        m.field = m.field.wrapping_add(1);
        unsafe { (*m.mailbox).push(42) };
    }
    go_tx.send(1).unwrap();
    let _ = ack_rx.recv(); // cursor tick

    // DEINIT: stop.notify via NARROW shared &(*tp).stop, then join (child dead).
    if shipped {
        let st: &RAsync = unsafe { &(*tp).stop }; // narrow shared into pinned struct
        st.notify();
    } else {
        let m: &mut RThread = unsafe { &mut *tp }; // whole reborrow (naive)
        m.stop.notify();
    }
    go_tx.send(2).unwrap(); // stop tick -> loop break
    handle.join().unwrap();

    let draws = unsafe { (*(*tp).renderer).draws };
    println!(
        "renderthread_{}: draws={draws} field={}",
        if shipped { "shipped" } else { "naive" },
        unsafe { (*tp).field }
    );
    let _ = unsafe { (*tp).draw_now }; // touch draw_now so the field is not dead
    let _ = unsafe { (*tp).draw_active };
    drop(boxed);
    drop(rndr);
    drop(ext);
    let _ = (wake, stopf, dnow);
}

// ============================================================================
// W5_DESIGN §6: the `Metal::loop_enter` display-callback trampoline aliasing model.
//
// Models the LAST live-windowing seam: the CA `display` method (main thread) fires a
// display callback that recovers `*mut Renderer` via container_of (@fieldParentPtr)
// and calls the RAW-self draw path, CONCURRENTLY with the render thread's own draw —
// both serialised by `draw_mutex`. The crux W5 must resolve: is a whole-`Renderer`
// `&mut` reborrow through the raw ctx sound, or must BOTH concurrent draw callers be
// raw-self? This pair answers it under Miri (not hand-reasoned).
//
//   displaycb_shipped — BOTH concurrent callers raw-self (the SHIPPED §3.4/§3.5 shape):
//     * render thread reaches draw via `&raw mut (*(*owner).renderer).0` (a raw place
//       projection off the already-raw renderer handle; renderer_seam.rs:1208 post-fix);
//     * display callback reaches draw via the container_of round-trip
//       (`&raw mut (*rp).api` → byte_sub(offset_of!) → *mut c_void in a Cell → back →
//       deref ALL of Renderer), the §2.4 provenance round-trip;
//     * draw_frame_raw locks draw_mutex, does a NARROW plain-field write (models
//       cells_rebuilt) + a nested completion re-entry (models metal_buffer_completed),
//       unlocks. NO whole-Renderer `&mut` protector ever spans either call.
//     Expected: CLEAN under BOTH SB and TB.
//
//   displaycb_naive — the REJECTED / pre-fix shape (the UB control that makes the
//     raw-self split load-bearing, not cosmetic): the render thread draws through the
//     `&mut self` `draw_frame` WRAPPER (a protected whole-`Renderer` `&mut` function
//     argument, exactly renderer_seam.rs:1208 PRE-fix), CONCURRENTLY with the raw
//     display callback's draw_frame_raw + its plain-field write. One protected `&mut`
//     receiver racing a raw foreign draw. The `draw_mutex` cannot rescue it — the
//     protector is established at the wrapper's entry, BEFORE the body reaches lock().
//     Expected: UB under BOTH SB and TB.
//
// A deterministic mpsc handshake parks the render thread INSIDE its draw call (so its
// borrow context — the protected `&mut` in the naive case — is live across the display
// callback's foreign access) and orders the two field writes (lock/unlock + channel
// happens-before ⇒ no data race to mask the borrow verdict). The layer's display-ctx
// Cell is main-thread-only (faithful: both setDisplayCallback and CA `display` fire on
// the main thread in the reference), so the container_of ctx never crosses threads.
//
// Pass criterion (recorded in README.md "W5 candidates"): displaycb_shipped CLEAN
// under BOTH -Zmiri-stacked-borrows and -Zmiri-tree-borrows; displaycb_naive UB under
// both — proving the raw-self split of BOTH concurrent draw callers is load-bearing.
// ============================================================================

use core::ffi::c_void;

// FakeApi models `Renderer.api : Metal` — the container_of target field.
struct FakeApi {
    touched: u32, // written by the completion re-entry (narrow &mut api)
}

// FakeRenderer models `generic::Renderer`. `draw_mutex` + `lead` sit BEFORE `api` so
// `offset_of!(FakeRenderer, api) != 0` — the byte_sub container_of round-trip is a real
// non-trivial +offset/−offset (§2.4), not an identity.
struct FakeRenderer {
    draw_mutex: SMutex<()>,
    lead: u64,
    api: FakeApi,
    field: u64,     // plain field written under draw_mutex (models `cells_rebuilt`)
    completed: u32, // written by the completion re-entry
}
impl FakeRenderer {
    fn new() -> Self {
        FakeRenderer {
            draw_mutex: SMutex::new(()),
            lead: 0,
            api: FakeApi { touched: 0 },
            field: 0,
            completed: 0,
        }
    }
}

// `core::Renderer(pub rustty_renderer::Renderer)` newtype — the render-thread lineage
// reaches the inner renderer via `&raw mut (*(*owner).renderer).0` (renderer_seam.rs).
struct RNewtype(FakeRenderer);

// The IOSurfaceLayer's stored display ctx: `Cell<*mut c_void>` (ptr-in-ptr, provenance
// preserved across the store/load — NOT an int round-trip). Main-thread-only in the
// model, exactly as in the reference where both setDisplayCallback and CA `display`
// fire on the main thread. A SEPARATE allocation from the Renderer (the ObjC layer).
struct FakeLayer {
    display_ctx: ICell<*mut c_void>,
}

// The completion trampoline (models metal_buffer_completed): recovers the renderer as a
// RAW *mut and takes only NARROW field reborrows (never a whole `&mut`/`&` self).
unsafe fn displaycb_buffer_completed(self_ptr: *mut FakeRenderer) {
    unsafe {
        (*self_ptr).api.touched = (*self_ptr).api.touched.wrapping_add(1); // narrow &mut api
        (*self_ptr).completed = (*self_ptr).completed.wrapping_add(1); // narrow &mut field
    }
}

// draw_frame_raw: the RAW-self draw entry, faithful to Zig `drawFrame(self: *Renderer)`.
// Locks draw_mutex, does the narrow field mutations + the completion re-entry, unlocks.
// BOTH concurrent callers invoke THIS through a raw *mut in the shipped shape.
unsafe fn displaycb_draw_frame_raw(self_ptr: *mut FakeRenderer) {
    // self.draw_mutex.lock() / defer unlock (RAII guard). The ONLY pre-lock access is a
    // SHARED borrow of the `Sync` mutex field — no `&mut` yet.
    let _g = unsafe { (*self_ptr).draw_mutex.lock().unwrap() };
    // narrow plain-field write (models `cells_rebuilt = false`) — the racing target.
    unsafe { (*self_ptr).field = (*self_ptr).field.wrapping_add(1) };
    // nested completion re-entry (transient, per-field access; mirrors the real body).
    unsafe { displaycb_buffer_completed(self_ptr) };
    // guard drops here → unlock
}

// The `&mut self` `draw_frame` WRAPPER (the pre-fix render-thread path). `this` is a
// protected whole-`Renderer` `&mut` FUNCTION ARGUMENT for the entire call — the exact
// shape §2.2 shows is unsound under concurrency. The handshake parks INSIDE this call
// so the protector is live across the display callback's foreign draw.
fn displaycb_draw_frame_mut(this: &mut FakeRenderer, go: &Receiver<u8>, ack: &Sender<u8>) {
    let self_ptr: *mut FakeRenderer = this;
    unsafe { displaycb_draw_frame_raw(self_ptr) };
    // Still INSIDE the `&mut self` call → `this`'s protector is LIVE. Hand off to the
    // main-thread display callback, which does its foreign raw draw while we park.
    ack.send(1).unwrap();
    go.recv().unwrap();
    // Re-use the protected `&mut` lineage after the foreign access (use-after-pop under
    // the naive shape). `this` is re-read below so its protector spans to here.
    unsafe { (*self_ptr).field = (*self_ptr).field.wrapping_add(1) };
    let _keep: &mut FakeRenderer = this;
    let _ = &*_keep;
}

// loopEnter(self: *Metal): container_of → *Renderer; layer.setDisplayCallback(.., renderer).
// The byte_sub keeps `api_ptr`'s whole-allocation provenance (§2.4) so the stored ctx
// can later deref ALL of Renderer.
unsafe fn displaycb_loop_enter(api_ptr: *mut FakeApi, layer: &FakeLayer) {
    let renderer: *mut FakeRenderer = unsafe {
        (api_ptr as *mut u8)
            .byte_sub(core::mem::offset_of!(FakeRenderer, api))
            .cast::<FakeRenderer>()
    };
    layer.display_ctx.set(renderer as *mut c_void);
}

// The CA `display` method → displayCallback(renderer): recover the ctx, RAW-self draw.
// NEVER `(*renderer).draw_frame(&mut self)` (which would form the protected whole-
// Renderer `&mut` that races the render thread).
unsafe fn displaycb_display_callback(layer: &FakeLayer) {
    let ctx: *mut c_void = layer.display_ctx.get();
    let renderer: *mut FakeRenderer = ctx.cast::<FakeRenderer>();
    unsafe { displaycb_draw_frame_raw(renderer) };
}

struct SendRN(*mut RNewtype);
unsafe impl Send for SendRN {}

fn run_displaycb(naive: bool) {
    let mut owner: Box<RNewtype> = Box::new(RNewtype(FakeRenderer::new()));
    let owner_ptr: *mut RNewtype = &raw mut *owner;
    let rp: *mut FakeRenderer = unsafe { &raw mut (*owner_ptr).0 };

    // The ObjC layer (a SEPARATE allocation), touched only on the main thread.
    let layer = Box::new(FakeLayer { display_ctx: ICell::new(core::ptr::null_mut()) });

    // loopEnter on the main thread: install the container_of ctx into the layer.
    let api_ptr: *mut FakeApi = unsafe { &raw mut (*rp).api };
    unsafe { displaycb_loop_enter(api_ptr, &layer) };

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel(); // main → render thread
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel(); // render thread → main

    // The render thread (the concurrent draw caller that races the display callback).
    let send = SendRN(owner_ptr);
    let handle = std::thread::Builder::new()
        .name("render".into())
        .spawn(move || {
            let s = send;
            let owner_ptr = s.0;
            let go = go_rx;
            let ack = ack_tx;
            if naive {
                // PRE-FIX: draw through the `&mut self` wrapper — a protected whole-
                // `Renderer` `&mut` (renderer_seam.rs:1208 pre-fix). Parks inside.
                let this: &mut FakeRenderer = unsafe { &mut (*owner_ptr).0 };
                displaycb_draw_frame_mut(this, &go, &ack);
            } else {
                // SHIPPED: raw-self draw via `&raw mut (*(*owner).renderer).0` — a raw
                // place projection, NO protector (renderer_seam.rs:1208 post-fix).
                let self_ptr: *mut FakeRenderer = unsafe { &raw mut (*owner_ptr).0 };
                unsafe { displaycb_draw_frame_raw(self_ptr) };
                // Park so the two draws temporally overlap (still raw — no reuse of the
                // raw across the display callback's own draw is needed for soundness).
                ack.send(1).unwrap();
                go.recv().unwrap();
            }
        })
        .unwrap();

    // Wait until the render thread is parked inside its draw (its borrow context — the
    // protected `&mut` in the naive case — is live).
    let _ = ack_rx.recv();

    // MAIN (display-callback path): fire the CA `display` → display_callback →
    // draw_frame_raw. In the naive case this raw foreign draw (mutex lock + field write)
    // hits the render thread's LIVE protected `&mut` ⇒ UB. In the shipped case both are
    // raw ⇒ clean (draw_mutex serialises the field write; channel gives happens-before).
    unsafe { displaycb_display_callback(&layer) };

    // Release the render thread.
    go_tx.send(0).unwrap();
    handle.join().unwrap();

    let field = unsafe { (*rp).field };
    let completed = unsafe { (*rp).completed };
    let touched = unsafe { (*rp).api.touched };
    println!(
        "displaycb_{}: field={field} completed={completed} touched={touched}",
        if naive { "naive" } else { "shipped" }
    );
    drop(owner);
    drop(layer);
}

// ============================================================================
// P7-W0 T1 (`72cf50855`, #14150): the display-link resync moved OUT of the locked
// draw body. `draw_frame_raw` now releases `draw_mutex` and only then resyncs the
// display link (`sync_display_link`), so the resync can overlap the main thread's
// CoreAnimation `draw_frame_raw(sync = true)`. The question: may the resync keep its
// `&mut self` receiver, or must it be raw-self like the draw itself?
//
//   synclink_naive — the resync through a `&mut self` method (a protected whole-
//     `Renderer` `&mut` function argument), reached from the raw draw path after the
//     unlock. The handshake parks INSIDE the resync (the CVDisplayLinkStop join) while
//     the main thread runs its raw sync draw (lock + plain-field write). Expected: UB.
//
//   synclink_shipped — `sync_display_link_raw(this: *mut Renderer)`: NARROW field
//     reads through the raw root, no protector spans the call. Expected: CLEAN.
//
// The mpsc handshake orders the field accesses (no data race masks the verdict).
// ============================================================================

// The resync's reads: `display_link`/`visible` (modelled by `lead`), `cells_rebuilt`
// (modelled by `field`).
fn synclink_sync_mut(this: &mut FakeRenderer, go: &Receiver<u8>, ack: &Sender<u8>) -> u64 {
    let seen = this.lead.wrapping_add(this.field);
    // Parked inside the call: `this`'s protector is live across the main draw.
    ack.send(1).unwrap();
    go.recv().unwrap();
    seen.wrapping_add(this.lead)
}

unsafe fn synclink_sync_raw(this: *mut FakeRenderer, go: &Receiver<u8>, ack: &Sender<u8>) -> u64 {
    let seen = unsafe { (*this).lead.wrapping_add((*this).field) };
    ack.send(1).unwrap();
    go.recv().unwrap();
    seen.wrapping_add(unsafe { (*this).lead })
}

// The render thread's no-redraw draw: the locked body (lock, read, unlock), then the
// resync after the mutex is released.
unsafe fn synclink_draw_no_redraw(
    self_ptr: *mut FakeRenderer,
    naive: bool,
    go: &Receiver<u8>,
    ack: &Sender<u8>,
) -> u64 {
    let wants_sync = {
        let _g = unsafe { (*self_ptr).draw_mutex.lock().unwrap() };
        unsafe { (*self_ptr).field == 0 }
    };
    if !wants_sync {
        return 0;
    }
    if naive {
        synclink_sync_mut(unsafe { &mut *self_ptr }, go, ack)
    } else {
        unsafe { synclink_sync_raw(self_ptr, go, ack) }
    }
}

fn run_synclink(naive: bool) {
    let mut owner: Box<RNewtype> = Box::new(RNewtype(FakeRenderer::new()));
    let owner_ptr: *mut RNewtype = &raw mut *owner;
    let rp: *mut FakeRenderer = unsafe { &raw mut (*owner_ptr).0 };

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();

    let send = SendRN(owner_ptr);
    let handle = std::thread::Builder::new()
        .name("render".into())
        .spawn(move || {
            let s = send;
            let self_ptr: *mut FakeRenderer = unsafe { &raw mut (*s.0).0 };
            unsafe { synclink_draw_no_redraw(self_ptr, naive, &go_rx, &ack_tx) }
        })
        .unwrap();

    // Wait until the render thread is parked inside its resync, draw_mutex released.
    ack_rx.recv().unwrap();

    // MAIN: the CoreAnimation display callback's raw sync draw takes draw_mutex and
    // writes a plain field while the resync is still running.
    unsafe { displaycb_draw_frame_raw(rp) };

    go_tx.send(0).unwrap();
    let seen = handle.join().unwrap();

    let field = unsafe { (*rp).field };
    println!(
        "synclink_{}: seen={seen} field={field}",
        if naive { "naive" } else { "shipped" }
    );
    drop(owner);
}

// ============================================================================
// VAXIS_C3B_DESIGN §8: the Loop read-thread aliasing model (loopthread_*).
//
// Models Loop.zig's construct-then-spawn read thread with its DISTINCT field mechanics
// vs renderthread: the produced-to channel is the internally-synced Mutex-backed Queue
// (not a plain-field mailbox), should_quit is an AtomicBool (not a plain field), the
// `thread` handle is a Cell mutated through &self by MAIN while the child holds *mut Loop,
// and the re-entrant callback is the type-erased SIGWINCH winsize callback (*mut c_void
// context). PLUS the queued-element ownership handoff (§4.5/§6): loopthread_shipped posts
// an OWNED Vec<u8> — the child keeps mutating its per-iteration STACK ring AFTER posting,
// and main never observes that mutation because it popped an owned copy — whereas
// loopthread_naive additionally binds a whole &mut *self_ptr and would post a slice
// borrowing that ring (the §4.5 cross-thread hazard the owned promotion removes).
//
//   loopthread_shipped — raw self_ptr, NARROW field access only (should_quit atomic,
//     &(*sp).queue external push, (*sp).tty/(*sp).vaxis narrow, (*sp).field narrow),
//     main = &self queue ops + AtomicBool store + a narrow Cell `thread` take/set, owned
//     queue bytes: expected CLEAN under BOTH SB and TB.
//   loopthread_naive — the child binds `let l = &mut *self_ptr` (whole) held across the
//     read loop; the winsize callback reborrows a SIBLING `&mut *self_ptr` (pops `l`);
//     `l` is re-used after (use-after-pop); main's stop/nextEvent also reborrow a whole
//     `&mut Loop`: expected UB under BOTH SB and TB. (A channel handshake parks the child
//     in `recv` while main reborrows, so the ONLY reported UB is the tag violation — no
//     data race masks the verdict; the borrowed-ring race is the documented §4.5 hazard
//     the shipped owned-copy avoids.)
//
// Pass criterion (recorded in README.md): loopthread_shipped CLEAN under BOTH
// -Zmiri-stacked-borrows and -Zmiri-tree-borrows; loopthread_naive UB under both.
// ============================================================================

// MiniQueue models vaxis::Queue (internally-synced Mutex). Reached via *const (external,
// outside the LThread SB/TB coverage — the decoupled-producer lineage, search_shipped).
struct MiniQueue {
    items: SMutex<Vec<Vec<u8>>>,
}
impl MiniQueue {
    fn push(&self, v: Vec<u8>) {
        self.items.lock().unwrap().push(v);
    }
    fn pop(&self) -> Option<Vec<u8>> {
        self.items.lock().unwrap().pop()
    }
}

struct LThread {
    // the internally-synced channel, reached via *const (narrow raw field read → push)
    queue: *const MiniQueue,
    // Zig `should_quit: bool` → AtomicBool (main writes via store, child reads via load)
    should_quit: AtomicBool,
    // Zig `thread: ?std.Thread` → a Cell mutated by MAIN through &self while the child
    // holds *mut Loop; MAIN-THREAD-ONLY (the child never touches it).
    thread: ICell<Option<u32>>,
    tty: *mut u32,   // narrow raw field (getWinsize/read stand-in)
    vaxis: *mut u32, // narrow raw field (handle_event/winsize stand-in)
    field: u32,      // misc narrow field write
}

struct SendLT(*mut LThread);
unsafe impl Send for SendLT {}

// ---- winsize callback: type-erased ctx, raw self, NARROW field access (SHIPPED) ----
unsafe fn loop_winsize_cb_shipped(ctx: *mut c_void) {
    unsafe {
        let sp = ctx as *mut LThread; // @ptrCast(@alignCast(ptr)) back to *Self
        let _vx = (*sp).vaxis; // narrow raw read (in_band_resize stand-in)
        let _fd = (*sp).tty; // narrow raw read (tty.fd stand-in)
        let q: *const MiniQueue = (*sp).queue; // narrow raw read of the queue ptr
        (*q).push(vec![0xAA, 0xBB]); // postEvent(.{ .winsize = ws }) — OWNED
    }
}
// ---- winsize callback: whole &mut *sp reborrow (NAIVE) ----
unsafe fn loop_winsize_cb_naive(ctx: *mut c_void) {
    unsafe {
        let sp = ctx as *mut LThread;
        let l: &mut LThread = &mut *sp; // SIBLING whole reborrow → pops the child's `l`
        let q: *const MiniQueue = l.queue;
        (*q).push(vec![0xAA, 0xBB]);
        l.field = l.field.wrapping_add(1);
    }
}

fn run_loopthread(shipped: bool) {
    let mq = Box::new(MiniQueue {
        items: SMutex::new(Vec::new()),
    });
    let mq_ptr: *const MiniQueue = &*mq;
    let mut tty_val = Box::new(7u32);
    let mut vaxis_val = Box::new(9u32);
    let tty_ptr: *mut u32 = &raw mut *tty_val;
    let vaxis_ptr: *mut u32 = &raw mut *vaxis_val;

    let mut boxed: Box<LThread> = Box::new(LThread {
        queue: mq_ptr,
        should_quit: AtomicBool::new(false),
        thread: ICell::new(None),
        tty: tty_ptr,
        vaxis: vaxis_ptr,
        field: 0,
    });
    // ONE transient &mut at birth, lowered to raw (§2.4 / search_shipped).
    let sp: *mut LThread = &raw mut *boxed;

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel(); // main → child ticks
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();

    let send = SendLT(sp);
    let handle = std::thread::Builder::new()
        .name("ttyRun".into())
        .spawn(move || {
            let s = send;
            let go = go_rx;
            let ack = ack_tx;
            unsafe {
                if shipped {
                    // ttyRun(self_ptr): NARROW projections only; a per-iteration STACK ring
                    // whose bytes are copied OWNED into the queue (the child keeps mutating
                    // the ring AFTER the push; main never observes it).
                    let mut ring = [0u8; 8]; // the ttyRun stack-local grapheme ring
                    loop {
                        let msg = match go.recv() {
                            Ok(m) => m,
                            Err(_) => break,
                        };
                        if msg == 2 {
                            break;
                        }
                        if msg == 1 {
                            loop_winsize_cb_shipped(s.0 as *mut c_void); // re-entrant winsize cb, raw ctx
                        }
                        // Zig ttyRun checks should_quit at the loop top.
                        if (*s.0).should_quit.load(AOrd::Relaxed) {
                            break;
                        }
                        // "read" bytes into the ring
                        ring[0] = ring[0].wrapping_add(1);
                        // OWNED copy across the queue (models Key::into_owned)
                        let q: *const MiniQueue = (*s.0).queue;
                        (*q).push(ring[0..2].to_vec());
                        // keep mutating the ring AFTER posting (the wrap that would race a
                        // BORROWED slice — harmless here because we posted an owned copy)
                        ring[1] = ring[1].wrapping_add(1);
                        (*s.0).field = (*s.0).field.wrapping_add(1); // narrow write
                        ack.send(1).unwrap();
                    }
                } else {
                    // NAIVE: whole &mut held across the loop; winsize cb reborrows &mut *s.0
                    // (sibling) → pops `l`; `l` re-used after → use-after-pop.
                    let l: &mut LThread = &mut *s.0;
                    let mut ring = [0u8; 8];
                    loop {
                        let msg = match go.recv() {
                            Ok(m) => m,
                            Err(_) => break,
                        };
                        if msg == 2 {
                            break;
                        }
                        if msg == 1 {
                            loop_winsize_cb_naive(s.0 as *mut c_void); // pops `l`
                            l.field = l.field.wrapping_add(1); // UAP
                        }
                        if l.should_quit.load(AOrd::Relaxed) {
                            break;
                        }
                        ring[0] = ring[0].wrapping_add(1);
                        let q: *const MiniQueue = l.queue;
                        // NAIVE would push a slice BORROWING `ring` (the §4.5 hazard); an
                        // owned copy stands in so MiniQueue stays `Vec<Vec<u8>>`.
                        (*q).push(ring[0..2].to_vec());
                        l.field = l.field.wrapping_add(1);
                        ack.send(1).unwrap();
                    }
                }
            }
        })
        .unwrap();

    // start(): set the `thread` Cell THROUGH a NARROW &(*sp).thread (main-thread-only; the
    // child never touches `thread`), AFTER the spawn — interior mutability, never a
    // *mut-through-&self plain-field write.
    unsafe {
        (*sp).thread.set(Some(1));
    }

    // let the child arm (establish its tag) — first read tick.
    go_tx.send(0).unwrap();
    let _ = ack_rx.recv();

    // winsize tick: the re-entrant callback fires on the child while its borrow context
    // (the naive `l`) is LIVE.
    go_tx.send(1).unwrap();
    let _ = ack_rx.recv();

    // MAIN steady state (between child ticks, child parked in recv → happens-before via
    // the channel): stop() pops the queue + stores should_quit.
    if shipped {
        let q: *const MiniQueue = unsafe { (*sp).queue }; // narrow raw read
        let _ = unsafe { (*q).pop() };
        unsafe {
            (*sp).should_quit.store(true, AOrd::Relaxed); // narrow atomic store
        }
    } else {
        let m: &mut LThread = unsafe { &mut *sp }; // whole reborrow → pops the child tag
        let q: *const MiniQueue = m.queue;
        let _ = unsafe { (*q).pop() };
        m.should_quit.store(true, AOrd::Relaxed);
    }

    // stop(): trigger a read tick so the child re-checks should_quit and breaks, then join
    // (no ack expected — the child breaks without acking).
    go_tx.send(0).unwrap();
    let _ = go_tx.send(2); // belt-and-suspenders break (receiver may already be gone)
    handle.join().unwrap();

    // take the `thread` Cell (narrow, main-only) after join.
    let _ = unsafe { (*sp).thread.take() };

    let field = unsafe { (*sp).field };
    let qlen = mq.items.lock().unwrap().len();
    println!(
        "loopthread_{}: field={field} qlen={qlen}",
        if shipped { "shipped" } else { "naive" }
    );
    drop(boxed);
    drop(mq);
    drop(tty_val);
    drop(vaxis_val);
}

// ============================================================================
// T11 / VALIDATION_REPORT H1: the Loop.stop()-vs-blocked-read overlap (stopread_*).
//
// Models the SHIPPED interactive exit path (`loop_.rs` `stop()` vs `tty_run`): the
// read thread is BY DESIGN blocked inside `Tty::read` (waiting on /dev/tty bytes)
// when the main thread calls `stop()`, which must WAKE it by writing a DSR request
// through the tty writer (`(*vx).device_status_report((*tty).writer())`). The
// loopthread_* pair above does NOT cover this: it models tty/vaxis as bare `*mut u32`
// and serializes stop() against the reader via channel handshakes, so the
// writer-retag-during-blocked-read overlap never executes. Non-FFI stand-in for the
// blocked `libc::read`: an mpsc `recv()` parked inside the read fn; the DSR flush
// "reaches the pty" by sending on that channel (the kernel hop of `libc::write`).
//
//   stopread_naive    — the pre-fix loop_.rs:206-211 shape: main forms a whole
//     `&mut Tty` via `(*tty).writer()` (autoref Unique retag over ALL Tty bytes)
//     while the reader is blocked in `Tty::read(&self)` holding a CALL-PROTECTED
//     SharedReadOnly of the same Tty: expected UB (protector violation) under SB+TB.
//   stopread_projonly — main HALF-fixed (writer projected via
//     `addr_of_mut!((*tty).tty_writer)`, no whole `&mut Tty`) but the reader still
//     blocked in `read(&self)`: the DSR bytes land in `tty_writer.buf` — a Vec-len
//     write INSIDE the reader's protected whole-Tty range — so the `&mut SWriter`
//     reborrow still pops the protected tag: expected UB under SB+TB. This is the
//     load-bearing variant: it PROVES `Tty::read` itself must go raw-self (narrow
//     fd projection), not just the stop() side.
//   stopread_dsrref   — tty side fully raw on BOTH ends, but main's
//     `device_status_report(&self)` holds a protected `&Vaxis` while the read
//     thread does a plain raw write to `vaxis.caps.*` (a late capability event):
//     the foreign write pops the protected shared tag: expected UB under SB+TB.
//   stopread_shipped  — the FIXED shape: reader blocked in
//     `Tty::read(this: *const Self)` (narrow `fd` projection + a field-narrow park,
//     NO whole-Tty borrow), main projects `addr_of_mut!((*tty).tty_writer)` and
//     calls a self-less `device_status_report(&mut *w)` (upstream libvaxis 0.5.1 is
//     `pub fn deviceStatusReport(_: Vaxis, tty: *IoWriter)` — the receiver is
//     UNUSED, so dropping it is the faithful form): expected CLEAN under SB and TB.
//
// Pass criterion (README): stopread_shipped CLEAN under BOTH -Zmiri-stacked-borrows
// (default) and -Zmiri-tree-borrows; the three others UB.
// ============================================================================

/// models `TtyWriter { fd, buf }` — `flush()`'s `libc::write` is the pty hop that
/// wakes the blocked reader; modeled by sending the bytes on the reader's `data`
/// channel (`sink`).
struct SWriter {
    fd: i32,
    buf: Vec<u8>,
    sink: Sender<u8>,
}
impl SWriter {
    fn write_all(&mut self, data: &[u8]) {
        // the Vec len write lives INSIDE the Tty allocation (the projonly hazard)
        self.buf.extend_from_slice(data);
    }
    fn flush(&mut self) {
        for &b in &self.buf {
            let _ = self.sink.send(b); // libc::write(self.fd, ..) → wakes the reader
        }
        self.buf.clear();
    }
}

/// models `PosixTty { fd, termios, tty_writer }` + the pty read end.
struct STty {
    fd: i32,
    tty_writer: SWriter,
    /// the "pty" read end (`libc::read` stand-in)
    data: Receiver<u8>,
    /// ack: the reader IS inside `read()`, its borrow established
    entered_read: Sender<u8>,
}
impl STty {
    /// the PRE-FIX shape: `read(&self)` — a call-protected shared borrow of the
    /// WHOLE Tty held for the entire blocked "syscall".
    fn read_ref(&self, buf: &mut [u8]) -> usize {
        let _ = self.entered_read.send(1); // reader is now inside the protected call
        match self.data.recv() {
            // blocked libc::read(self.fd, buf..)
            Ok(b) => {
                buf[0] = b;
                1
            }
            Err(_) => 0,
        }
    }
    /// the FIXED shape (tty.rs `read(this: *const Self)`): narrow projections only;
    /// the park holds a borrow of the `data` FIELD only (the closest non-FFI stand-in
    /// for "no Rust borrow held across the blocked syscall"), never the whole Tty.
    unsafe fn read_raw(this: *const Self, buf: &mut [u8]) -> usize {
        unsafe {
            let _fd = std::ptr::addr_of!((*this).fd).read(); // libc::read's fd arg
            let _ = (*std::ptr::addr_of!((*this).entered_read)).send(1);
            match (*std::ptr::addr_of!((*this).data)).recv() {
                Ok(b) => {
                    buf[0] = b;
                    1
                }
                Err(_) => 0,
            }
        }
    }
    /// the PRE-FIX `writer(&mut self)` — the whole-Tty Unique retag of the naive shape.
    fn writer(&mut self) -> &mut SWriter {
        &mut self.tty_writer
    }
}

/// models `Vaxis { caps, .. }` — caps are PLAIN fields the read thread writes raw.
struct SVaxis {
    caps_kitty_keyboard: bool,
}
impl SVaxis {
    /// the PRE-FIX `device_status_report(&self, w)` — a protected `&Vaxis` held
    /// across the write+flush. The `in_dsr`/`caps_done` handshake deterministically
    /// places the reader's caps write INSIDE the protected window (channels give
    /// happens-before, so the ONLY reported UB is the tag violation, no data race).
    fn dsr_ref(&self, w: &mut SWriter, in_dsr: &Sender<u8>, caps_done: &Receiver<u8>) {
        let _ = in_dsr.send(1); // the &Vaxis protector is live NOW
        let _ = caps_done.recv(); // the reader does its raw caps write meanwhile
        w.write_all(b"\x1b[5n");
        w.flush();
    }
    /// the PRE-FIX `device_status_report(&self, w)` verbatim (no handshake): used by
    /// the Naive arm so the stop() line is a literal mirror of loop_.rs:210.
    fn device_status_report(&self, w: &mut SWriter) {
        w.write_all(b"\x1b[5n");
        w.flush();
    }
}
/// the FIXED shape: upstream `deviceStatusReport(_: Vaxis, tty)` never uses the
/// receiver — ported self-less so no `&Vaxis` protector exists during the wake.
fn dsr_shipped(w: &mut SWriter) {
    w.write_all(b"\x1b[5n");
    w.flush();
}

/// models `Loop { tty: *mut Tty, vaxis: *mut Vaxis, should_quit, thread }`.
struct SLoop {
    tty: *mut STty,
    vaxis: *mut SVaxis,
    should_quit: AtomicBool,
    thread: ICell<Option<u32>>,
}
struct SendSL(*mut SLoop);
unsafe impl Send for SendSL {}

#[derive(Clone, Copy, PartialEq)]
enum StopRead {
    Naive,    // whole `&mut Tty` via (*tty).writer() during blocked read(&self)
    ProjOnly, // addr_of_mut! writer projection, reader STILL in read(&self)
    DsrRef,   // tty raw both ends, but protected &Vaxis DSR vs raw caps write
    Shipped,  // all narrow/raw/self-less — the fixed loop_.rs shape
}

fn run_stopread(variant: StopRead) {
    let (data_tx, data_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (entered_tx, entered_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (in_dsr_tx, in_dsr_rx): (Sender<u8>, Receiver<u8>) = channel();
    let (caps_done_tx, caps_done_rx): (Sender<u8>, Receiver<u8>) = channel();

    let mut tty = Box::new(STty {
        fd: 7,
        tty_writer: SWriter {
            fd: 7,
            buf: Vec::new(),
            sink: data_tx,
        },
        data: data_rx,
        entered_read: entered_tx,
    });
    let mut vx = Box::new(SVaxis {
        caps_kitty_keyboard: false,
    });
    // raw ptrs once at birth (the list_themes `&raw mut` discipline)
    let tty_ptr: *mut STty = &raw mut *tty;
    let vx_ptr: *mut SVaxis = &raw mut *vx;

    let mut lp = Box::new(SLoop {
        tty: tty_ptr,
        vaxis: vx_ptr,
        should_quit: AtomicBool::new(false),
        thread: ICell::new(None),
    });
    let sp: *mut SLoop = &raw mut *lp;

    let send = SendSL(sp);
    let handle = std::thread::Builder::new()
        .name("ttyRun".into())
        .spawn(move || {
            let s = send;
            unsafe {
                if variant == StopRead::DsrRef {
                    // the caps leg in isolation: a late capability event (parsed off
                    // the wire) does a plain RAW write into vaxis.caps while main sits
                    // inside the protected `device_status_report(&self)`.
                    let _ = in_dsr_rx.recv(); // main's &Vaxis protector is live
                    (*(*s.0).vaxis).caps_kitty_keyboard = true; // foreign write → pops it
                    let _ = caps_done_tx.send(1);
                    return;
                }
                // handle_event_generic's caps write — raw, main never borrows &Vaxis
                // in the shipped shape, so this is conflict-free there.
                (*(*s.0).vaxis).caps_kitty_keyboard = true;
                // tty_run read loop (should_quit checked at loop top, Zig-faithful)
                let mut buf = [0u8; 8];
                while !(*s.0).should_quit.load(AOrd::Relaxed) {
                    let n = match variant {
                        StopRead::Shipped => STty::read_raw((*s.0).tty, &mut buf),
                        // Naive/ProjOnly: the whole-&Tty call-protected blocked read
                        _ => (*(*s.0).tty).read_ref(&mut buf),
                    };
                    if n == 0 {
                        return;
                    }
                }
            }
        })
        .unwrap();

    // start(): the `thread` Cell is MAIN-only (narrow, interior-mutable).
    unsafe {
        (*sp).thread.set(Some(1));
    }

    if variant == StopRead::DsrRef {
        // stop()'s DSR with the pre-fix `&self` receiver, writer correctly projected:
        // UB fires at the reader's caps write while this protector is live.
        unsafe {
            (*sp).should_quit.store(true, AOrd::Relaxed);
            let tty = (*sp).tty;
            let vx = (*sp).vaxis;
            let w = std::ptr::addr_of_mut!((*tty).tty_writer);
            (*vx).dsr_ref(&mut *w, &in_dsr_tx, &caps_done_rx);
        }
        handle.join().unwrap();
        unsafe {
            let _ = (*sp).thread.take();
            (*sp).should_quit.store(false, AOrd::Relaxed);
        }
        println!("stopread_dsrref: caps={}", vx.caps_kitty_keyboard);
        return;
    }

    // wait until the reader IS inside read() with its borrow established — the
    // deterministic image of "stop() runs while the reader is blocked in read".
    let _ = entered_rx.recv();

    // === stop() ===
    unsafe {
        (*sp).should_quit.store(true, AOrd::Relaxed);
        // trigger a read (the DSR wake)
        let tty = (*sp).tty;
        let vx = (*sp).vaxis;
        match variant {
            StopRead::Naive => {
                // the pre-fix loop_.rs:210 line VERBATIM: `(*tty).writer()` is a
                // whole `&mut Tty` autoref — pops the reader's call-protected SRO →
                // protector violation UB.
                (*vx).device_status_report((*tty).writer());
            }
            StopRead::ProjOnly => {
                // HALF-fix: narrow writer projection, but the reader still holds a
                // protected whole-&Tty — the &mut SWriter reborrow lands inside that
                // protected range → still UB.
                let w = std::ptr::addr_of_mut!((*tty).tty_writer);
                dsr_shipped(&mut *w);
            }
            StopRead::Shipped => {
                // the FIXED line: narrow projection + self-less DSR.
                let w = std::ptr::addr_of_mut!((*tty).tty_writer);
                dsr_shipped(&mut *w);
            }
            StopRead::DsrRef => unreachable!(),
        }
    }
    handle.join().unwrap();
    unsafe {
        let _ = (*sp).thread.take();
        (*sp).should_quit.store(false, AOrd::Relaxed);
    }
    // reader joined → whole-object access is sound again (the deinit phase).
    println!(
        "stopread_{}: caps={} buf_empty={}",
        match variant {
            StopRead::Naive => "naive",
            StopRead::ProjOnly => "projonly",
            StopRead::Shipped => "shipped",
            StopRead::DsrRef => "dsrref",
        },
        vx.caps_kitty_keyboard,
        tty.tty_writer.buf.is_empty()
    );
    drop(lp);
    drop(tty);
    drop(vx);
}

// ============================================================================
// T12 / VALIDATION_REPORT §3c.1: Page::get_row `&mut Row` materialization (getrow_*).
//
// Models `crates/rustty-vt/src/page_impl.rs` `get_row`: the page's rows live in
// OFFSET-ADDRESSED backing memory (a separate allocation reached via a base ptr
// + offset), and `getRow(self: *const Page) *Row` returns a mutable pointer into
// it. The pre-fix Rust body materialized that pointer as `&mut *ptr` — minting a
// Unique-derived tag over the row's bytes. Because a Zig `*Row` carries NO
// aliasing protector, callers are free to hold one `*Row` while deriving another
// to the SAME row; under Stacked Borrows the second `&mut` retag pops the first
// Unique tag, so the held pointer's next use is use-after-pop. Pure pointer
// arithmetic (the shipped shape, matching the sibling get_row_and_cell/get_cells)
// derives both pointers from the same raw root — compatible raw siblings, no tag
// to pop.
//
//   getrow_naive   — `&mut *rows_base.add(y)` twice for the same y; the first
//                    pointer is used after the second call. Expected: UB.
//   getrow_shipped — `rows_base.add(y)` (pure arithmetic) twice; same access
//                    pattern. Expected: CLEAN under BOTH SB and TB.
// ============================================================================

#[derive(Clone, Copy)]
struct MiniRow {
    semantic_prompt: u8,
    wrap: bool,
}

// Models Page: the struct holds only a base pointer + row count; the rows live
// in a SEPARATE heap allocation (the offset-addressed page memory).
struct MiniPage {
    memory: *mut MiniRow, // models `self.rows.ptr(self.memory_ptr())`
    rows: usize,
}
impl MiniPage {
    fn new(rows: usize) -> MiniPage {
        let v: Vec<MiniRow> = vec![
            MiniRow {
                semantic_prompt: 0,
                wrap: false
            };
            rows
        ];
        let boxed: Box<[MiniRow]> = v.into_boxed_slice();
        MiniPage {
            memory: Box::into_raw(boxed) as *mut MiniRow,
            rows,
        }
    }
    // PRE-FIX shape: `unsafe { &mut *self.rows.ptr(self.memory_ptr()).add(y) }`
    // — materializes a `&mut Row` (Unique tag) before decaying to *mut.
    fn get_row_naive(&self, y: usize) -> *mut MiniRow {
        assert!(y < self.rows);
        unsafe { &mut *self.memory.add(y) }
    }
    // SHIPPED shape: pure pointer arithmetic (matches get_row_and_cell/get_cells).
    fn get_row_shipped(&self, y: usize) -> *mut MiniRow {
        assert!(y < self.rows);
        unsafe { self.memory.add(y) }
    }
}
impl Drop for MiniPage {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                self.memory,
                self.rows,
            )));
        }
    }
}

fn run_getrow(naive: bool) {
    let page = MiniPage::new(4);
    // The latent pattern-(b) caller: hold one `*Row` while deriving another to
    // the SAME row (a Zig `*Row` permits this freely), then use the held one.
    let (a, b) = if naive {
        (page.get_row_naive(2), page.get_row_naive(2))
    } else {
        (page.get_row_shipped(2), page.get_row_shipped(2))
    };
    unsafe {
        (*b).wrap = true; // touch through the second pointer…
        (*a).semantic_prompt = 7; // …then USE the held first pointer (naive: use-after-pop)
    }
    let sp = unsafe { (*page.get_row_shipped(2)).semantic_prompt };
    println!(
        "getrow_{}: semantic_prompt={sp}",
        if naive { "naive" } else { "shipped" }
    );
}

// ============================================================================
// T12 / VALIDATION_REPORT §3c.5: vaxis SIGWINCH handler lifetime (winch_*).
//
// Models `crates/rustty-vaxis/src/tty.rs` + `loop_.rs`: `Loop::init` registers a
// SignalHandler { context: *mut Loop, callback } into the process-global
// HANDLERS array; `handle_winch` (the SIGWINCH sigaction target) iterates the
// array and invokes every callback with its stored context. libvaxis never
// deregisters: dropping the Loop leaves a DANGLING *mut in HANDLERS, and the
// next SIGWINCH is a use-after-free in signal context. The shipped fix gives
// `Loop` a `Drop` that removes its own entries (`Tty::deregister_winsize`,
// keyed on the context pointer) before the pointee dies.
//
//   winch_naive   — register, drop the Loop, fire handle_winch: the callback
//                   derefs the dangling context. Expected: UB (use of a
//                   deallocated allocation).
//   winch_shipped — register, DEREGISTER on drop, fire handle_winch: no entry
//                   left, nothing dereferenced. Expected: CLEAN under SB + TB.
// ============================================================================

struct WSignalHandler {
    context: *mut c_void,
    callback: unsafe fn(*mut c_void),
}
struct WHandlerState {
    handlers: [Option<WSignalHandler>; 8],
    idx: usize,
}
// SAFETY: models tty.rs's `unsafe impl Send for SignalHandler` (raw ctx in a
// static Mutex) — single-threaded here, the Send is Mutex-container glue only.
unsafe impl Send for WSignalHandler {}
// Models the `static HANDLERS: Mutex<HandlerState>` global.
static WHANDLERS: SMutex<WHandlerState> = SMutex::new(WHandlerState {
    handlers: [const { None }; 8],
    idx: 0,
});

struct WLoop {
    events: u32,
}
unsafe fn wloop_winsize_cb(ctx: *mut c_void) {
    let lp = ctx as *mut WLoop;
    unsafe { (*lp).events = (*lp).events.wrapping_add(1) }; // UAF if lp dangles
}

fn wnotify_winsize(h: WSignalHandler) {
    let mut st = WHANDLERS.lock().unwrap();
    let idx = st.idx;
    assert!(idx < st.handlers.len());
    st.handlers[idx] = Some(h);
    st.idx += 1;
}
// The shipped deregistration: remove every entry whose context matches, then
// compact (mirrors Tty::deregister_winsize).
fn wderegister_winsize(context: *mut c_void) {
    let mut st = WHANDLERS.lock().unwrap();
    let mut w = 0usize;
    for r in 0..st.idx {
        let keep = match &st.handlers[r] {
            Some(h) => h.context != context,
            None => false,
        };
        if keep {
            if w != r {
                st.handlers[w] = st.handlers[r].take();
            }
            w += 1;
        } else {
            st.handlers[r] = None;
        }
    }
    st.idx = w;
}
fn whandle_winch() {
    let st = WHANDLERS.lock().unwrap();
    let mut i = 0usize;
    while i < st.idx {
        if let Some(h) = &st.handlers[i] {
            unsafe { (h.callback)(h.context) };
        }
        i += 1;
    }
}

fn run_winch(shipped: bool) {
    // A first, still-live loop so the handler array is non-trivially iterated.
    let mut live = Box::new(WLoop { events: 0 });
    let live_ptr: *mut WLoop = &raw mut *live;
    wnotify_winsize(WSignalHandler {
        context: live_ptr as *mut c_void,
        callback: wloop_winsize_cb,
    });

    // The short-lived loop: register, then die.
    {
        let mut short = Box::new(WLoop { events: 0 });
        let short_ptr: *mut WLoop = &raw mut *short;
        wnotify_winsize(WSignalHandler {
            context: short_ptr as *mut c_void,
            callback: wloop_winsize_cb,
        });
        whandle_winch(); // both alive: fine in both shapes
        if shipped {
            // Loop::drop → Tty::deregister_winsize(self as ctx)
            wderegister_winsize(short_ptr as *mut c_void);
        }
        drop(short);
    }

    // The next SIGWINCH after the short loop died.
    whandle_winch(); // naive: derefs the dangling short_ptr → UAF

    let ev = live.events;
    // reset the global so candidates are order-independent
    wderegister_winsize(live_ptr as *mut c_void);
    println!(
        "winch_{}: live_events={ev}",
        if shipped { "shipped" } else { "naive" }
    );
    drop(live);
}

// ============================================================================
// T12 / VALIDATION_REPORT §3c.10: IOSurfaceLayer display_cb/display_ctx ivars
// (ivars_*).
//
// Models `crates/rustty-renderer/src/metal/iosurface_layer.rs` `Ivars`: the
// stored display callback + context are written by `set_display_callback` and
// read by the ObjC `display` override; nothing pins the two to one thread at
// the type level. The pre-fix `Cell<*const c_void>`/`Cell<*mut c_void>` are
// NON-atomic: two threads touching them unsynchronized is a data race
// (formally UB, even though it mirrors the reference's plain ObjC ivars).
// `AtomicPtr` is the free fix.
//
//   ivars_naive   — Cell written on one thread, read concurrently on another.
//                   Expected: UB (data race) under Miri.
//   ivars_shipped — AtomicPtr store/load (SeqCst). Expected: CLEAN.
// ============================================================================

use std::sync::atomic::AtomicPtr;

struct IvarsNaive {
    display_cb: ICell<*const c_void>,
    display_ctx: ICell<*mut c_void>,
}
// SAFETY(deliberately wrong — the UB control): models the objc2 class object
// being reachable from two threads while its ivars are plain Cells.
unsafe impl Sync for IvarsNaive {}

struct IvarsShipped {
    display_cb: AtomicPtr<c_void>,
    display_ctx: AtomicPtr<c_void>,
}

fn run_ivars(shipped: bool) {
    unsafe fn cbfn(_ctx: *mut c_void) {}
    // fn-pointer bits carried as usize so the closure capture is Send; the cast
    // back happens on the storing thread (models the transmute round-trip).
    let cb_bits = cbfn as usize;

    let out = if shipped {
        let ivars = IvarsShipped {
            display_cb: AtomicPtr::new(core::ptr::null_mut()),
            display_ctx: AtomicPtr::new(core::ptr::null_mut()),
        };
        let iv = &ivars; // capture the WHOLE struct ref (edition-2021 field capture)
        std::thread::scope(|s| {
            // renderer thread: set_display_callback
            s.spawn(move || {
                iv.display_cb.store(cb_bits as *mut c_void, AOrd::SeqCst);
                iv.display_ctx
                    .store(0xABCD_usize as *mut c_void, AOrd::SeqCst);
            });
            // main thread: the CA `display` override reads concurrently
            let cb = iv.display_cb.load(AOrd::SeqCst);
            let ctx = iv.display_ctx.load(AOrd::SeqCst);
            (cb as usize, ctx as usize)
        })
    } else {
        let ivars = IvarsNaive {
            display_cb: ICell::new(core::ptr::null()),
            display_ctx: ICell::new(core::ptr::null_mut()),
        };
        let iv = &ivars; // capture the WHOLE struct ref (edition-2021 field capture)
        std::thread::scope(|s| {
            s.spawn(move || {
                iv.display_cb.set(cb_bits as *const c_void); // non-atomic write…
                iv.display_ctx.set(0xABCD_usize as *mut c_void);
            });
            // …racing this non-atomic read on another thread → data race UB
            let cb = iv.display_cb.get();
            let ctx = iv.display_ctx.get();
            (cb as usize, ctx as usize)
        })
    };
    println!(
        "ivars_{}: cb={:x} ctx={:x}",
        if shipped { "shipped" } else { "naive" },
        out.0,
        out.1
    );
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    match which.as_str() {
        "getrow_naive" => return run_getrow(true),
        "getrow_shipped" => return run_getrow(false),
        "winch_naive" => return run_winch(false),
        "winch_shipped" => return run_winch(true),
        "ivars_naive" => return run_ivars(false),
        "ivars_shipped" => return run_ivars(true),
        _ => {}
    }
    match which.as_str() {
        // Chunk-4 draw_frame / completion candidates (own Rndr2 model)
        "drawframe_shipped" | "drawframe_naive" | "atlas_self_isolated" => {
            let mut r2 = Rndr2::new();
            match which.as_str() {
                "drawframe_shipped" => r2.draw_frame_shipped(true),
                "drawframe_naive" => r2.draw_frame_naive(true),
                "atlas_self_isolated" => r2.atlas_self_isolated(),
                _ => unreachable!(),
            }
            println!(
                "{}: other={} sema={} frame0_uni={:?}",
                which,
                r2.other,
                r2.sema.get(),
                r2.swap_chain[1].uniforms
            );
            unsafe {
                drop(Box::from_raw(r2.font_grid));
            }
            return;
        }
        _ => {}
    }

    // SEARCH_BINDING_ARM_DESIGN §3.4 candidates (own detached-spawn model).
    match which.as_str() {
        "search_shipped" => {
            run_search(true);
            return;
        }
        "search_naive" => {
            run_search(false);
            return;
        }
        "callback_shipped" => {
            run_callback(true);
            return;
        }
        "callback_naive" => {
            run_callback(false);
            return;
        }
        "renderthread_shipped" => {
            run_renderthread(true);
            return;
        }
        "renderthread_naive" => {
            run_renderthread(false);
            return;
        }
        "displaycb_shipped" => {
            run_displaycb(false);
            return;
        }
        "displaycb_naive" => {
            run_displaycb(true);
            return;
        }
        "synclink_shipped" => {
            run_synclink(false);
            return;
        }
        "synclink_naive" => {
            run_synclink(true);
            return;
        }
        "loopthread_shipped" => {
            run_loopthread(true);
            return;
        }
        "loopthread_naive" => {
            run_loopthread(false);
            return;
        }
        "stopread_naive" => {
            run_stopread(StopRead::Naive);
            return;
        }
        "stopread_projonly" => {
            run_stopread(StopRead::ProjOnly);
            return;
        }
        "stopread_dsrref" => {
            run_stopread(StopRead::DsrRef);
            return;
        }
        "stopread_shipped" => {
            run_stopread(StopRead::Shipped);
            return;
        }
        _ => {}
    }

    let mut r = Renderer::new();
    match which.as_str() {
        "row_current" => r.rebuild_row_current(),
        "row_rawself" => r.rebuild_row_rawself(),
        "row_shaper_idiom" => r.rebuild_row_shaper_idiom(),
        "row_rawsibling" => r.rebuild_row_rawsibling(),
        "row_surgical" => r.rebuild_row_surgical(),
        "row_persistent" => r.rebuild_row_persistent(),
        "row_realistic" => r.rebuild_row_realistic(),
        "cursor_current" => r.addcursor_current(),
        "cursor_hoist" => r.addcursor_hoist(),
        "cursor_rawself" => r.addcursor_rawself(),
        other => {
            eprintln!("unknown candidate: {other:?}");
            std::process::exit(2);
        }
    }
    // consume cells so nothing is optimized away
    println!("{}: cells={:?}", which, r.cells);
    // reclaim the grid box the harness leaked via into_raw (not part of the
    // aliasing model; just keeps Miri's leak checker quiet)
    unsafe { drop(Box::from_raw(r.font_grid)); }
}
