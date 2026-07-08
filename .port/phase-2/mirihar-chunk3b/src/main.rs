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

use std::sync::atomic::{AtomicU32, Ordering as AOrd};
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

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
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
