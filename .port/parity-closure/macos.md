# Parity closure checklist — macos

8 open finding(s): 0 high, 4 medium, 4 low. Full evidence per row is in [`docs/parity-audit/macos.md`](../../docs/parity-audit/macos.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (4)

- [x] **S15-F2** · CONFIRMED · `INDIV` · missing-test
  - **What:** OUT-OF-SHARD NOTE: TerminalViewContainerTests.swift has no counterpart in rustty/macos/Tests
  - **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-EOF` → **Rustty:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:n/a (absent)`
  - **Audit's suggested fix:** Port macos/Tests/Terminal/TerminalViewContainerTests.swift to rustty/macos/Tests/Terminal/ with the same assertions (rename-only), and add it to Rustty.xctestplan if tests are enumerated there.
- [x] **S16-F1** · CONFIRMED · `ABI` · missing-test
  - **What:** TerminalViewContainerTests.swift (glassAvailability) has no rustty counterpart
  - **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-63` → **Rustty:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:n/a (file absent)`
  - **Audit's suggested fix:** Add macos/Tests/Terminal/TerminalViewContainerTests.swift to rustty as a token-transformed copy of the oracle file (Ghostty->Rustty, ghosttyConfigDidChange->rusttyConfigDidChange), keeping the single @Test glassAvailability with both #expect arms.
- [x] **S17-F1** · CONFIRMED · `ABI` · missing-test
  - **What:** TerminalViewContainerTests.swift (glassAvailability Swift Testing case) has no rustty counterpart
  - **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-63` → **Rustty:** `macos/Tests/Terminal/ (no TerminalViewContainerTests.swift):n/a`
  - **Audit's suggested fix:** Add rustty/macos/Tests/Terminal/TerminalViewContainerTests.swift as a token-renamed copy of the oracle file (Ghostty->Rustty, ghosttyConfigDidChange->rusttyConfigDidChange, @testable import Rustty) and make sure it is included in the Rustty test target / Rustty.xctestplan.
- [x] **S30-F1** · CONFIRMED · `ABI` · missing-test
  - **What:** TerminalViewContainerTests.swift (glassAvailability) has no counterpart
  - **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-63` → **Rustty:** `macos/Tests/Terminal/:n/a`
  - **Audit's suggested fix:** Add macos/Tests/Terminal/TerminalViewContainerTests.swift as the token-renamed oracle file (Rustty.Config, rusttyConfigDidChange); the Tests group is a synchronized root group so no pbxproj edit is needed.

## Low (4)

- [ ] **S15-F1** · CONFIRMED · `INDIV` · other
  - **What:** Comment cites upstream GitHub issue as "Rustty #560" instead of "Ghostty #560"
  - **Oracle:** `macos/Sources/Features/Terminal/BaseTerminalController.swift:483` → **Rustty:** `macos/Sources/Features/Terminal/BaseTerminalController.swift:483`
  - **Audit's suggested fix:** Restore "Ghostty #560" (or "ghostty-org/ghostty#560") in the comment; upstream issue references are proper nouns and should be exempt from the rename.
- [ ] **S17-F2** · CONFIRMED · `INDIV` · other
  - **What:** Doc comment on glassEffectView differs from oracle (doc-only)
  - **Oracle:** `macos/Sources/Features/Terminal/TerminalViewContainer.swift:9` → **Rustty:** `macos/Sources/Features/Terminal/TerminalViewContainer.swift:9`
  - **Audit's suggested fix:** Restore the oracle wording at rustty TerminalViewContainer.swift:9 for R1 comment fidelity (or leave as-is; no behavioural effect).
- [ ] **S30-F2** · DISPUTED · `ABI` · missing-function
  - **What:** Android NDK auto-discovery and sysroot wiring not reproduced; Android lib-vt build never runs
  - **Oracle:** `pkg/android-ndk/build.zig:18-205` → **Rustty:** `.port/tools/gates/check_cross_targets.sh:51-98`
  - **Audit's suggested fix:** Add NDK discovery mirroring findNDKPath/findLatestNDK (env chain + default SDK path + latest semver) in the cross gate / a small xtask helper that exports CC_aarch64_linux_android, AR, CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER from <ndk>/toolchains/llvm/prebuilt/<host>/bin, and add an Android leg to ci.yml (setup-ndk r29 + rustup target add aarch64-linux-android) so the build is actually exercised.
- [ ] **S30-F3** · DISPUTED · `INDIV` · missing-branch
  - **What:** iOS / iOS-simulator SDK and clang-target mappings have no counterpart; VT xcframework is macOS-only
  - **Oracle:** `pkg/apple-sdk/native_link.zig:13-85` → **Rustty:** `xtask/src/main.rs:377-470`
  - **Audit's suggested fix:** When the iOS tier is scheduled: build rustty-vt-c for aarch64-apple-ios and aarch64-apple-ios-sim with IPHONEOS_DEPLOYMENT_TARGET, and fold those slices into `xcodebuild -create-xcframework` in xtask xcframework-vt. Until then, the deferral should be listed in the deferral ledger (no DEFERRALS.md row mentions apple-sdk/native_link/iOS).
