# Rust security audit fixes

Branch: `feat/rust-security-fixes`, based on landing `bd94abb524`.
Scope: Security section of `RUST_AUDITS.md`; original audit at `df2d6a4a03`.
Reverify each finding before changing it. No TS venue deletion, credentialed
live trading, weakened parity oracle, or performance threshold changes.

## Phases

1. Hosts (M1–M4 and host Low items). Require deployment-provided per-account
   token before Workers WebSocket upgrade, never trust the first client's
   credentials as authorization. Reject a conflicting start and serialize
   startup before its first await; recheck client membership and generation
   after loading. Catch malformed client JSON and options parsing without
   echoing secrets. Disable redirects in native HTTP, browser/Node fetch and
   Workers fetch (including outbound WebSocket upgrades). Native errors must
   carry no request URL. Bound decompression and polling intervals while
   preserving explicit pause semantics. Regressions exercise unauthorized
   upgrade, conflicting/racing starts, disconnect during loading, malformed
   input, redirect exfiltration, signed URL errors, bombs and interval edges.
2. Live smoke and recording (M5 and tape/scanner Low items). Hard fixed USD
   notional ceiling covers both placement and amendment with native contract
   units; use post-only where the adapter supports it. Environment-specific
   credential names have no mainnet fallback. Replace tape denylisting with
   explicit protocol/header allowlists, strip Set-Cookie from recorder output
   and existing fixtures without refreshing market data, and narrow the
   synthetic-key scanner exemption. Regressions use deterministic fake
   accounts/transports only; preserve replay payload/timing evidence.
3. Signers and credentials (M6 and cryptographic Low items). Allocate monotonic
   nonces atomically across engine objects for each signing identity; HL and
   TxFlow share their underlying protocol signer namespace. Registry keys
   contain public identity or one-way fingerprints, never raw secrets. Retain
   sequence on engine recreation and clock rollback; native threads share the
   registry, Wasm shares within one module memory (independent processes or
   separately instantiated Wasm memories need external coordination). Redact
   credential options Debug, forbid Polymarket credential serialization,
   zeroize owned secrets including error paths, shorten Paradex authentication
   authorization, compare receive windows to TS, and hedge production Lighter
   Schnorr nonces while retaining explicit vector APIs. Test collisions,
   concurrency, drop/recreate, secret formatting, and production nonce entropy.
4. Integration and gates. Independent correctness review of each phase;
   address findings before committing each phase. Run workspace formatting,
   bans, strict Clippy/native tests and Wasm checks/build, host/harness and TS
   quality gates, isolated venue size/performance gates for every changed
   venue plus Binance, private fixture gates, and combined consumer checks.
   No live private evidence is claimed from scripted tapes.
5. PR and landing. Merge current origin landing structure, push and create PR
   into `feat/rust-rewrite`. Post independent code-correctness reviews on the
   PR, fix/retest/review until accepted, merge only with green CI, then create
   a fresh follow-up branch from the landing branch. Strike addressed security
   findings in `RUST_AUDITS.md` with regression evidence and report any dropped
   finding explicitly.

## Progress

- Read current origin audit and venue-port brief before branching; clean
  worktree, no stash. Host, signer, and harness owners independently reverify
  current findings and implement disjoint phases with regression coverage.
- All six Medium findings and listed Low categories remain present on this
  baseline. Receive-window subcases will be compared to current TS rather
  than shortened speculatively.

### Hosts checkpoint

- M1–M4 implemented with token-before-upgrade, one pending/running start,
  post-load membership/generation checks, generic client errors, no redirects,
  and URL-free native HTTP errors. Gzip/zlib output capped at 16 MiB.
- Poll interval command and native typed-event paths clamp to 1000–120000 ms;
  positive infinity/null still pause. Startup-option forwarding follow-up is
  part of the integration/credential phase (the shared bound is exported).
- Independent phase review accepted the host fixes, and identified that
  startup forwarding gap for correction. Native engine 86 tests, native host
  64 tests, JS host 34 tests, TS account transport 257 tests pass. Full JS had
  78 passing before the new generic Wasm parse-error assertion; that assertion
  is deliberately committed with its rebuilt Wasm implementation later.

### Signer and credential checkpoint

- Independent signer review accepted process/module-wide nonce reservation,
  shared Hyperliquid/TxFlow identity, Kraken decoded-key fingerprinting,
  hedged Lighter nonce derivation, redacted options and retained credential
  cleanup. Review found a Phemex clone-before-validation secret copy; fixed
  by validating borrowed fields before creating credential copies.
- Explicit nonce vectors remain unchanged. Production Lighter derives a
  domain-separated HMAC-SHA512 nonce from the canonical key, signed message
  hash and fresh entropy, with rejection sampling; entropy failure refuses.
  Independent nonce tests 3/3 and hedge boundary tests 2/2 pass.
- Paradex signs a 300-second auth lifetime rather than TS's 86400 seconds.
  Original independent signatures remain unchanged in the vector file;
  additional StarknetJS vectors bind the shorter lifetime. Replay verifies
  both preimages and separately enforces the chosen implementation's lifetime.
- Binance/Bybit/MEXC keep TS's 20-second default and now forward a caller's
  shorter receive window. Existing Bitrue and Backpack controls were already
  present; Aster's fixed 20 seconds matches its explicit TS policy. No
  unsupported blanket shortening of venue defaults was applied.
- Internal owned Options and stored credential copies scrub on drop, including
  KuCoin broker credentials and Orderly's retained options/decoded seed. Public
  caller-owned Strings, borrowed JS input, partially failed serde allocations
  and transient copies inside upstream cryptography/allocators are outside
  this ownership guarantee; no universal memory-erasure claim is made.
- All-target workspace check and strict native Clippy passed. Final workspace
  tests and Wasm compilation are running after the last regression additions.

### Smoke and recording checkpoint

- Independent host reviewer accepted the fixed $100 placement/amend cap,
  native-unit sizing, per-mode environment names, post-only support policy,
  protocol field/header allowlisting, cookie stripping and narrow scanner
  exemptions. Review found nested URL/form bypasses; both now have regressions.
- Existing public fixtures changed only to remove Set-Cookie headers: records,
  timestamps and protocol bodies are preserved. No public capture is refreshed
  in response to a performance result.
- A real TS USD-M defect surfaced through the strict scripted smoke roundtrip:
  REST/NEW order mapping and cancel-replace dropped native GTX/PostOnly.
  The paired TS fix preserves TIF through mapping and replacement; two actual
  mapper-to-wire regressions pass. Strict write identity is unchanged.
- Harness 589/589 pass; root quality before the paired TS change passed all
  44 typecheck, 28 lint and 27 test tasks (7679 safe-cex tests in each run).
  An actual scanner export of all 130 changed files found zero leaks; exact
  known public vectors use both a path AND byte-match exemption.
- A final recording review added explicit secondary/read-key credential aliases
  and removed the unused arbitrary `secrets` container from the protocol schema.
  Its new regression passes; final harness count is 590/590.
- Full workspace testing exposed an existing contract for invalid native polling
  inputs: NaN, negative infinity, zero and negative values preserve the previous
  policy. The engine now retains that behavior while bounding positive finite
  values and preserving explicit positive-infinity pause; independent re-review
  accepted the correction. The workspace suite is rerunning after the change.
- Landing advanced during implementation to `1b8592dbd2` (remote payload
  signing and KuCoin Spot parity). Commit the reviewed credential/signing work
  before integration; merge the landing structure and re-audit newly introduced
  signing/credential paths before final tests and all replay gates. Pre-merge
  checks remain checkpoint evidence, not final merged-source gate claims.

### Landing integration checkpoint

- Resolved six merge conflicts against landing `1b8592dbd2`, retaining incoming
  feature gates, delegated signing and KuCoin Spot. Extended credential cleanup
  to the new Spot owner and retained the short Paradex lifetime in delegated
  signatures. Native and Wasm constructors preserve incoming public-only guards.
- New incoming signer/request Debug implementations exposed authenticators and
  callback failures could echo credential-bearing URLs. Added redacted Debug
  and generic external callback failures with regressions; kept actionable
  Workers venue command errors and connection/lifecycle fences.
- Native/engine merged checks pass: 91 engine, 71 native host, 86 standalone
  engine without default features; JS host 41/41. Full JS 87/88 currently has
  only the stale pre-rebuild options diagnostic. The three focused signer
  venue suites pass after fixing a test which reserved its nonce comparison
  bound after the request (it now reserves first and asserts strict monotonicity).
- Integrated fixture proof checks all 39 captures / 304201 records. The incoming
  futures and Spot KuCoin data are preserved, removing only 32 Set-Cookie
  headers across 18 HTTP records. Scanner export: 103 changed files, zero leaks.
- Integrated harness 596/597: existing Phemex delayed-auth 225ms replay deadline
  expired during compilation contention. Preserve this first failure and rerun
  after heavy checks; no protocol assertion, deadline or performance threshold
  has been relaxed.
- Apply Sam's production-impact rule: no audit is judged by TS lifecycle shape.
  Every retained security change addresses account isolation, credential
  exposure, replay/collision risk or uncontrolled financial writes. Default
  receive windows that already matched supported TS policy were not treated as
  bugs. No lifecycle-only parity item is being repaired in this lane.
- Independent integration re-review accepted the new signer ownership, nonce
  assertions, Debug redaction and generic callback errors. It caught one stale
  delegated Paradex Wasm test expectation using the original 24-hour auth hash;
  the test now uses the independent `shortAuth` vector and asserts the actual
  HTTP expiration is 300 seconds. Original vectors remain unchanged.
- Final merged-source root typecheck (44 tasks) and lint (28 tasks) pass; the
  full root test run, native workspace checks, Wasm checks, then all 30 isolated
  replay/build-size gates are queued. The primary matrix is read from current
  CI rather than a hand-maintained venue subset, followed by combined bindings
  and consumer/lifecycle checks.

### Final merged-source quality receipts

- Formatting and banned-pattern checks pass; strict all-target native workspace
  Clippy passes (3m46s). All 2825 workspace tests across 238 suites pass,
  including negative compilation proving Polymarket credentials cannot Serialize.
- Root typecheck 44/44, lint 28/28, test tasks 27/27 pass; safe-cex has
  7758 passing tests in each of its two runners (525 files each).
- Wasm workspace Clippy is running. The serialized 30-venue isolated build,
  size/public/private replay matrix follows it, then combined bindings,
  consumer and fake-account lifecycle checks. No real private trading occurred.

### Consumer landing integration and replay scheduling

- Strict native and Wasm workspace Clippy both passed before this integration.
  Paradex and Binance isolated public/private/size gates passed. Their standalone
  Wasm sizes were 1252329 and 937923 bytes against the unchanged 1258291 budget.
  Public main-thread CPU (TS/Rust) was 1084.5/981.3 ms and 314.8/154.1 ms;
  retained memory was 19.1/2.3 MiB and 18.2/4.1 MiB respectively.
- ApeX and Aster private correctness passed (50 and 14 frames respectively);
  public attempts stopped in TS's final heap sampler before any comparison.
  Original logs remain `/tmp/security-gate-apexomni-public.log` and
  `/tmp/security-gate-asterdex-public.log`; these are not public gate passes.
  Independent read-only review found unchanged sampler/TS behavior and measured
  severe host contention: 10 CPUs, load approximately 160/160/128, 45 Vitest
  workers and simultaneous Rust builds, with 2.72 GiB of swap used. Do not alter
  the sampler, tapes, deadlines or acceptance criteria to obtain a result.
- Stopped orchestration after the current Backpack build completed, before its
  replay. Landing advanced to `fc809a9d85` with shared public-feed consumer
  support; integrate it before restarting gates on the final combined source.
  Existing receipts remain checkpoint evidence. Schedule timed replays after
  host contention subsides; repeated failures under quiet conditions require
  diagnosis rather than repeated acceptance retries.
- Independent integration reviews found no new signer storage/nonce issue.
  Preserve sensitive option ownership, redacted Debug, global nonces and
  hedged Lighter signatures alongside incoming feed valuation behavior.
- The host review found a new production-impact boundary gap: network Workers
  commands could inject trusted-provider `publicFeedUpdate` quotes/books. Add
  rejection and a regression at that network boundary, preserving trusted
  in-process provider delivery and the incoming native server rejection.
- Added the Workers network rejection. Its authenticated-socket regression
  proves fabricated prices never reach the host and ordinary commands still
  work. Intentional negative control (guard disabled) fails; restoring the guard
  passes with `NODE_OPTIONS=--test-name-pattern=Workers.rejects.provider.controls`
  and the existing workspace test script. Logs are
  `/tmp/security-workers-provider-negative.log` and
  `/tmp/security-workers-provider-restored.log`.
  An earlier unfiltered JS run used temporary Backpack-only bindings: the new
  assertion passed, while four unavailable Binance/Phemex binding cases failed.
  The final complete suite must use rebuilt combined bindings.

### Final transport boundary review

- A further correctness review traced raw custom HTTP request/body-reader
  exceptions through `httpFailed` into venue logs and command errors. Sanitize
  those unstructured exceptions at their origin, preserving structured venue
  responses and actionable errors such as insufficient balance.
- Custom WebSocket connect/send exceptions receive fixed diagnostics too.
  Detach failed physical sockets before cleanup, then notify the engine through
  a bounded one-second timer so immediate venue reconnects cannot recurse.
  Symbol timer keys cannot collide with engine IDs; pending-token/physical
  socket checks and disposal prevent late failures closing replacements.
- All 50 mock host tests pass. Both the three HTTP exception regressions and
  the two WebSocket exception regressions intentionally fail against the
  original host and pass after restoration. Logs:
  `/tmp/security-host-http-negative.log`, `/tmp/security-host-origin-restored.log`,
  `/tmp/security-host-ws-negative.log`, `/tmp/security-host-ws-restored.log`.
  Independent read-only re-review accepted the added logic without findings.
- Merged native workspace: strict Clippy passes; 2852 tests in 244 suites pass,
  zero failed/ignored. Wasm workspace Clippy, standalone engine without default
  features, root typecheck (44 tasks) and lint (28 tasks) pass.
- Root JS tests first ended with exit 137 before completion; preserve
  `/tmp/security-v2-tests-sigkill.log`. The cause is not asserted from that exit
  code alone. The controlled rerun serializes package tasks and retains two
  Vitest workers. Turbo's strict environment filters `VITEST_MAX_WORKERS`, so
  the external runner uses `--env-mode=loose` to pass that operational limit.
  No repository code, test assertions, tape content or gate criteria changed
  to obtain a result. The replay matrix waits for these checks and low host
  contention; it did not start after the incomplete JS run.
- Scanner proof at `f479ec6b7d` against landing `fc809a9d85`: 151 changed
  files exported, zero findings. All 39 fixtures / 304201 records match except
  removal of 145 Set-Cookie headers across 120 HTTP records in 12 captures.
  Proof files: `/tmp/security-gitleaks-final-receipt.json` and
  `/tmp/security-fixture-final-proof.json`.
