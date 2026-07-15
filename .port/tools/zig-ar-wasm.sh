#!/bin/sh
# WASM archiver wrapper (zig ar). Used only for wasm32-wasip1 via AR_wasm32_wasip1.
exec zig ar "$@"
