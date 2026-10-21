# Explicit host entropy

WASM production provider lookup prefers `globalThis.crypto`, then callable
WebCrypto providers on `self`/`window`. Partial Node/Bun DOM shims must not shadow
the realm's cryptographic source. Missing providers still fail construction with
a typed error. The actual-WASM regression installs empty `self` and `window`
before the first engine construction; the existing SDK Node suites also cover
their shared empty-window setup without a test-only entropy workaround.

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

## Implementation audit

Construction, channel routing and the final replay shape passed independent
security/correctness review. Native and WASM production factories keep OS/WebCrypto;
all 23 families use Cx. Seeded native Binance IDs reproduce, while production IDs
differ; missing providers and post-start replacement are typed failures.

Both replay modes now create their own exact tape-SHA identity source. Legacy
initializer return values are ignored (Aster returns a module, not a provider).
The global patch and Bitget-only catalog hooks are retired; the original Bitget
SHA/domain/counter byte stream is pinned in tests. Owner codex-mxb4mq approved
this common mode policy and retains full raw-ID comparison with no exclusions.

No Rust assertions were lost. Three tests were renamed; clock-driven jitter checks
now cover host-source repeatability and seed variation, and pass-through checks
prove no bytes were consumed. Coinbase assertions gained explicit signing source
arguments. The Bybit stable-socket fixture crosses 30 seconds by 1 ms to avoid
fractional-start rounding; its reset assertion is retained and independently
reviewed. KuCoin spot/futures cross-engine uniqueness uses distinct explicit host seeds and
adds same-source replay equality, retaining every old assertion. Old JS global-patch/fallback checks now test isolated providers and
typed failure without WebCrypto. Actual-WASM regression tests run in existing CI.

Local strict Clippy/fmt, wasm32 check, SDK typecheck, 142 host and 994 harness
unit tests pass. 4831 unique Rust tests pass across all 32 touched crates (focused runs after
fixture corrections). Landing integration, CI/size gates, a fresh separate revert
guard and merge reporting remain pending.

Integrated landing 4592d985e8a (including Bybit account row-isolation changes)
cleanly in d8be4c45e11. The exact 315-path entropy delta and all own patch lines
are unchanged relative to that landing. 314 integrated Bybit tests, strict
Bybit Clippy and all 994 harness tests pass. Full CI and final guard remain.

Final source window: #1617 and #1621 integrated once at landing7adb43c4beb,
without conflicts. All315 own patch paths/lines are retained exactly, including
the incoming BingX partial-error and Bybit leverage-fence tests/assertions.
Final CI, budgets, fresh separate guard and actual merge provenance are recorded
in PR #1624; deployment and caller changes remain outside this scope.
