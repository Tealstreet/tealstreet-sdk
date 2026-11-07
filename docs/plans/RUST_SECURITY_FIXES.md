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

### Final source quality acceptance

- At source `5a9314df83`, all seven serialized quality stages pass. Root
  typecheck 44/44, lint 28/28 and test tasks 27/27 pass. Both safe-cex runners
  pass 7759 tests across 525 files. The initial exit-137 attempt remains in the
  failure log; the bounded rerun completed in 295.9 seconds without relaxing
  assertions. Native workspace remains 2852/2852 tests across 244 suites,
  strict native/Wasm Clippy pass, and standalone engine is 93/93 across three
  suites. The native session adapter fixture also builds successfully.
- Both HTTP and WebSocket original-boundary negative controls fail as expected;
  restored mock host suite is 50/50. Independent final transport review accepted.
- Scanner refreshed at `5a9314df83`: 151 changed files, zero findings, exit 0.
  Receipt: `/tmp/security-gitleaks-transport-final-receipt.json`.
- The 30-venue isolated build/size/public/private matrix now runs against this
  source, with low-contention scheduling before timed replays. It restores
  combined bindings afterward and runs host, wrapper, consumer, frame/session,
  product, startup/polling, scripted live-smoke and harness checks. Final replay
  gate numbers and hosted CI/PR review receipts remain outstanding.

### Replay scheduling checkpoint

- Final-source Paradex builds successfully: 1222994 bytes against the fixed
  1258291-byte budget. No public replay has started in this final-source matrix
  yet; there is no new failed acceptance attempt to rerun.
- After a read-only scheduling review, external orchestration checks actual CPU
  idle and paging as well as the lagging load average. Admission requires load
  at most twice the logical CPU count, at least 20% idle CPU, and no sustained
  paging pressure over two consecutive 30-second windows. These are scheduling
  conditions only; the sampler, five-second settling deadline, three-run
  medians, fixture bytes and CPU/memory acceptance rules remain unchanged.
- Recent host samples again have zero idle CPU and load above 30 on 10 CPUs,
  so orchestration is waiting before timing. Progress and machine observations
  are in `/tmp/security-v2-all-gates-progress.log`; receipts accumulate in
  `/tmp/security-v2-all-gates.json`. The final-source code is `5a9314df83`;
  later commits only record documentation. All quality and scanner receipts
  above remain valid. PR creation/review/CI/merge remain outstanding.

### Added named logging leaks from the quality audit

- Accepted the Aimux request in `thread-dmam1oibxrui` to include the named
  Bitrue/Bybit logging leaks and an all-notice native regression. Read
  `origin/feat/rust-rewrite:docs/plans/RUST_QUALITY.md` after fetching. Current
  Bitrue still logs a full rejected private frame; current Bybit logs signed
  rejection text and forwards the same `origin_string` through final errors.
  The existing TS Bybit signing regression demonstrates an API key inside that
  protocol field. Native URL stripping already exists; strengthen its proof
  through actual engine/host notices, including both Log and CommandDone.
- Phase plan: preserve fixed subscription/operation diagnostics and numeric
  codes, strip signed-origin/auth payloads at shared rejection boundaries,
  retain ordinary venue business messages, and test every emitted notice for
  sentinel credentials/URLs. Keep request retry counts and trading behavior.
  Independent review found no further demonstrated credential-bearing case;
  broader raw-body redaction and the logging facade remain their later lane.
- Verification order now retains exact isolated bindings/hashes and runs
  builds/size/private correctness before public timing, so untimed work can
  advance during host saturation. The `v3` runner reached six early venues;
  it is paused before Bitrue/Bybit while their fixes are implemented. Public
  timing has still not started. Artifacts/receipts are checkpoints only.
- Fetched landing advanced to `702a62050c`, including broker/cancellation fixes
  and a signing-clock setup repair in the harness. Commit/review the logging
  phase before merging it, preserve security ownership/window/nonce guards,
  then regenerate final tapes/replays and final quality checks on the merged
  source. Existing fixtures need no recapture. No lifecycle-only parity change
  or sampling-deadline/acceptance relaxation is authorized by this addition.

### Named logging phase acceptance

- Bitrue private failures now retain the operation and a parsed numeric code;
  Bybit signing/window failures retain a fixed reason and code, with ordinary
  business rejections still actionable. Retry counts, clock correction and
  account recovery remain unchanged. Six venue regressions inspect every raw
  notice payload for credential/signature/URL sentinels. All six fail against
  the original implementations and pass after restoration; the full account
  suites pass 48 tests. Independent correctness review accepted this phase.
- A real native transport/engine regression covers both connection refusal and
  truncated response bodies through Log and CommandDone notices. Disabling
  connection URL stripping reproduces the leak. Disabling body URL stripping
  does not: reqwest 0.13.5 already constructs stream decode errors without a
  URL. Removed that redundant guard and retained the non-disclosure test;
  this is explicitly not a claim of a reproduced body-stream defect.
- Strict native/venue Clippy and formatting pass. The logging-phase scanner
  exported 185 working-tree files against landing 702a62050c, zero findings,
  without new exemptions. Final merged-source quality/replay/scanner receipts
  supersede these checkpoints. Broader raw-body logging remains a later lane.

### Broker/cancellation landing integration

- Merged landing `702a62050c` at `9fe87d1eed`. Kept incoming broker attribution,
  cancellation preflights and replay signing-clock initialization. The three
  conflicts retain both test sets and Blofin broker forwarding with credential
  ownership transfer. Focused tests and formatting pass.
- Independent correctness review accepted the union: global signer nonces,
  nested credential erasure, caller receive windows, unsigned public broker
  headers and primary/read-key routing remain intact. Named quality-audit log
  leaks are struck separately from the still-open broader raw-body sweep.
- All final quality, private/public replay, isolated size and combined consumer
  receipts are regenerated on this integration. Earlier results remain
  checkpoints; no performance or replay acceptance condition changes.

### Integrated security gate checkpoint (v4)

- Runtime source is `9fe87d1eed`; documentation head `c788d598f3`. Fresh
  formatting/bans, strict native workspace Clippy, 2876 native tests across
  248 suites, and 93 engine tests without default features pass. Actual
  workspace Wasm compilation and strict Wasm Clippy pass. Root typecheck
  44/44 and lint 28/28 pass; the first safe-cex runner passes 7769 tests
  across 527 files. The full root task run remains active, with its second
  safe-cex runner making progress under host contention.
- The first actual Wasm-build attempt failed because macOS stripped inherited
  DYLD_LIBRARY_PATH before the external Python runner started; rust-lld could
  not load libLLVM. Preserve `/tmp/security-v4-quality-attempt1.json` and its
  logs. The runner now sets the pinned Rust sysroot library path inside its
  subprocess environment. Only that environment-failed stage and unexecuted
  stages resume; the five successful source-valid receipts remain intact.
  No SDK source, test assertion, timeout or replay threshold changed.
- Fresh source export against landing 702a62050c: 156 files, zero scanner
  findings, exit 0 (`/tmp/security-v4-gitleaks-receipt.json`), no new exemptions.
  Fixture proof again confirms all 39 captures / 304201 records, with only
  145 Set-Cookie headers removed across 120 HTTP records in 12 captures.
- External v4 orchestration checks indexed, unstaged and untracked runtime
  source before and after stages, stops on failed prerequisites, retains full
  Node/web artifact manifests, and gives the combined bundle no size budget.
  All 30 fresh primary private/public replay receipts, combined consumer
  checks, PR reviews and hosted CI remain pending; no final gate claim yet.

### Integrated quality acceptance (v4)

- All 11 quality stages pass at runtime source `9fe87d1eed`. Native workspace
  2876/2876 across 248 suites and standalone engine 93/93 pass; native/Wasm
  strict Clippy, actual Wasm workspace build, format/bans and native session
  fixture build pass. Root typecheck 44/44, lint 28/28, test tasks 27/27 pass.
  Both safe-cex runners pass 7769 tests across 527 files each. The slower second
  runner completed without retries, relaxed deadlines or assertion changes.
- Receipts: `/tmp/security-v4-quality.json`; the original environment failure
  remains under `security-v4-quality-attempt1.json`. The final matrix has
  started: Paradex is 1222994 bytes against 1258291 and its fresh private
  replay passes. No final-source public timing has started yet.

### Primary private/size acceptance (v4)

- All 30 CI-primary isolated Wasm builds, size gates and fresh private tape
  replays pass at runtime `9fe87d1eed`, with no failed stage in this matrix.
  Largest bundle: Paradex 1222994 bytes against the unchanged 1258291 limit.
- Node/web artifacts and fixture/tape hashes are retained in
  `/tmp/security-v4-gates.json` and `/tmp/security-v4-artifacts/`. The combined
  bundle is rebuilding for the 18 host/consumer/integration checks; it has no
  isolated size budget. Public correctness/CPU/memory and PR/CI remain pending.

### Combined SDK diagnostic regression update

- Rebuilt combined host suite passes 100/100. The Wasm SDK suite exposes one
  stale assertion: 443/444 pass, while Polymarket's partial-batch test still
  expects a raw custom transport exception. The intended fixed diagnostic is
  present and every partial-order metadata field matches. Original failure
  remains in `/tmp/security-v4-combined-wasm-tests.log`.
- Only that integration test changes: inject all synthetic API credentials
  and a signed URL into the exception, require the exact fixed message and no
  credential/URL in Error text/JSON, retain every partial-identity/write/store
  assertion. Removing the HTTP boundary reproduces failure; restoring it
  passes all 16 Polymarket Wasm tests. Package typecheck/lint pass; independent
  review accepted the test. Production, build and fixture inputs are unchanged.
- Preserve all 30 immutable isolated/private/size receipts. The external guard
  may permit exactly this reviewed test path at its pinned post-commit hash;
  no runtime/build/fixture path is excluded. Recheck artifact manifests and
  rerun the full Wasm suite plus all remaining combined checks before public
  timing. No performance, semantic or partial-write assertion is relaxed.

### Combined acceptance and exact scanner fixture policy

- All combined checks pass: 100 host tests, 444 Wasm SDK tests across 16
  files, server/MCP/CLI/React Native loaders, frame/native-session contracts,
  product semantics, startup/polling and Aster/Bitget/spot protocol regressions.
  Scripted smoke 10/10 and harness 601/601 pass, plus MCP build/smoke. All
  31 immutable artifact manifests recheck successfully; runtime remains `9fe87d1eed`.
- Exporting the newly touched Polymarket test exposes its pre-existing public
  synthetic API secret (base64 of `safe-fake-secret`). Preserve the scanner's
  one-finding receipt. Add an exception requiring both that exact public byte
  string and exact test path, consistent with the narrowed vector policy.
  Refreshed export: 157 files, zero findings. A new secret in that same path
  and the known fake secret in another path each remain detected (one finding,
  exit 1). No real credential was identified; no broad exemption is restored.
- Before timed public replays, the external guard separately checks staged and
  unstaged paths, since aggregate `git diff HEAD` can hide canceled index/work
  tree changes. Current status and both diffs were independently confirmed
  clean during accepted checks. This guard improvement changes no SDK input.

### Public timing moved to CI by overseer

- Explicit instruction in Aimux thread `thread-dmam1oibxrui`: the machine is
  shared by 11 lanes; do not wait for local CPU admission. Open the PR now,
  let CI run the 30-venue public timing gates, and merge only on green CI.
- Stopped local admission with zero final-source public timing attempts and
  restored/reverified the combined bindings. Local public receipt explicitly
  says delegated to CI; it does not claim PASS. All local quality, 30 primary
  private/size gates, combined consumers, smoke/harness and scanner evidence
  above remain accepted checkpoints. CI supplies final public measurements.
- Fresh fetch advances landing to `c44f22bd8e` (61 commits after the last
  integration). Open the requested PR and integrate/resolve the latest landing
  structure, review the resulting security boundaries and rely on current
  green merge-result CI before landing. No TS venue deletion.

### PR and latest landing integration

- Opened [PR #1351](https://github.com/Tealstreet/tealstreet-sdk/pull/1351)
  into `feat/rust-rewrite`, then cleanly merged latest landing `c44f22bd8e`
  at `4c6f5d494f`. Incoming wire, leverage-side and order-lifecycle fixes retain
  the landing structure. Earlier local numeric receipts are checkpoints for
  implementation9fe87d1eed; latest merged-source CI governs final acceptance.
- Run independent PR code-correctness reviews for host boundaries, signing/
  credentials, and harness/TS/scanner behavior; post their findings on the PR,
  address findings and repeat. Public timing is exclusively delegated to CI
  by the overseer's shared-machine instruction. Merge only after green CI.

### PR code-correctness review round 1

- Posted independent [host](https://github.com/Tealstreet/tealstreet-sdk/pull/1351#issuecomment-6292577299),
  [signer](https://github.com/Tealstreet/tealstreet-sdk/pull/1351#issuecomment-6292577786),
  and [harness](https://github.com/Tealstreet/tealstreet-sdk/pull/1351#issuecomment-6292578411)
  reviews. Harness/TS/scanner scope accepted. Two concrete findings retained:
  Kraken Spot's original Options copy lacked a scrubbing owner, and native
  direct-proxy/HOW HTTP token-provider errors could forward secret-bearing text.
- Kraken Spot now wraps original options at constructor entry. A test-only
  observing allocator verifies actual original key/secret/read-credential
  allocations remain owned until engine drop, then contain zeros immediately
  before System deallocation, covering valid/incomplete/public-only inputs.
  Restoring the original owner fails on uncleared credential buffer0; fixed
  full Kraken crate passes 138 tests and focused Clippy. Independent post-fix
  correctness review accepted both ownership and regression memory safety.
  Scope excludes caller copies, earlier reallocations, crypto scratch and abort.
- Native token-provider fix/regressions are in progress; public/replay CI is
  green on many initial-head jobs but remains a checkpoint until fixes land.

### PR review fixes accepted

- Native direct-proxy and HOW HTTP token-provider errors now become fixed
  `Proxy authorization failed` diagnostics at each callback origin. Both actual
  Host/Binance regressions inspect every startup Log and command notice, and
  verify no HTTP/auth payload is dispatched. Each origin's raw-propagation
  negative control independently exposes credential/signed-URL sentinels and
  fails; restored tests plus existing connection/body regression pass 3/3.
- Independent code-correctness re-review accepts both fixes. Relay opening
  failures already emit only WsClosed1006, so no unproven relay change is made.
  Successful/empty-token auth behavior, HOW greeting/IP auth and structured
  venue errors remain intact. Scoped strict all-target Clippy for native and
  Kraken passes, as do all 76 native tests and all 138 Kraken tests.
- Push the fixes and require fresh green CI at the corrected head; earlier
  public/variant successes at49f2a35681 do not substitute for that final CI.

### Final CI performance repair

- Corrected-head CI `39435940351` finishes with 39/40 checks passing. All 30
  primary public gates and completed private suites pass; Bybit's later Hyro
  variant fails only steady CPU (TS 5.4 ms, Rust 5.6 ms), with identical output,
  total CPU 38.9/31.5 ms and retained memory 13.5/0.9 MB. Preserve this failure.
- Plan a bounded Bybit book-parser repair: deserialize field names into typed
  identifiers instead of constructing a Cow string for every map key. Preserve
  escaped-key equivalence, duplicate/type/level validation, unknown fields,
  defaults, scratch-buffer reuse and complete-frame validation. Add semantic
  regressions, run Bybit tests/strict Clippy/bans/formatting, and require an
  independent correctness review before the next commit. No ticker API,
  benchmark fixture, sampler or gate threshold changes. CI supplies the next
  performance measurement; no local CPU-admission wait or public timing.
- Wind-down instruction: finish this PR through green CI and merge, write the
  handoff and stop. Do not start queued follow-up work.
- Implemented only the typed key dispatch and three parity tests. Pinned Rust
  1.99 passes all 171 Bybit tests, strict all-target Clippy, formatting and bans.
  Independent final code-correctness review accepts every preserved schema and
  mutation boundary; it makes no speedup claim. Commit this bounded repair and
  require fresh CI measurements before landing.

### Latest landing merge before final CI

- Landing advanced to `f895a98f26` (Paradex partial-placement/error/SBE repairs
  and exchange-listener isolation), making the PR temporarily conflicted.
  Resolve the sole conflict by retaining both independent Paradex
  KNOWN_TS_FIXES comments; their handler remains empty and no oracle is changed.
  Keep incoming structure and all protocol/listener fixes. Security auth
  lifetime/credential behavior and the bounded Bybit parser repair are retained.
- Review the combined boundary and verify focused Paradex/Bybit tests, strict
  Clippy, harness/formatting/bans before pushing; then require fresh full CI.
  The a185 source scan is a historical 161-file/zero-finding checkpoint only;
  scan the committed merged result again before final acceptance.
- Resolved integration passes 296 Bybit/Paradex tests across 19 suite results,
  strict all-target Clippy, workspace formatting, bans and all 601 harness
  tests. Independent merged-boundary review accepts auth/credential retention,
  native partial-identity/error behavior, SBE validation and listener isolation.
  Final CI and a fresh committed-source scan remain required before merge.

### Sparse frame-reader overhead

- CI `39439169933` retains a second Hyro steady failure (TS/Rust 3.2/3.8 ms;
  total 23.3/20.8 ms, memory 13.5/0.9 MB and output pass). Primary Bybit and
  inverse/spot/demo public variants pass. The typed-key repair shrinks its
  bundle by 250 bytes, but establishes no timing improvement across runners.
- Independent static review finds no substantial duplicate work in the Rust
  ticker/store path. The public replay consumes Frame/TickerState directly,
  so a RustExchange/FrameApplier change would not repair this gate. Plan only
  two bounded shared-reader changes: remove delegated book generator layers,
  and visit only set bits in sparse ticker masks. Preserve section ordering,
  lazy book validation, whole-batch delta validation, exact numeric values,
  consumer-replaced columns and missing-column NaNs. Add semantic regressions,
  run the full host JS suite and independent correctness review, then use fresh
  CI for performance evidence. Keep all gate inputs and thresholds unchanged;
  no unchanged-code retry and no local public timing.
- Implemented only those iterator/sparse-loop changes and three guard-focused
  regressions. Full host JavaScript workspace passes 116/116 using Node 22 and
  `corepack yarn`; bans and diff checks pass. Independent code-correctness
  review accepts lazy validation/order, sparse numeric order, NaN/signed zero,
  consumer-owned columns and validation-before-patching. A first agent test
  invocation hit the environment's Volta shim and was stopped; its empty log
  is retained, and the explicit prescribed Corepack invocation passes.
- No local speedup is claimed. Push this bounded reader repair, collect fresh
  CI and scan the new committed source before merging. Broader logging work
  remains outside this wind-down assignment.

### Overseer timing exception and final landing conflict

- Explicit overseer decision in Aimux thread `thread-dmam1oibxrui`: stop
  profiling, cite landing runs `39438422053` at `f895a98f26` and `39433770908`
  at `c44f22bd8e`, and merge this PR once every other job is green. Both
  downloaded landing Bybit logs independently show the identical Hyro steady
  failure (TS/Rust 5.3/5.7 and 5.7/6.3 ms). A separate lane owns that inherited
  timing gate. Preserve its actual FAIL and downstream unexecuted cases;
  no all-gates-pass claim or threshold change. The profiling build was stopped
  with exit 130 before any profiling replay ran.
- Before merge, landing advances to `d17518bc5f` (socket supervisor) and creates
  three conflicts. Keep incoming Bybit field enums (same bounded optimization)
  and both sets of semantic tests, removing duplicate enum definitions. Bitrue
  keeps incoming Supervisor/generation/listen-key recovery and our fixed
  numeric/private diagnostics; retain both test sets, with privacy retries
  following the actual bounded jitter timers instead of retired fixed delays.
- Review automatic credential/nonce/auth merges, run focused engine/Bitrue/
  Bybit/Hyperliquid tests and Clippy plus harness/formatting/bans, commit/push
  the resolved merge, scan it and collect fresh CI under that explicit exception.
  Final handoff records the deferred Hyro gate; start no queued follow-up.
- Resolved merge passes all 472 focused Rust tests across 30 suite results,
  strict all-target Clippy, workspace formatting, bans, and 711 harness tests.
  Independent code-correctness review accepts retained credential erasure,
  signer nonces, short Paradex auth, host boundaries, Bitrue diagnostics and
  complete-item JSON validation. Commit/push for final CI under the explicit
  inherited-Hyro exception; make no profiling or universal PASS claim.

### Final merge race: Binance/Bybit follow-up

- Head `c11f8047c4` completes 39/40 green checks, with only the authorized
  inherited Hyro failure. Reviewed receipts preserve 99 passing replays,
  one actual FAIL, four NOT RUN cases, 3070 native tests and a 163-file clean
  scan. Posted final metrics, then attempted a normal merge; GitHub rejected
  it because landing had advanced to `3686c4ffa1` during CI.
- Integrate incoming Binance/Bybit fixes under standing conflict authority.
  Preserve incoming Binance f64 receive-window API, including explicit zero
  and fractional values required by its newly landed caller regressions.
  This retires our redundant Binance-only u32/1..20000 clamp: the production
  contract now supports those caller values and must not silently alter them.
  Default and shorter-window signing still have independent HMAC regressions;
  Bybit/MEXC tightening, generic parse errors and credential ownership remain.
  Keep incoming auth/history capability behavior and TS forwarding superset.
- Scoped load rule applies: CARGO_BUILD_JOBS=4, touched-crate tests/Clippy/fmt
  and two touched TS test files only; no local release build or replay sweep.
  Independent merge correctness review, commit/push and fresh CI follow. No
  queued follow-up has begun; inherited Hyro timing remains separately owned.
- Resolved integration passes 377 Binance/Bybit tests (30 suite results),
  11 Binance-only native host tests, 13 focused TS tests across two files,
  strict Clippy for touched venues plus Binance-only and Bybit/MEXC host
  features, touched-crate formatting and bans. Independent static review
  accepts the receive-window contract and retained security boundaries.
  Commit/push the resolved merge; all new release/gate work stays in CI.

### WEEX landing and explicit restoration boundary

- Before Rust CI can launch for `5de440fe97`, landing advances to
  `04973f2729` (OKX/Aster/WEEX) and causes actual conflicts. Integrate it;
  keep WEEX's new incomplete-credential guard inside the original Sensitive
  owner, clone read credentials under that owner, and retain our host/TS
  receive-window union and generic error/erasure behavior. WEEX itself adds
  no receive-window option. TS also retains incoming OKX leverage forwarding.
- Detected that incoming landing `0ce5383377` had removed the previously
  landed #1354 Binance/Bybit follow-up (still present at preceding `9279ddc1c0`).
  Overseer confirmed a dedicated restoration lane `claude-8x5tm1`; do not
  restore #1354 broadly here. Preserve only this security PR's Binance
  receive-window signing and regressions, plus security ownership changes.
  The broader removal is existing landing state, explicitly reported.
- Overseer holds other landing merges until #1351 lands (except #1353);
  refresh only on an actual conflict. After focused checks/review, push and
  merge as soon as every other check is green under the Hyro exception.
  Collect detailed receipts after merge to avoid another artifact-download
  race, keeping the feature HEAD unchanged until receipt collection finishes.
- Resolved WEEX integration passes 240 Binance/WEEX tests (22 suite results),
  11 focused native host tests, 13 focused TS tests, touched-venue/host strict
  Clippy, formatting and bans with CARGO_BUILD_JOBS=4. Independent security
  review accepts WEEX partial-credential ownership and retained signing,
  nonce, erasure and error boundaries. No release build or replay ran locally.
- Mandatory revert guard: before merge, with landing integrated, run
  `git diff --stat origin/feat/rust-rewrite HEAD`; inspect every file against
  this PR's intended scope and block on any unexpected deletion/revert. Audit
  our open PR as well. The already-removed #1354 files remain explicitly owned
  by the separate restoration lane, not silently restored/dropped by this PR.
