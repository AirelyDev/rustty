//! fontconfig-sys — FFI bindings to the vendored Fontconfig 2.18.3 font
//! discovery/matching engine (`links = "fontconfig"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: copies the SAME C library the reference uses (fontconfig 2.18.3, per
//! pkg/fontconfig/build.zig.zon:8) and builds it standalone (`cc` + `bindgen`;
//! see `build.rs`). The host's system fontconfig is deliberately NOT
//! linked — parity is pinned to the version the reference itself builds. The
//! FFI is bindgen-generated from `wrapper.h` — the same single public umbrella
//! header the reference's `pkg/fontconfig/c.zig` @cImports
//! (`<fontconfig/fontconfig.h>`).

// Force-link the vendored FreeType and libxml2. fontconfig is built with the
// FreeType backend (`src/fcfreetype.c` references the FreeType API) and
// `-DENABLE_LIBXML2` (`src/fcxml.c` references libxml2), so the archive has
// undefined references into both. Without an explicit reference to the
// `freetype-sys` / `libxml2-sys` rlibs, rustc prunes them — and with them the
// `cargo:rustc-link-lib=static={freetype,xml2}` directives — leaving those
// symbols undefined at link time. `extern crate` keeps the archives on the link
// line (mirrors the reference's `lib.linkLibrary(freetype)` /
// `lib.linkLibrary(libxml2)`).
extern crate freetype_sys;
extern crate libxml2_sys;

// The bindgen-generated module carries C naming conventions and machine-shaped
// code; suppress the whole surface (this is generated, not hand-authored — the
// clippy/rustc-lint gate applies to our code, not the generated bindings).
#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    unnecessary_transmutes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    dead_code,
    rustdoc::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

// The configuration-directory defines `build.rs` compiles in, tested here
// (S31-F1); `build.rs` includes the same file through `#[path]`.
#[cfg(test)]
mod dir_defines;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int};
    use std::ptr;

    /// R3 known-answer test: prove the vendored fontconfig 2.18.3 links and runs.
    ///
    /// Mirrors the reference's smoke paths (pkg/fontconfig `init.zig` version /
    /// initLoadConfig tests + `pattern.zig`'s "create" test):
    ///
    ///   * `FcGetVersion()` returns exactly `21803` — the fixed known answer for
    ///     the pinned 2.18.3 (`FC_VERSION` in `override/fontconfig/fontconfig.h`;
    ///     the `FC_VERSION_*` defines `build.rs` passes stamp the cache, UD360),
    ///     and proof we linked the vendored archive rather than a host's system
    ///     fontconfig (2.17 reports `21700`, 2.14.2 `21402`).
    ///   * `FcInitLoadConfig()` returns a non-null `*FcConfig` — a LIVE fontconfig
    ///     call that parses the XML config (exercising the `fcxml.c` → libxml2
    ///     wiring end to end) but, unlike `FcInit()`/`FcInitLoadConfigAndFonts()`,
    ///     does NOT scan system fonts and does NOT write the on-disk cache. So
    ///     the KAT is hermetic w.r.t. the compiled-in `/var/cache/fontconfig`
    ///     (which may be unwritable in the sandbox) and needs no installed fonts.
    ///   * `FcPatternCreate()` → non-null; `FcPatternAddString(FC_FAMILY,
    ///     "monospace")` → `FcTrue`; `FcPatternGetString(FC_FAMILY, 0)` →
    ///     `FcResultMatch` with the value read back as `"monospace"`;
    ///     `FcPatternDestroy` / `FcConfigDestroy` tear down cleanly.
    #[test]
    fn kat_fontconfig_2_18_3_init_and_pattern() {
        // Known-answer: the vendored release is exactly fontconfig 2.18.3, whose
        // encoded version is major*10000 + minor*100 + micro = 2*10000 + 18*100
        // + 3 = 21803. If this ever reads 21402, 21700 (or anything else), we
        // linked a different fontconfig than the pinned one.
        // SAFETY: `FcGetVersion` takes no arguments and only reads a compile-time
        // constant; it is safe to call at any time.
        let version = unsafe { FcGetVersion() };
        assert_eq!(version, 21803, "vendored fontconfig FcGetVersion (2.18.3)");

        unsafe {
            // Load the configuration WITHOUT scanning fonts or touching the
            // cache (the reference's `initLoadConfig` path). Non-null on success.
            let config = FcInitLoadConfig();
            assert!(!config.is_null(), "FcInitLoadConfig returned NULL");

            // Build a pattern and round-trip the `family` property.
            let pat = FcPatternCreate();
            assert!(!pat.is_null(), "FcPatternCreate returned NULL");

            // FC_FAMILY is the string macro `"family"`; bindgen emits it as a
            // NUL-terminated byte array, usable directly as a C string pointer.
            let family_obj = FC_FAMILY.as_ptr() as *const c_char;
            let monospace = b"monospace\0";

            let added = FcPatternAddString(pat, family_obj, monospace.as_ptr());
            assert_ne!(added, 0, "FcPatternAddString(FC_FAMILY, monospace)");

            let mut out: *mut FcChar8 = ptr::null_mut();
            let result = FcPatternGetString(pat, family_obj, 0 as c_int, &mut out);
            // FcResultMatch == 0 (the first FcResult enumerator).
            assert_eq!(result, 0, "FcPatternGetString result must be FcResultMatch");
            assert!(!out.is_null(), "FcPatternGetString yielded a NULL string");
            let got = CStr::from_ptr(out as *const c_char);
            assert_eq!(
                got.to_str().unwrap(),
                "monospace",
                "round-tripped FC_FAMILY value"
            );

            // Tear down (the string `out` is owned by the pattern; destroying the
            // pattern frees it).
            FcPatternDestroy(pat);
            FcConfigDestroy(config);
        }
    }

    /// Byte offset of `FcCache.fc_version` in a cache file (`fcint.h:453-466`:
    /// two `int`s, four `intptr_t`s, four more `int`s and the `int64_t`
    /// `checksum_nano` precede it on a 64-bit target: 8 + 32 + 16 + 8 = 64).
    #[cfg(target_pointer_width = "64")]
    const FC_VERSION_OFFSET: u64 = 64;

    /// The stamp fontconfig 2.18.3 writes into every cache file
    /// (`fccache.c:1340-1342`: `(major << 24) + (minor << 12) + micro`).
    const STAMP_2_18_3: i64 = (2 << 24) + (18 << 12) + 3;

    /// Number of fonts a fresh config over `conf` (a fonts.conf document)
    /// finds, using and writing the cache.
    fn fonts_found(conf: &str) -> c_int {
        let doc = std::ffi::CString::new(conf).unwrap();
        // SAFETY: plain fontconfig C API calls on a config this function
        // creates and destroys; `doc` outlives the parse, and the font set is
        // owned by the config and read before it is destroyed.
        unsafe {
            let config = FcConfigCreate();
            assert!(!config.is_null(), "FcConfigCreate returned NULL");
            assert_ne!(
                FcConfigParseAndLoadFromMemory(config, doc.as_ptr() as *const FcChar8, 1),
                0,
                "FcConfigParseAndLoadFromMemory"
            );
            assert_ne!(FcConfigBuildFonts(config), 0, "FcConfigBuildFonts");
            let set = FcConfigGetFonts(config, _FcSetName_FcSetSystem);
            let n = if set.is_null() { 0 } else { (*set).nfont };
            FcConfigDestroy(config);
            n
        }
    }

    /// UD360: the reference passes its stale `build.zig.zon` version, 2.14.2,
    /// as `FC_VERSION_*`, so its cache files are stamped 0x200e002 and a cache
    /// stamped by a real fontconfig 2.18.3 counts as NEWER
    /// (`FcCacheIsNewVersion`, `fccache.c:793-817`): `FcDirCacheMapFd` re-uses
    /// it after its directory changed, and a font added since is not found.
    ///
    /// The cache is stamped as fontconfig 2.18.3 stamps it (for this build that
    /// is also what it wrote), a second font is added, and a fresh config must
    /// find both. With the reference's 2/14/2 it finds one (the stale cache).
    ///
    /// The scenario runs in a child process (this test binary re-executed with
    /// `UD360_CHILD` set) whose environment drops the two variables that change
    /// fontconfig's cache decision: `SOURCE_DATE_EPOCH` (`FcDirChecksum`,
    /// `fccache.c:1190-1221`, records the epoch instead of an older directory
    /// mtime, so both scans would record the same checksum; Nixpkgs' `mkShell`
    /// exports it) and `FONTCONFIG_NO_CHECK_CACHE_VERSION`
    /// (`FcCacheIsNewVersion` skips the version comparison). A fresh process
    /// also starts with no fontconfig state from other tests.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn ud360_cache_stamped_by_fontconfig_2_18_3_is_rescanned_after_its_dir_changes() {
        const NAME: &str =
            "tests::ud360_cache_stamped_by_fontconfig_2_18_3_is_rescanned_after_its_dir_changes";
        if std::env::var_os("UD360_CHILD").is_some() {
            ud360_scenario();
            return;
        }
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env("UD360_CHILD", "1")
            .env_remove("SOURCE_DATE_EPOCH")
            .env_remove("FONTCONFIG_NO_CHECK_CACHE_VERSION")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success() && stdout.contains("1 passed"),
            "UD360 child failed ({}):\n{stdout}\n{stderr}",
            out.status
        );
    }

    /// The UD360 scenario (run in the child process, see above).
    #[cfg(target_pointer_width = "64")]
    fn ud360_scenario() {
        use std::fs;
        use std::io::{Seek, SeekFrom, Write};

        let base =
            std::env::temp_dir().join(format!("fontconfig-sys-ud360-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let fonts = base.join("fonts");
        let cache = base.join("cache");
        fs::create_dir_all(&fonts).unwrap();
        fs::create_dir_all(&cache).unwrap();
        // A font the sibling `freetype-sys` crate ships, embedded so the test
        // also runs where it was not built (the Windows GNU test executables
        // are cross-built); two copies under two names are two fonts to
        // fontconfig.
        const FONT: &[u8] = include_bytes!("../../freetype-sys/res/GeistMono-Regular.ttf");
        fs::write(fonts.join("a.ttf"), FONT).unwrap();

        let conf = format!(
            "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n\
             <fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            fonts.display(),
            cache.display()
        );

        // Scan once: writes the directory's cache file.
        assert_eq!(fonts_found(&conf), 1, "first scan");
        let cache_files: Vec<_> = fs::read_dir(&cache)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().contains(".cache-"))
            .collect();
        assert_eq!(cache_files.len(), 1, "one cache file: {cache_files:?}");

        // Stamp it as fontconfig 2.18.3 does (a system 2.18.3 sharing the
        // cache directory would have written exactly this, in native byte
        // order, like every `FcCache` field).
        let mut f = fs::OpenOptions::new()
            .write(true)
            .open(&cache_files[0])
            .unwrap();
        f.seek(SeekFrom::Start(FC_VERSION_OFFSET)).unwrap();
        f.write_all(&STAMP_2_18_3.to_ne_bytes()).unwrap();
        drop(f);

        // Change the directory in a later second (the cache compares whole
        // seconds of the directory's mtime, `FcCacheTimeValid`).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(fonts.join("b.ttf"), FONT).unwrap();

        let found = fonts_found(&conf);
        let _ = fs::remove_dir_all(&base);
        assert_eq!(
            found, 2,
            "a cache stamped by fontconfig 2.18.3 must be rescanned once its directory changed"
        );
    }
}
