# Explicit host entropy

Scope: host-owned entropy at the EngineHost/Engine/Cx boundary. Fresh landing
c63acfc19c2; overseer assignment and construction/signing requirements approved.

The inventory is 23 venue families (ApexOmni, Backpack, Binance, BingX, Bitget,
Bitrue, Bitunix, BloFin, BreakoutProp, Bybit, Coinbase, GRVT, Hyperliquid,
Kraken, KuCoin, Lighter, MEXC, OKX, Paradex, Phemex, Polymarket, TxFlow, WEEX),
core/socket jitter and JS receipt IDs. Crypto uuid() currently hides ambient
getrandom. The Bitget replay hook currently patches global crypto bindings.

1. Add synchronous, fallible engine entropy providers owned per engine, exposed
   through Cx. Keep identity/jitter separate from signing entropy: seeding the
   first must not seed signing unless a test explicitly supplies that provider.
   Missing providers fail construction with a typed VenueError, before start or
   any placement. Keep configuration outside account JSON/command protocols.
   Add explicit seeded test constructors and migrate the existing fixtures;
   never install an implicit fixed seed in production. Preserve Send behavior
   where supported; do not require Sync or borrow WASM memory across JS calls.
2. Bind native production construction to OS getrandom and WASM/browser/CF
   construction to existing WebCrypto imports. Expose EngineHost and native
   host injection before start; reject later replacement. Cover native frame
   server, browser worker, Cloudflare host, CLI and exchange-mcp paths. Provider
   exceptions/bad byte lengths fail safely; no Math.random or weak fallback.
   Use existing dependencies only and check CI WASM budgets.
3. Route every random draw through Cx/host providers. Keep the injected UUID
   formatter's existing NanoID spelling; remove ambient uuid(). Preserve native
   client-ID formats, salt bounds, caller-supplied IDs and failure-before-send.
   Lighter auth/transaction signatures retain message+key hedged nonce derivation
   and zeroization; Coinbase REST/WS JWTs retain fresh cryptographic nonces.
   Request-signing nonces/salts use the signing source; retry jitter may fall
   back to the nominal delay if an installed source fails, never weak randomness.
4. Replace the replay global patch with isolated per-engine seeded injection.
   Preserve Bitget private Rust seeding from exact tape SHA and full raw-ID
   comparisons. Coordinate harness changes with codex-mxb4mq (confirmed no
   overlapping host/replay edits; current tape-time work only flags/manifests).
   Test chunk-invariant native/WASM seed streams, engine isolation, equal native
   client IDs across seeded runs, differing production IDs, explicit signing
   injection, entropy failure before writes, nonce/salt rules and jitter floors.
5. Add a cheap CI source ban against ambient engine/venue/crypto RNG calls.
   Retain required transitive browser entropy backends without treating them as
   runtime draws. Run focused touched-crate tests/Clippy with jobs4, existing JS
   scripts, independent correctness/adversarial audits, full CI and WASM budgets.
   Preserve every unrelated landing change; fresh separate guard, green merge,
   report actual merge pin to claude-ddi2il and the harness owner.

No direct upstream or downstream deployment. New host options are optional at
the JS production boundary (secure defaults), while core construction requires
explicit providers. Public core/registry constructor changes and mechanical
fixture migrations will be listed in the PR with affected native callers.
