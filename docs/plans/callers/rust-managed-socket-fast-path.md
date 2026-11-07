# Managed-socket fast path for control-free feeds

A text frame on a current `ControlPolicy::None` feed is now admitted once,
not twice. Its traffic and receive bookkeeping run straight from that
admission, without the second admission, policy lookup and live-check pair;
finish, processing and delivery are shared with native frames. Validation keeps
every such feed on `AuthMode::None` and `TrafficPolicy::AnyText`. Idle expiry
and watched-stream silence are decided once at the frame's own `now`; no
venue code runs before receive bookkeeping. An exhaustion stop at admission revokes the epoch and
keeps the ordinary re-admission path, including both capacity checks. Native-control
feeds, binary frames, opens, closes, timers and gated sends keep their existing
path.

Both paths share receive bookkeeping, including the supervisor's data-silence
evidence for `Control::Other` and native `Control::Traffic`. Skipping a redundant
admission must never skip that evidence or refresh it for a revoked epoch.

## Obligations

- None for venues, NX or server callers. Delivered frames, retirements, timers
  and health are unchanged. The engine originals in
  `sdk-rs/engine/src/socket_message_ingress_tests.rs` and the BingX and WOO
  idle-deadline tests pin this behaviour.
- A future change that lets a control-free feed declare auth, a non-any-text
  traffic policy or run venue code between admission and bookkeeping must route
  those frames back through `observe_control`, which admits them again. The fast
  path requires a still-current epoch and the validated control-free policy;
  an admission cannot be retained for a later frame.

## Qualification against c05f56390

Measured against `c05f56390026bc999c4f1b5ff948a85f95395701` on Linux x86_64,
using the same release flags and checkout path for both builds. The baseline
used master's two production engine files; the diagnostic example was identical
on both sides. Baseline and candidate binaries have different SHA-256 hashes.
The optimized sources were restored exactly after building the baseline.

`sdk-rs/engine/examples/managed_frame_profile.rs` isolates managed ingress with
a counting venue and no decoding, network or signing work. Build it with:

```sh
cargo build -j2 --manifest-path sdk-rs/Cargo.toml --release \
  -p safe-cex-engine --features managed-sockets --example managed_frame_profile
```

Each sample ran the saved binary pinned to CPU 15 with
`--frames=20000000` (and `--native` for the control row). Each binary first
warms up 100,000 frames. Sixteen pairs alternate which build runs first.
Whole-child user plus system CPU time comes from
`resource.getrusage(RUSAGE_CHILDREN)` around each child and is normalized by
all 20,100,000 received frames, including warmup. Timed-loop wall results have
the same direction.

| Feed policy | Baseline median CPU ns/frame | Candidate median CPU ns/frame | Median paired change | Candidate lower |
| --- | ---: | ---: | ---: | ---: |
| None | 131.965 | 125.422 | -4.25% | 14/16 |
| Native control | 134.710 | 133.644 | -0.83% | 9/16 |

The Native control row has no consistent direction. The initial, shorter
2,000,000-frame wall-time set was inconclusive: None was lower in 7/16 pairs
and its median paired change was +0.90%. Both sets are retained in the worker
measurement artifacts. The longer run is one separately recorded remeasurement.

Control-free CPU samples, in ns per received frame:

| Pair | Baseline | Candidate |
| --- | ---: | ---: |
| 1 | 129.182 | 125.385 |
| 2 | 131.454 | 125.800 |
| 3 | 133.634 | 125.459 |
| 4 | 130.714 | 127.208 |
| 5 | 130.717 | 125.220 |
| 6 | 132.341 | 124.836 |
| 7 | 134.297 | 130.008 |
| 8 | 133.324 | 136.886 |
| 9 | 134.752 | 135.312 |
| 10 | 133.205 | 125.839 |
| 11 | 140.731 | 124.833 |
| 12 | 127.712 | 123.557 |
| 13 | 131.883 | 124.395 |
| 14 | 130.858 | 124.708 |
| 15 | 127.914 | 124.634 |
| 16 | 132.046 | 125.735 |

Public BingX and WOO correctness replays used identical fixture tapes and
`BTCUSDT` books on both versions. Every serialized output field except the
top-level timing `metrics` block is byte-identical, including checkpoints,
requests, identities and financial data:

| Tape | Identical non-metric bytes | SHA-256 |
| --- | ---: | --- |
| BingX | 3,574,499 | `14e063c3562aabe869780d5c278e2da826a7483f8a7bdaec603261e8ea7ed6a8` |
| WOO | 613,324 | `42d84485a4e72fe9be25bc9c1efbd74ac600f088a672fb09aa8fb7a60b8b35ef` |

The two-venue WASM comparison used `bingx,woo,algos-exec`, workspace release
settings, wasm-bindgen Node bindings and Binaryen 132 `-Oz` twice. Its module
grew from 3,319,456 to 3,319,941 bytes (+485 B). This measures that comparison
build; shipped standalone/combined size budgets and whole-venue Rust/TS CPU
remain CI gates. No budget was changed.
