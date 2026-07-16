# Inspector Subsystem

The inspector is a feature of Rustty that works similar to a
browser's developer tools. It allows the user to inspect and modify the
terminal state.

- The UI toolkit is Dear ImGui, reached through the `dcimgui-sys` crate
  (vendored Dear ImGui + Dear Bindings C API, `links = "dcimgui"`).
- See the full C API by finding the vendored `dcimgui.h` header shipped by the
  `dcimgui-sys` crate: `find . -type f -name dcimgui.h`. Use the newest
  version.
- See full examples of how to use every widget by loading this file:
  <https://raw.githubusercontent.com/ocornut/imgui/refs/heads/master/imgui_demo.cpp>
- Build with `cargo build -p rustty-inspector` to verify API usage.
- There are no unit tests in this package.
