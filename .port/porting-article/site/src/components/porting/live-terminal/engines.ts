/** EngineId names the two terminal engines the demo can run. */
export type EngineId = "rustty" | "ghostty";

/** ENGINES lists the selectable engines with the wasm each loads. */
export const ENGINES: readonly {
  id: EngineId;
  /** Full name, for assistive technology and the status line. */
  label: string;
  /** Short name of the engine's language, the switch's headline. */
  name: string;
  /** Whose engine it is, the switch's second line. */
  sub: string;
  wasmPath: string;
}[] = [
  {
    id: "rustty",
    label: "Rust engine (Rustty)",
    name: "Rust",
    sub: "Rustty engine",
    wasmPath: "/porting/wterm/rustty-wterm.wasm",
  },
  {
    id: "ghostty",
    label: "Zig engine (Ghostty)",
    name: "Zig",
    sub: "Ghostty engine",
    wasmPath: "/porting/wterm/ghostty-vt.wasm",
  },
];
