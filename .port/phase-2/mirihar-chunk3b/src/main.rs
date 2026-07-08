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

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
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
