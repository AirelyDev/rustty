# N522 / DECISIONS P10: the app-scoped render `Device` lives in its own allocation

An isolated, **non-FFI** reproduction of where P8-W6 T2 (`40d5b860d`) puts the render device.
It follows the doctrine in memories `rust-mut-args-strongly-protected` and
`shaper-hook-sb-clean-interleave`, and the precedents `../mirihar-n441` (the renderer moved out of
the core `Surface`) and `../mirihar-n444` (the renderer thread and state blocks).

## The hazard

Upstream embeds the device by value in `App` (`App.zig:57-58`, `device: renderer.Device`) and gives
each surface's renderer `&app.device` (`Surface.zig:564`), which the renderer reads on its render
thread (`Metal.zig`: `self.device.device`, `.max_texture_size`, `.default_storage_mode`;
`OpenGL.zig`: `self.device.display`). Zig pointers carry no aliasing rule. In the port the main
thread keeps calling `&mut self` methods on the core `App` (`addSurface`, `deleteSurface`,
`updateConfig`, `deinit`, ...) while render threads run. Each such call holds a PROTECTED `&mut App`
over every byte of the app, device included, so a render-thread read of the device during the call
is Stacked Borrows UB.

## The fix

`rustty_core::app::DeviceBox(*mut rustty_renderer::Device)`: the device in its own `Box::into_raw`
allocation, created in `App::init` at the reference's point and freed in `App::deinit` after the
surface loop and the grid-set assert. `Options.device` is the `*const Device` from
`DeviceBox::as_ptr`. A `&mut App` then covers only the box's pointer bytes. The `#[cfg(test)]`
constructor (DECISIONS P11) builds a null box that `deinit` skips.

## The candidates

| candidate | models | SB | TB |
|---|---|---|---|
| `inline_device` | the literal shape: a render-thread device read inside `App::add_surface(&mut self)` | **UB** | CLEAN |
| `boxed_device` | the port, the same interleaving | CLEAN | CLEAN |
| `boxed_device_lifecycle` | the port end to end: two surfaces rendering through the shared device, destroy one while the other renders, more `&mut App` calls, then `App::deinit` (surfaces, then the device) | CLEAN | CLEAN |
| `boxed_device_null` | the test constructor's null box, skipped by `deinit` | CLEAN | CLEAN |
| `boxed_device_freed_before_surfaces` | negative control: `deinit` frees the device before the surviving render thread stops (the reference's order inverted) | **UB** | **UB** |

`inline_device` is clean under Tree Borrows because TB tracks each byte separately and the main
thread writes only `surfaces` bytes while the render thread reads only device bytes; SB's protected
`Unique` covers the whole `App` and is what the port must satisfy (the N441 precedent). The SB
report (`outputs/inline_device.sb.log`):

```
error: Undefined Behavior: not granting access to tag <1906> because that would remove
[Unique for <4169>] which is strongly protected
   --> src/main.rs:118:26
118 |         let v = unsafe { (*dev.0).read() };
```

The negative control fails under SB (the `Box::from_raw` free conflicts with the render thread's
protected `&Device`) and under TB (`encountered a dangling reference (use-after-free)`), so the
harness sees the lifetime the reference's ordering protects. A clean run is `CLEAN` with no
`memory leaked` report, so the lifecycle candidates also show the device is freed exactly once.

## Running

```
bash run-all.sh      # from this directory; writes outputs/<candidate>.{sb,tb}.log
```

Each candidate runs over 8 scheduler seeds per model (`-Zmiri-many-seeds=0..8`). The committed
`outputs/` are the run recorded in the P8-W6 T2 report.
