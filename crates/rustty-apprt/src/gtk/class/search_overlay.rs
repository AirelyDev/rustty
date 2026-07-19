//! rustty-apprt::gtk::class::search_overlay
//!
//! PORTS: ghostty `src/apprt/gtk/class/search_overlay.zig` (`GhosttySearchOverlay`).
//!
//! The overlay shown while a search is active on a surface. `Surface`'s template
//! instantiates `$GhosttySearchOverlay search_overlay { … }` (`surface.blp:191-196`)
//! and connects its four signals (`stop-search`, `search-changed`, `next-match`,
//! `previous-match`); the surface's border/unfocused-split closure reads the
//! `active` property (`surface.blp:208`). This is a `CompositeTemplate` over
//! `ui/1.2/search-overlay.ui` (`search-overlay.blp`): the `search_entry`
//! `SearchEntry` child, the match-count `Label` (via the `match_label_closure`
//! bind-closure), and the prev/next/close buttons — all of which emit the four
//! outward signals the surface reacts to.
//!
//! Mirrors the `CompositeTemplate`-with-children idiom of `resize_overlay.rs`
//! (loads the `.ui`, binds a template child) and the `PhantomData` computed-
//! property idiom of `config.rs` (the read-only `search-total`/`has-search-total`
//! /`search-selected`/`has-search-selected` props derive from `Option<usize>`
//! backing fields notified explicitly by `set_search_total`/`set_search_selected`).

use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use glib::prelude::*;
use glib::translate::FromGlib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    /// PORTS: `search_overlay.zig:25` `pub const SearchOverlay` + its `Private`
    /// (`:207-227`). A `CompositeTemplate` over `ui/1.2/search-overlay.ui`; binds
    /// the `search_entry` child (`search_overlay.zig:209,456`).
    #[derive(CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::SearchOverlay)]
    #[template(resource = "/io/airely/rustty/ui/1.2/search-overlay.ui")]
    pub struct SearchOverlay {
        /// PORTS: `search_overlay.zig:209` `search_entry: *gtk.SearchEntry` — the
        /// search text entry (`search-overlay.blp:26`).
        #[template_child]
        pub search_entry: TemplateChild<gtk4::SearchEntry>,

        /// PORTS: `search_overlay.zig:38-56,212` property/field `active`. "True
        /// when a search is active, meaning we should show the overlay." The
        /// custom setter mirrors `setSearchActive` (`:244-251`): on an
        /// inactive→active transition it re-emits `search-changed` with the
        /// entry's current text.
        #[property(get, set = Self::set_active)]
        pub active: Cell<bool>,

        /// PORTS: `search_overlay.zig:58-75` read-only property `search-total`
        /// (getter `getSearchTotal`, `:288-290`). Derives from
        /// [`SearchOverlay::search_total`] (`0` when unknown).
        #[property(name = "search-total", get = Self::get_search_total)]
        pub search_total_prop: PhantomData<u64>,

        /// PORTS: `search_overlay.zig:77-92` read-only property `has-search-total`
        /// (getter `getHasSearchTotal`, `:292-294`).
        #[property(name = "has-search-total", get = Self::get_has_search_total)]
        pub has_search_total_prop: PhantomData<bool>,

        /// PORTS: `search_overlay.zig:94-111` read-only property `search-selected`
        /// (getter `getSearchSelected`, `:296-298`).
        #[property(name = "search-selected", get = Self::get_search_selected)]
        pub search_selected_prop: PhantomData<u64>,

        /// PORTS: `search_overlay.zig:113-128` read-only property
        /// `has-search-selected` (getter `getHasSearchSelected`, `:300-302`).
        #[property(name = "has-search-selected", get = Self::get_has_search_selected)]
        pub has_search_selected_prop: PhantomData<bool>,

        /// PORTS: `search_overlay.zig:130-141,221` property/field `halign-target`
        /// (default `.end`) — the target horizontal alignment; the template binds
        /// `halign: bind template.halign-target` (`search-overlay.blp:9`) and
        /// `on_drag_end` flips it (`:363-404`).
        #[property(get, set, name = "halign-target", builder(gtk4::Align::End))]
        pub halign_target: Cell<gtk4::Align>,

        /// PORTS: `search_overlay.zig:143-154,224` property/field `valign-target`
        /// (default `.start`) — the target vertical alignment
        /// (`search-overlay.blp:10`).
        #[property(get, set, name = "valign-target", builder(gtk4::Align::Start))]
        pub valign_target: Cell<gtk4::Align>,

        /// PORTS: `search_overlay.zig:215` `search_total: ?usize = null` — total
        /// number of matches (`null` = unknown/none). Backs `search-total` +
        /// `has-search-total`; not itself a GObject property.
        pub search_total: Cell<Option<usize>>,

        /// PORTS: `search_overlay.zig:218` `search_selected: ?usize = null` —
        /// currently selected match index. Backs `search-selected` +
        /// `has-search-selected`.
        pub search_selected: Cell<Option<usize>>,
    }

    impl Default for SearchOverlay {
        /// gtk4-rs needs a `Default` for `object_subclass`; `gtk.Align` has no
        /// `Default` so it cannot be derived. The align defaults mirror the
        /// reference pspec defaults (`search_overlay.zig:137,150`); the template
        /// (`search-overlay.blp:7-8`) sets them at load anyway.
        fn default() -> Self {
            Self {
                search_entry: TemplateChild::default(),
                active: Cell::new(false),
                search_total_prop: PhantomData,
                has_search_total_prop: PhantomData,
                search_selected_prop: PhantomData,
                has_search_selected_prop: PhantomData,
                halign_target: Cell::new(gtk4::Align::End),
                valign_target: Cell::new(gtk4::Align::Start),
                search_total: Cell::new(None),
                search_selected: Cell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchOverlay {
        // PORTS: `search_overlay.zig:30` `.name = "GhosttySearchOverlay"`.
        // Preserved 1:1 so the surface template's `$GhosttySearchOverlay` resolves.
        const NAME: &'static str = "GhosttySearchOverlay";
        type Type = super::SearchOverlay;
        // PORTS: `search_overlay.zig:28` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;

        /// PORTS: `search_overlay.zig:445-466` `Class.init` — load the template,
        /// bind the `search_entry` child + the seven template callbacks.
        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        /// PORTS: `search_overlay.zig:229-231` `init` — `gtk.Widget.initTemplate`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for SearchOverlay {
        /// PORTS: `search_overlay.zig:157-205` `signals`. The four the surface
        /// template connects (`surface.blp:192-195`).
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // `stop-search` (`:159-168`), no params.
                    glib::subclass::Signal::builder("stop-search").build(),
                    // `search-changed` (`:171-180`), one nullable-string param
                    // (`&.{?[*:0]const u8}`). `G_TYPE_STRING` carries the null.
                    glib::subclass::Signal::builder("search-changed")
                        .param_types([String::static_type()])
                        .build(),
                    // `next-match` (`:183-192`), no params.
                    glib::subclass::Signal::builder("next-match").build(),
                    // `previous-match` (`:195-204`), no params.
                    glib::subclass::Signal::builder("previous-match").build(),
                ]
            })
        }

        /// PORTS: `search_overlay.zig:409-422` `dispose` — dispose the template.
        /// The `search_entry` is deeply nested (not the Bin's root child), so no
        /// double-free workaround is needed (cf. `resize_overlay.rs`).
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for SearchOverlay {}
    impl BinImpl for SearchOverlay {}

    impl SearchOverlay {
        /// PORTS: `search_overlay.zig:243-251` `setSearchActive` (the property
        /// setter). On an inactive→active transition, re-emit `search-changed`
        /// with the entry's current text so the core search picks up the existing
        /// needle. The `active` field write comes last; the property machinery
        /// emits `notify::active` (which drives `visible: bind template.active`).
        fn set_active(&self, active: bool) {
            if !self.active.get() && active {
                // `const text = priv.search_entry.as(gtk.Editable).getText();`
                // `signals.@"search-changed".impl.emit(self, null, .{text}, null);`
                let text = self.search_entry.text();
                self.obj()
                    .emit_by_name::<()>("search-changed", &[&text.as_str()]);
            }
            self.active.set(active);
        }

        /// PORTS: `search_overlay.zig:288-290` `getSearchTotal`.
        fn get_search_total(&self) -> u64 {
            self.search_total.get().unwrap_or(0) as u64
        }

        /// PORTS: `search_overlay.zig:292-294` `getHasSearchTotal`.
        fn get_has_search_total(&self) -> bool {
            self.search_total.get().is_some()
        }

        /// PORTS: `search_overlay.zig:296-298` `getSearchSelected`.
        fn get_search_selected(&self) -> u64 {
            self.search_selected.get().unwrap_or(0) as u64
        }

        /// PORTS: `search_overlay.zig:300-302` `getHasSearchSelected`.
        fn get_has_search_selected(&self) -> bool {
            self.search_selected.get().is_some()
        }
    }
}

glib::wrapper! {
    /// PORTS: `search_overlay.zig:25` `pub const SearchOverlay`. The searchbar
    /// overlay driving a surface's find-in-terminal.
    pub struct SearchOverlay(ObjectSubclass<imp::SearchOverlay>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SearchOverlay {
    fn default() -> Self {
        glib::Object::new()
    }
}

#[gtk4::template_callbacks]
impl SearchOverlay {
    // -- outward-signal emitters (search-overlay.blp) ----------------------
    // PORT NOTE (arg order): these are connected NON-swapped in the blueprint,
    // so gtk4-rs delivers `[emitter, ...signal_args, this]` — the template
    // instance (`this: &SearchOverlay`) LAST, matching the reference's
    // callconv(.c) signatures (`search_overlay.zig:323-361`). The `stop_search`
    // /`next_match`/`previous_match` callbacks are shared between a `SearchEntry`
    // signal and a `Button.clicked`, so the reference types the emitter as
    // `*anyopaque` (`:331,335,339`); here it is the common `&gtk4::Widget`.

    /// PORTS: `search_overlay.zig:331-333` `stopSearch`. Emit `stop-search`.
    #[template_callback]
    fn stop_search(_emitter: &gtk4::Widget, this: &SearchOverlay) {
        this.emit_by_name::<()>("stop-search", &[]);
    }

    /// PORTS: `search_overlay.zig:323-326` `searchChanged`. On entry-text change,
    /// emit `search-changed` with the entry's current text.
    #[template_callback]
    fn search_changed(entry: &gtk4::SearchEntry, this: &SearchOverlay) {
        let text = entry.text();
        this.emit_by_name::<()>("search-changed", &[&text.as_str()]);
    }

    /// PORTS: `search_overlay.zig:335-337` `nextMatch`. Emit `next-match`.
    #[template_callback]
    fn next_match(_emitter: &gtk4::Widget, this: &SearchOverlay) {
        this.emit_by_name::<()>("next-match", &[]);
    }

    /// PORTS: `search_overlay.zig:339-341` `previousMatch`. Emit `previous-match`.
    #[template_callback]
    fn previous_match(_emitter: &gtk4::Widget, this: &SearchOverlay) {
        this.emit_by_name::<()>("previous-match", &[]);
    }

    /// PORTS: `search_overlay.zig:343-361` `searchEntryKeyPressed`. Return/KP_Enter
    /// navigates matches (Shift → previous, else next), consuming the event; other
    /// keys pass through. Bound on the entry's capture-phase `EventControllerKey`
    /// (`search-overlay.blp:31-35`).
    #[template_callback]
    fn search_entry_key_pressed(
        _ec: &gtk4::EventControllerKey,
        keyval: u32,
        _keycode: u32,
        gtk_mods: gdk4::ModifierType,
        this: &SearchOverlay,
    ) -> bool {
        // SAFETY: `gdk4::Key` is a transparent `u32` keyval newtype; every `u32`
        // is a valid keyval per GDK (mirrors `key.rs` `key_from_keyval`).
        let key = unsafe { gdk4::Key::from_glib(keyval) };
        if key == gdk4::Key::Return || key == gdk4::Key::KP_Enter {
            if gtk_mods.contains(gdk4::ModifierType::SHIFT_MASK) {
                this.emit_by_name::<()>("previous-match", &[]);
            } else {
                this.emit_by_name::<()>("next-match", &[]);
            }
            return true;
        }
        false
    }

    /// PORTS: `search_overlay.zig:363-404` `onDragEnd`. On drag end, flip the
    /// overlay's `halign-target`/`valign-target` to whichever corner the overlay's
    /// center crossed, so it can be dragged between corners of the parent.
    #[template_callback]
    fn on_drag_end(_gesture: &gtk4::GestureDrag, offset_x: f64, offset_y: f64, this: &SearchOverlay) {
        let widget = this.upcast_ref::<gtk4::Widget>();
        let Some(parent) = widget.parent() else {
            return;
        };

        // `getAllocatedWidth`/`getAllocatedHeight` (`search_overlay.zig:377-380`);
        // `width()`/`height()` are the non-deprecated gtk4 spelling of the
        // current allocation.
        let parent_width = f64::from(parent.width());
        let parent_height = f64::from(parent.height());
        let self_width = f64::from(widget.width());
        let self_height = f64::from(widget.height());

        let self_x = if this.halign_target() == gtk4::Align::Start {
            0.0
        } else {
            parent_width - self_width
        };
        let self_y = if this.valign_target() == gtk4::Align::Start {
            0.0
        } else {
            parent_height - self_height
        };

        let new_x = self_x + offset_x + (self_width / 2.0);
        let new_y = self_y + offset_y + (self_height / 2.0);

        let new_halign = if new_x > parent_width / 2.0 {
            gtk4::Align::End
        } else {
            gtk4::Align::Start
        };
        let new_valign = if new_y > parent_height / 2.0 {
            gtk4::Align::End
        } else {
            gtk4::Align::Start
        };

        let mut changed = false;
        if new_halign != this.halign_target() {
            this.set_halign_target(new_halign);
            changed = true;
        }
        if new_valign != this.valign_target() {
            this.set_valign_target(new_valign);
            changed = true;
        }

        if changed {
            widget.queue_resize();
        }
    }

    /// PORTS: `search_overlay.zig:304-318` `closureMatchLabel`. The match-count
    /// label bind-closure (`search-overlay.blp:44`). `"0/0"` when no total; else
    /// `"{selected+1}/{total}"` (`0` selected when none).
    #[template_callback]
    fn match_label_closure(
        &self,
        has_selected: bool,
        selected: u64,
        has_total: bool,
        total: u64,
    ) -> String {
        if !has_total || total == 0 {
            return "0/0".to_string();
        }
        let sel = if has_selected { selected + 1 } else { 0 };
        format!("{sel}/{total}")
    }
}

impl SearchOverlay {
    /// PORTS: `search_overlay.zig:234-241` `grabFocus`. Grab focus on the search
    /// entry and select all its text (`selectRegion(0, -1)`).
    pub fn grab_focus(&self) {
        let entry = &self.imp().search_entry;
        let _ = entry.upcast_ref::<gtk4::Widget>().grab_focus();
        // `-1` is distance from the end, selecting the entire text.
        entry.select_region(0, -1);
    }

    /// PORTS: `search_overlay.zig:253-258` `setSearchContents`. Set the entry text
    /// and emit `search-changed` with it (so the core search updates).
    pub fn set_search_contents(&self, content: &str) {
        self.imp().search_entry.set_text(content);
        self.emit_by_name::<()>("search-changed", &[&content]);
    }

    /// PORTS: `search_overlay.zig:260-270` `setSearchTotal`. Set the total match
    /// count (`None` = unknown), notifying `search-total` (and `has-search-total`
    /// only when the has-ness flips).
    pub fn set_search_total(&self, total: Option<usize>) {
        let imp = self.imp();
        let had_total = imp.search_total.get().is_some();
        if imp.search_total.get() == total {
            return;
        }
        imp.search_total.set(total);
        self.notify("search-total");
        if had_total != total.is_some() {
            self.notify("has-search-total");
        }
    }

    /// PORTS: `search_overlay.zig:272-282` `setSearchSelected`. Set the selected
    /// match index (`None` = none), notifying `search-selected` (and
    /// `has-search-selected` only when the has-ness flips).
    pub fn set_search_selected(&self, selected: Option<usize>) {
        let imp = self.imp();
        let had_selected = imp.search_selected.get().is_some();
        if imp.search_selected.get() == selected {
            return;
        }
        imp.search_selected.set(selected);
        self.notify("search-selected");
        if had_selected != selected.is_some() {
            self.notify("has-search-selected");
        }
    }
}
