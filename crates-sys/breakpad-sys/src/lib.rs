//! breakpad-sys — the vendored google breakpad crash-dump client static
//! library (`links = "breakpad"`), pin b99f444b (the reference's pkg/breakpad
//! build.zig.zon dependency).
//!
//! PORT STATUS: vendored (mirrors pkg/breakpad/build.zig)
//!
//! R3: copies the SAME C++ library the reference uses and builds it standalone
//! (`cc`; see `build.rs`) with the reference's exact per-target source lists.
//! breakpad exposes no C API and the reference consumes it link-only — its
//! pkg/sentry build (`backend = .breakpad`) links the `breakpad` artifact so
//! sentry-native's `sentry_backend_breakpad.cpp` can reach the platform
//! `ExceptionHandler`/minidump writers. Accordingly this crate generates NO
//! bindings: it exists to (a) build/link the archive once (`links`) and
//! (b) re-export the vendored header root as `DEP_BREAKPAD_INCLUDE` for
//! sentry-sys's backend compile.

#[cfg(test)]
mod tests {
    /// R3 known-answer test: prove the vendored breakpad client library was
    /// really built with the reference's per-target source list — the built
    /// archive (path injected by build.rs) must contain the platform
    /// `exception_handler` client object and the shared `minidump_file_writer`
    /// object (pkg/breakpad/build.zig `client_mac`+`client_apple` on macOS /
    /// `client_linux` on Linux). Archive member names appear verbatim in the
    /// `ar` archive bytes (BSD long-name members inline, GNU via the `//`
    /// extended-name table), so a raw byte scan needs no `ar` binary.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn kat_archive_contains_client_objects() {
        let archive = env!("BREAKPAD_SYS_ARCHIVE");
        let bytes = std::fs::read(archive)
            .unwrap_or_else(|e| panic!("reading built archive {archive}: {e}"));
        assert!(
            bytes.starts_with(b"!<arch>\n"),
            "{archive} is not an ar archive"
        );
        for member in ["exception_handler", "minidump_file_writer"] {
            assert!(
                bytes.windows(member.len()).any(|w| w == member.as_bytes()),
                "archive {archive} is missing the `{member}` client object"
            );
        }
    }
}
