# Historical first gate attempt

Measured code SHA: `3148de9e29d84b2530761637fb09761c78b7e15d`, fresh parent `d8ab034625643795919e84c63ee6e5d764cb11df`. This attempt is superseded by the later witness-inventory/manifest-note correction and its own gates. Raw logs and `gates.json` retain the actual measurements; do not interpret this directory as an all-green gate sheet.

Native baseline: 0. Three controls, in order convert/topic/private open, each compiled and failed the named native assertion at runtime (101), then passed after exact restoration (0). Scoped all-target check: 0. Host clippy: **101**, caused by `needless_range_loop` in the witness's post-native inventory loop. Wasm clippy, fmt, writer and extractor were **not run by this gate attempt**, because the runner stopped at host clippy. No workspace or full venue suite was run.

Mutation floor stops at private-open control index 2; no survivor in these three controls. Native fixture floor: 20 converter, 24 topic, 12 open, stopped at open11 promise. Counts use fixed independent expectations; decoded UTF-16 strings/native void/original generic error identity/effects precede inventory assertions. Unicode casing, property lookup/template coercion and websocket class/transport wiring remain explicit dependencies, separately pending.

Independent six-manifest census moved three pending rows to ported, zero to divergent (24 ported, 25 divergent, 58 pending). Enumeration stopped at `BitruePublicWebsocket.listenOrderBook`; census is separate from runtime proof.
