//! libxml2-sys — FFI bindings to the vendored libxml2 2.11.5 XML parser
//! (`links = "xml2"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: copies the SAME C library the reference uses (libxml2 2.11.5, per
//! pkg/libxml2/build.zig.zon:8) and builds it standalone (`cc` + `bindgen`; see
//! `build.rs`). The host's system libxml2 (2.12) is deliberately NOT linked —
//! parity is pinned to the version the reference itself builds.
//!
//! The reference's pkg/libxml2 is a build-only package: it produces the `xml2`
//! archive that pkg/fontconfig links (fontconfig's `fcxml.c` parses
//! `/etc/fonts/fonts.conf`), and ghostty writes no Zig binding over it. This
//! crate therefore exposes only the bindgen surface over `<libxml/parser.h>` +
//! `<libxml/tree.h>` (see `wrapper.h`); its consumer is `fontconfig-sys`, which
//! links the archive and gets the headers via `DEP_XML2_INCLUDE`.

// The bindgen-generated module carries C naming conventions and machine-shaped
// code; suppress the whole surface (this is generated, not hand-authored — the
// clippy/rustc-lint gate applies to our code, not the generated bindings).
// (`unnecessary_transmutes` fires on bindgen's own bitfield accessors for
// libxml2's `xmlParserInput`/`xmlParserCtxt` structs — generated code we do not
// hand-maintain, so it is silenced here rather than patched.)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::os::raw::c_char;
    use std::ptr;

    /// R3 known-answer test: prove the vendored libxml2 2.11.5 links and runs.
    ///
    /// This is the smoke path fontconfig's `fcxml.c` drives when it parses
    /// `/etc/fonts/fonts.conf` (`xmlReadMemory`-class entry → walk the tree →
    /// free): parse an in-memory document, read back the root element's name,
    /// and tear down. No filesystem, no system libxml2.
    ///
    ///   * the archive's own `xmlParserVersion` global reports the version our
    ///     `-D` flags baked in — a fixed known answer, and proof we linked the
    ///     vendored 2.11.5 rather than the host's system libxml2 (2.12.10, which
    ///     would report `"21210"`);
    ///   * `xmlReadMemory` parses the embedded document;
    ///   * `xmlDocGetRootElement` yields an element node named `"root"`;
    ///   * `xmlNodeGetContent` on the child `<a>` yields its text `"1"`;
    ///   * `xmlFreeDoc` / `xmlCleanupParser` tear down cleanly.
    #[test]
    fn kat_libxml2_2_11_5_parse_memory() {
        // Known-answer: `globals.c:182` defines
        // `const char *xmlParserVersion = LIBXML_VERSION_STRING LIBXML_VERSION_EXTRA;`
        // so this reads back, at runtime, the exact value our build.rs `-D`
        // flags baked into the archive. It pins two things at once: that the
        // vendored C compiled with OUR flags, and that we are not accidentally
        // linked against the host's system libxml2 (2.12.10 → "21210").
        //
        // The expected value is "201105", NOT the 21105 upstream's `./configure`
        // would compute: we reproduce the reference's `Version.number()` concat
        // (pkg/libxml2/build.zig:128-130) verbatim, bug included, because R3
        // pins parity to the archive ghostty actually builds. See the long note
        // on `version_number()` in build.rs — the value is provably inert (its
        // only consumer, `xmlCheckVersion`, is unreachable in our link graph).
        // If this assert ever fails with 21105, someone "fixed" the concat and
        // silently diverged from the oracle.
        // SAFETY: `xmlParserVersion` is a `const char *` global in the archive,
        // initialized statically; it is never NULL and points to a static
        // NUL-terminated string.
        let version = unsafe { CStr::from_ptr(xmlParserVersion as *const c_char) };
        assert_eq!(
            version.to_str().unwrap(),
            "201105",
            "vendored libxml2 LIBXML_VERSION_STRING (reference parity)"
        );

        const XML: &[u8] = b"<root><a>1</a></root>\0";

        unsafe {
            // Parse the document from memory. NULL url/encoding, no options —
            // the minimal form of what fcxml.c's parser entry does.
            let doc = xmlReadMemory(
                XML.as_ptr() as *const c_char,
                // The trailing NUL is not part of the document.
                (XML.len() - 1) as i32,
                ptr::null(),
                ptr::null(),
                0,
            );
            assert!(!doc.is_null(), "xmlReadMemory returned NULL");

            // Walk to the root element and check its name.
            let root = xmlDocGetRootElement(doc);
            assert!(!root.is_null(), "xmlDocGetRootElement returned NULL");
            let root_ref = &*root;
            assert_eq!(
                root_ref.type_, xmlElementType_XML_ELEMENT_NODE,
                "root node type"
            );
            let name = CStr::from_ptr(root_ref.name as *const c_char);
            assert_eq!(name.to_str().unwrap(), "root", "root element name");

            // The first element child is `<a>` and its content is the text "1".
            let child = root_ref.children;
            assert!(!child.is_null(), "root has no children");
            let child_ref = &*child;
            assert_eq!(
                CStr::from_ptr(child_ref.name as *const c_char)
                    .to_str()
                    .unwrap(),
                "a",
                "child element name"
            );
            let content = xmlNodeGetContent(child);
            assert!(!content.is_null(), "xmlNodeGetContent returned NULL");
            assert_eq!(
                CStr::from_ptr(content as *const c_char).to_str().unwrap(),
                "1",
                "child element content"
            );
            xmlFree.unwrap()(content as *mut std::os::raw::c_void);

            // Tear down.
            xmlFreeDoc(doc);
            xmlCleanupParser();
        }
    }
}
