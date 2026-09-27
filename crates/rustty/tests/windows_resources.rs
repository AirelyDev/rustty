//! Parity S63-F5 on Windows: the `rustty` executable is a GUI-subsystem program
//! with `dist/windows/rustty.rc` compiled into it, as `GhosttyExe.zig:43-49` builds
//! it for a Windows target:
//!
//! ```zig
//! .windows => {
//!     exe.subsystem = .Windows;
//!     exe.root_module.addWin32ResourceFile(.{
//!         .file = b.path("dist/windows/ghostty.rc"),
//!     });
//! },
//! ```
//!
//! The subsystem half is `main.rs`'s `windows_subsystem` attribute and the
//! resource half is `build.rs` (`rc.exe` on MSVC, `windres` on GNU). This reads
//! both back from the built executable (task W-2). Windows-only: the reference's
//! switch has an arm for no other OS, so the test does not exist elsewhere (R14
//! form 1).
#![cfg(windows)]

use std::ffi::c_void;
use std::path::{Path, PathBuf};

const EXE: &str = env!("CARGO_BIN_EXE_rustty");

/// `IMAGE_SUBSYSTEM_WINDOWS_GUI` (winnt.h); `WINDOWS_CUI` (the console) is 3.
const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;

// winuser.h resource types / libloaderapi.h flags.
const RT_ICON: usize = 3;
const RT_GROUP_ICON: usize = 14;
const RT_VERSION: usize = 16;
const RT_MANIFEST: usize = 24;
const LOAD_LIBRARY_AS_DATAFILE: u32 = 0x0000_0002;
const LOAD_LIBRARY_AS_IMAGE_RESOURCE: u32 = 0x0000_0020;

#[link(name = "version")]
extern "system" {
    fn GetFileVersionInfoSizeW(name: *const u16, handle: *mut u32) -> u32;
    fn GetFileVersionInfoW(name: *const u16, handle: u32, len: u32, data: *mut c_void) -> i32;
    fn VerQueryValueW(
        block: *const c_void,
        sub: *const u16,
        buf: *mut *mut c_void,
        len: *mut u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn FindResourceW(module: *mut c_void, name: *const u16, ty: *const u16) -> *mut c_void;
    fn SizeofResource(module: *mut c_void, info: *mut c_void) -> u32;
    fn LoadResource(module: *mut c_void, info: *mut c_void) -> *mut c_void;
    fn LockResource(data: *mut c_void) -> *mut c_void;
}

fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// The PE optional header's `Subsystem` field (offset 68 in PE32 and PE32+).
fn pe_subsystem(image: &[u8]) -> u16 {
    assert_eq!(&image[0..2], b"MZ", "not a PE image");
    let pe = u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&image[pe..pe + 4], b"PE\0\0", "no PE signature");
    // Signature (4) + IMAGE_FILE_HEADER (20) = the optional header.
    let opt = pe + 4 + 20;
    u16::from_le_bytes(image[opt + 68..opt + 70].try_into().unwrap())
}

/// The executable's resources, read through the loader as a data file.
struct Resources(*mut c_void);

impl Resources {
    fn open(path: &str) -> Resources {
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `wide` is a NUL-terminated path; the flags map the image for
        // resource access only (no code runs).
        let h = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                core::ptr::null_mut(),
                LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
            )
        };
        assert!(
            !h.is_null(),
            "LoadLibraryExW({path}): {}",
            std::io::Error::last_os_error()
        );
        Resources(h)
    }

    /// `FindResourceW(MAKEINTRESOURCE(id), MAKEINTRESOURCE(ty))`'s bytes, or `None`.
    fn get(&self, ty: usize, id: usize) -> Option<Vec<u8>> {
        self.find(ty, id as *const u16)
    }

    /// `FindResourceW(L"<name>", MAKEINTRESOURCE(ty))`'s bytes, or `None`.
    fn get_named(&self, ty: usize, name: &str) -> Option<Vec<u8>> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        self.find(ty, wide.as_ptr())
    }

    fn find(&self, ty: usize, name: *const u16) -> Option<Vec<u8>> {
        // SAFETY: `name` is a MAKEINTRESOURCE id (a small integer passed as a
        // pointer) or a live NUL-terminated string; the module handle is live;
        // the locked data is `SizeofResource` bytes long and lives as long as
        // the module.
        unsafe {
            let info = FindResourceW(self.0, name, ty as *const u16);
            if info.is_null() {
                return None;
            }
            let len = SizeofResource(self.0, info) as usize;
            let data = LockResource(LoadResource(self.0, info));
            assert!(!data.is_null());
            Some(std::slice::from_raw_parts(data as *const u8, len).to_vec())
        }
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        // SAFETY: the handle came from `LoadLibraryExW`.
        unsafe { FreeLibrary(self.0) };
    }
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// `exe.subsystem = .Windows`.
#[test]
fn the_exe_uses_the_windows_gui_subsystem() {
    let image = std::fs::read(EXE).expect("read the rustty executable");
    assert_eq!(
        IMAGE_SUBSYSTEM_WINDOWS_GUI,
        pe_subsystem(&image),
        "{EXE} is not a GUI-subsystem executable"
    );
}

/// `addWin32ResourceFile(dist/windows/ghostty.rc)`: every resource the `.rc`
/// declares is in the executable, with the file contents it names.
#[test]
fn the_exe_carries_the_rc_resources() {
    let res = Resources::open(EXE);

    // `RT_MANIFEST 24 "rustty.manifest"` with `#define RT_MANIFEST 1`: manifest
    // resource id 1, the manifest file's bytes verbatim.
    let manifest = res
        .get(RT_MANIFEST, 1)
        .expect("RT_MANIFEST resource 1 (rustty.manifest) is missing");
    let want = std::fs::read(repo_file("dist/windows/rustty.manifest")).unwrap();
    assert_eq!(
        want, manifest,
        "the embedded manifest is not rustty.manifest"
    );

    // `ID_ICON_RUSTTY ICON "rustty.ico"` with `#define ID_ICON_RUSTTY 1`: the
    // icon group id 1, whose entries are the .ico's images (RT_ICON 1..=n).
    let group = res
        .get(RT_GROUP_ICON, 1)
        .expect("RT_GROUP_ICON resource 1 (rustty.ico) is missing");
    let ico = std::fs::read(repo_file("dist/windows/rustty.ico")).unwrap();
    // GRPICONDIR and ICONDIR share their 6-byte header (reserved, type 1, count).
    assert_eq!(
        &ico[0..6],
        &group[0..6],
        "icon group header differs from rustty.ico"
    );
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    assert!(count > 0);
    for i in 0..count {
        // GRPICONDIRENTRY is 14 bytes; its last u16 is the RT_ICON id.
        let e = 6 + i * 14;
        let id = u16::from_le_bytes([group[e + 12], group[e + 13]]) as usize;
        // ICONDIRENTRY is 16 bytes: size at +8, file offset at +12.
        let f = 6 + i * 16;
        let size = u32::from_le_bytes(ico[f + 8..f + 12].try_into().unwrap()) as usize;
        let off = u32::from_le_bytes(ico[f + 12..f + 16].try_into().unwrap()) as usize;
        let image = res
            .get(RT_ICON, id)
            .unwrap_or_else(|| panic!("RT_ICON {id} (image {i} of rustty.ico) is missing"));
        assert_eq!(
            &ico[off..off + size],
            image.as_slice(),
            "icon image {i} differs"
        );
    }

    // `VS_VERSION_INFO VERSIONINFO`: the two string values the `.rc` keeps,
    // UTF-16 in the StringFileInfo block, under id 1 (`#define VS_VERSION_INFO 1`,
    // the R13 fix of UD331).
    // CORRECTION (task W-2b, review W-2 F1): this looked the resource up by the
    // STRING name `VS_VERSION_INFO`, which is what the reference's `.rc` (no
    // define) produces and why Windows showed no version information.
    let version = res
        .get(RT_VERSION, 1)
        .expect("RT_VERSION resource 1 (VS_VERSION_INFO) is missing");
    assert!(
        res.get_named(RT_VERSION, "VS_VERSION_INFO").is_none(),
        "the version resource is still named by the string VS_VERSION_INFO (UD331)"
    );
    for (key, value) in [
        ("OriginalFilename", "rustty.exe"),
        ("ProductName", "Rustty"),
    ] {
        assert!(contains(&version, &utf16(key)), "version info lacks {key}");
        assert!(
            contains(&version, &utf16(value)),
            "version info lacks {key} = {value}"
        );
    }
}

/// What a user sees (UD331, R13): Windows' own version API finds the
/// `ProductName` the `.rc` declares (`\StringFileInfo\040904b0\ProductName`).
/// With the reference's `.rc` (no `#define VS_VERSION_INFO 1`) there is none,
/// as `(Get-Item rustty.exe).VersionInfo.ProductName` was empty (measured, task W-2).
#[test]
fn windows_reads_the_product_name() {
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let exe = wide(EXE);
    let mut handle = 0u32;
    // SAFETY: `exe` is NUL-terminated; `data` is `size` bytes; `VerQueryValueW`
    // returns a pointer into `data` and the value's length in characters.
    let product = unsafe {
        let size = GetFileVersionInfoSizeW(exe.as_ptr(), &mut handle);
        assert!(
            size > 0,
            "GetFileVersionInfoSizeW: {}",
            std::io::Error::last_os_error()
        );
        let mut data = vec![0u8; size as usize];
        assert_ne!(
            0,
            GetFileVersionInfoW(exe.as_ptr(), 0, size, data.as_mut_ptr().cast()),
            "GetFileVersionInfoW: {}",
            std::io::Error::last_os_error()
        );
        let sub = wide(r"\StringFileInfo\040904b0\ProductName");
        let mut buf: *mut c_void = core::ptr::null_mut();
        let mut len = 0u32;
        assert_ne!(
            0,
            VerQueryValueW(data.as_ptr().cast(), sub.as_ptr(), &mut buf, &mut len),
            "VerQueryValueW found no ProductName"
        );
        let chars = std::slice::from_raw_parts(buf as *const u16, len as usize);
        String::from_utf16_lossy(chars)
            .trim_end_matches('\0')
            .to_string()
    };
    assert_eq!("Rustty", product);
}
