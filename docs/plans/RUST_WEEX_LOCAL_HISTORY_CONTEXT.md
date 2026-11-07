# WEEX synchronous local history Read custody

Plan-first,2026-10-24; owner `codex-zs5e83`, overseer `claude-ddi2il`.
Explicit handoff from `codex-uufub4`, recorded in its `legacy-burndown.md` final
section. Branch `feat/rust-weex-local-history-context` started at requested
`aa556d59cd904fe6cd81f489fc7680f80cbe578c`, then plain fast-forward merged actual
`a828ede7ebb54bc9c032ea61365213abac5e3dca` at the overseer's direction.
This checkpoint changes only the plan/index; no implementation, test or prune.

## Scope and collisions

Ordinary WEEX Contract V3 USDT perpetual Futures only. This lane owns the seven
synchronous local terminal occurrences inside `src/history.rs::history_query`,
a NEW `tests/request_context_local_history.rs`, this plan and shared docs/exact
inventory prune. Fresh open-PR file lists show no `history.rs` collision.

uufub4 retains all remote history/paging/delay/row/finalizer/owner code, as well
as lib/account/trading/metadata, signer/dispatch/staging and BingX. Do not edit
#1934's Cargo/lib/trading/context and account/trading fixtures; #1939 engine
source; #1747's WEEX account/trading and BingX stack; or BingX trading plan
`dfa6583e541`. New source changes stay within the seven terminal branches plus
imports. No helper finalizer, cfg fallback, API alias, signer or new engine seam.
Recheck held file lists before each source/test phase. Shared doc integration
keeps incoming first and own suffix once; manifest keeps every foreign prune.

## Exact inventory and family routing

At this actual pin WEEX has **42 path fingerprints / 51 occurrences**:
account8F/13O, depth_query2F/3O, history9F/10O, identity2F/2O, lib16F/17O,
secondary_tests1F/1O, trading4F/5O. The appendix lists every path/hash/count.
Held destinations are future engine paths, not claims that these callers are
already managed or authority to edit another owner's area.

The seven selected history fingerprints each occur once:

| Native branch | Full fingerprint |
| --- | --- |
| Keyless private-query `[]` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` |
| Unsupported OrderByClientId | `e1915deeaaaaf8a9e3750ca925ab6babf3d80d0e98872e4fa1e3605b0157faf0` |
| Invalid Fills range | `eedb1e386409cb0cf603aec0da2556ed50ae27b991a5d95dd7803bdcd031fefa` |
| Invalid OrderHistory range | `a71f8e454d2b13bf32e7e5f0480e52dec36d8826551ab16e583fab196fd8a695` |
| Invalid Income range | `57e9ef562b19c29667a0398bc3f7852967c59c395f72ddf62163007a6d41b0ed` |
| Income chunk-budget refusal | `5d609353fd1b58bf8e3fe5a6394312bb4a2fd94bfacec1fba9dbcd044854b8e6` |
| Empty list/request-budget refusal | `fad37049b9b5fba400c6c5026df2ac50477f39d4e606f6edbbf92e51eddb9f7a` |

Expected own shrink: **WEEX42F/51O -> 35F/44O; history9F/10O -> 2F/3O**.
All other maps stay incoming-exact; do not credit #1934's separate trading prune
as this work. The remaining three history occurrences are conservative macro
records: `6d43c66aaf7abcd460dbf1de393805b744df14fbe16199b207541204dfdb892a`:1
(`matches!(request.kind, Kind::Income)`) and
`bbe154e63f6392afb4e62ef9d8cd214a20497c58bcffd3f4811ae22309ede175`:2
(`matches!(request.kind, Kind::Algo)`). They remain untouched with the remote
history owner. A numeric terminal count is not a physical-request count.

## Native decisions and engine path

Real Engine Query entry passes `lib.rs::on_command` through `trade_command`
to `history_query`. Keep that existing dispatch and its native behavior; do not
change the held trade preflight/cleanup. Before the selected replies there is
no HTTP, signing, timer, row publication or resource allocation through Cx.
Some list-budget decisions construct pure native Fetch vectors first; preserve
that geometry and its ordering without issuing an owner on remote fallthrough.

1. If the venue's trusted primary identity is absent, Fills, OrderHistory and
   Income return exact `[]` BEFORE any range/budget validation. This includes
   public-data-only construction and a secondary-only identity. PublicTrades
   and OrderByClientId bypass that empty-result rule. Do not replace the native
   primary-presence predicate with signer readiness or a selected Read identity.
2. OrderByClientId always rejects with exact `WEEX client order lookup is not
   supported by the Rust engine`, regardless of credentials or ID/symbol shape.
   It does not return a non-definitive miss or gain a REST capability.
3. Each private family validates every supplied bound as finite and within
   `[0, 9007199254740991]`, inclusive, and from<=to. Keep exact family messages
   `invalid WEEX fill range`, `invalid WEEX order range`,
   `invalid WEEX income range`. No additional kind or outcome is attached.
4. Income rejects `(to-from)/(100days)>10000` with exact `WEEX income range
   exceeds chunk budget`; equality is allowed. Preserve one-sided/unbounded
   handling, native finite precision and the later held RangeWalk budget.
5. Fills uses7-day windows and OrderHistory90-day windows, each capped at1000.
   An empty generated request list rejects with exact `WEEX history range
   exceeds request budget`. This same occurrence covers both list families;
   no new zero-window success is introduced. Zero/equal valid bounds retain
   their existing one-window remote behavior. All list/query bytes stay native.

Inside EACH selected branch, immediately before publication, consume the genuine
initial `Cx::admit_http_command(Policy)` loan with Read, credential None,
timeout20,000ms, pacing None and RetryPolicy::Never. On either entry error,
return immediately: Rejected has already emitted one typed correlated terminal;
Revoked grants no completion authority. For `[]` use `done_context`; for native
strings use `fail_context`, retaining native kind None/outcome None. Handle the
Result without unwrap/numeric fallback. Keep all remote branches unchanged,
including their existing legacy history owners, signing, routing and pacing;
do not admit an unused root at function entry. There is no shared Plan, session,
history-bridge or signer dependency for synchronous Read terminal ownership.

Caller-visible change: these local replies now require capacity and valid clock
policy. A denied Read emits exactly one engine-owned capacity or InvalidInput
failure without a financial outcome. Successful admission preserves every
native message/result/predicate and introduces no HTTP/sign/WS/timer action,
Store mutation, history marker or new completion companion event.

## Ordered controls, review and commit gates

1. Commit this plan/index and report the exact selected slice to the overseer
   before coding. Independent source/adversarial, caller and full-map plan
   reviews must be clean before writing original REDs.
2. Add only the new focused test file in a separate commit. Use the existing
   trusted `tests/support/setup` provider for real identities; do not add private
   key material, fabricate Contexts or alter shared/held fixtures. Native controls
   cover all seven branches, both list-budget families, keyless/public-data-only/
   secondary-only ordering, exact strings/kind/outcome, upper numeric boundaries,
   NaN/infinities/negative/reversed ranges, equal bounds and budget boundaries.
3. Capacity controls must use256 genuine command Read owners in the SAME engine
   while retaining the keyless case. Existing signed identity Reads can exercise
   credentialed peers, but the keyless case needs no credential workaround:
   a test-only forwarding Venue may intercept a designated fixture command and
   call genuine initial Read admission, retaining the returned Context roots.
   It delegates actual Query commands unchanged to WEEX. These are real engine-
   issued retained command roots at fixed time, not fabricated handles, fake Cx
   or a production wrapper. Assert every admission succeeds, no peer completes,
   the next admission refuses, and no HTTP is claimed or generated by the probe.
   Never claim this no-HTTP probe is a native WEEX HTTP operation.
4. Commit seven original assertion REDs: each local branch must refuse shared
   capacity rather than publish its old result/error. Add invalid-clock controls
   for each branch. Native controls pass alongside those intended failures on
   the same original source; compilation/setup failure is not a RED. Independently
   review that frozen test-only commit before implementation.
5. Recovery retires a genuine peer through a checked Context terminal, then
   verifies each exact native local reply and reuses the freed capacity. Other
   peers remain live; duplicate command IDs cannot complete them. Exercise300
   synchronous completions at fixed finite time, post-completion ID reuse and
   refill256 owners. No expiry may hide retained-owner leaks. Existing private
   remote and PublicTrades requests must still use native routes, body/header/
   sorting/pacing and codec behavior, including valid fallthrough at full
   Context capacity; this slice must not claim authority over remote history.
6. Adopt only the seven inline terminals and imports; rerun the unchanged RED
   assertions. Prune exactly their seven manifest entries, preserving the raw
   remainder and trading/other-venue same-hash siblings. Add CALLERS behavior
   and exact test/ratchet receipts to AUDITS/QUALITY/this plan in the PR.
7. Scoped gates use CARGO_BUILD_JOBS=2: new target first, then WEEX plus Binance
   native suites, strict all-target/all-feature WEEX Clippy, scoped fmt, bans,
   request-context scan and its15 tests. CI runs WASM/replay/size/feature closure;
   no local release builds or replay sweeps. Keep at most one120s CI watcher.
8. Independent source/adversarial, caller and full-map implementation reviews
   must be clean. Architecture gate: No proposed venue cross-cutting mechanism;
   this is native terminal data through existing shared entry and completion
   authority. Actual integration, green CI, an authorized train slot and a fresh
   separate full-map guard plus head-matched merge remain required. This plan
   grants no edit rights over retained remote or open-PR regions.

## Complete pinned WEEX fingerprint appendix

All paths are relative to sdk-rs. Counts include conservative lexical records.
Every unselected fingerprint remains owned by uufub4 or its delegated shared
owner; no implementation/readiness claim follows from its proposed destination.

| Path | Full fingerprint | Count | Destination |
| --- | --- | ---: | --- |
| `venues/weex/src/account.rs` | `081836d9c7754374dd638220175c8309762a6051bf903c4086657abeaa132928` | 1 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/account.rs` | `1780f5566f27a646218b3f4bd05ca0105232d2564f706a1a1b073bd62617aa6b` | 2 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/account.rs` | `66e350394c2f8827347498da5d2d080b56a08e9d8fb58e6643810b8b6868353f` | 1 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/account.rs` | `7b3506f07b5fc4e26ae133a6f96e0e77d2c65bd80f117982dad67dfd800a3c7c` | 1 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/account.rs` | `ad97274a8e870ff3a426895727bd6f3c1a3b32db2a5a53000851ed27da09b3b5` | 4 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/account.rs` | `bbe154e63f6392afb4e62ef9d8cd214a20497c58bcffd3f4811ae22309ede175` | 2 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/account.rs` | `f0ba50adb6e075b899a05e5cc6033e2e9fd34b68771a89d5c56c393c88ef362c` | 1 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/account.rs` | `f647ab9388bbdecfb96530a66f5c9c87a6cd014cbd48e17f4263f7c00d3447e8` | 1 | Held: account/session/snapshot managed Read/Plan custody |
| `venues/weex/src/depth_query.rs` | `7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b` | 1 | Held: public command Read Context/terminal |
| `venues/weex/src/depth_query.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | Held: public command Read Context/terminal |
| `venues/weex/src/history.rs` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | This slice: genuine local Read entry and typed terminal |
| `venues/weex/src/history.rs` | `57e9ef562b19c29667a0398bc3f7852967c59c395f72ddf62163007a6d41b0ed` | 1 | This slice: genuine local Read entry and typed terminal |
| `venues/weex/src/history.rs` | `5d609353fd1b58bf8e3fe5a6394312bb4a2fd94bfacec1fba9dbcd044854b8e6` | 1 | This slice: genuine local Read entry and typed terminal |
| `venues/weex/src/history.rs` | `6d43c66aaf7abcd460dbf1de393805b744df14fbe16199b207541204dfdb892a` | 1 | Held: remote cursor/range Plan history and managed dispatch |
| `venues/weex/src/history.rs` | `a71f8e454d2b13bf32e7e5f0480e52dec36d8826551ab16e583fab196fd8a695` | 1 | This slice: genuine local Read entry and typed terminal |
| `venues/weex/src/history.rs` | `bbe154e63f6392afb4e62ef9d8cd214a20497c58bcffd3f4811ae22309ede175` | 2 | Held: remote cursor/range Plan history and managed dispatch |
| `venues/weex/src/history.rs` | `e1915deeaaaaf8a9e3750ca925ab6babf3d80d0e98872e4fa1e3605b0157faf0` | 1 | This slice: genuine local Read entry and typed terminal |
| `venues/weex/src/history.rs` | `eedb1e386409cb0cf603aec0da2556ed50ae27b991a5d95dd7803bdcd031fefa` | 1 | This slice: genuine local Read entry and typed terminal |
| `venues/weex/src/history.rs` | `fad37049b9b5fba400c6c5026df2ac50477f39d4e606f6edbbf92e51eddb9f7a` | 1 | This slice: genuine local Read entry and typed terminal |
| `venues/weex/src/identity.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Held: signed account Read Context/terminal |
| `venues/weex/src/identity.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held: signed account Read Context/terminal |
| `venues/weex/src/lib.rs` | `067aa233d0ebbb24c837ae5cb4549d6aa68e9f29bf4bdde0ba0630bda9f466c1` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `0d6bbb8e01da4f9bc9a177c0e8ca9d0d015c922780050cfeda6b33807cac8e78` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `1805af84e51e32d19a267749aeea1b85d4e801cb4df04d796274edac35db929d` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `43cf91072f928d61f9f061f65a83f846ce5b37f7a7328105f8f2c390ba4ccf62` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `61034a5650efd3373a0ab46c4948145e4c4022d60a9c7f0898563e273d100159` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `6f0b47296411f75eb3a85b182bf43c08378b387a1a584d8848b0e811f29b01f9` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `869f03854972431807651e3f560b5dbb7232088a7fcc89705e7a58f747bb3e2a` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `93e77dda58930ad87b59e2334bedbf6fbbef499f277e47697e979bc8e474f9e3` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `a40239e5961362c920c8e84905898477b16673e64c7d94eacaa5b51ba63a1152` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `ab3206b428125b95aea7df47c71c9bfb8cef1809ff9c4432ba4ab8f34db5bac9` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `cd66da21afeeef22b98026c1d44238cf77b537c7b5ea67ee5d4fb4464905d893` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/lib.rs` | `fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6` | 1 | Held: managed background/call dispatch and Context terminals |
| `venues/weex/src/secondary_tests.rs` | `f758cbc2754c2b39d2f81d56901f6920f66dcc953a7fe7faa9b5c8f2a5b46c90` | 1 | Held: provider/dispatch fixture provenance |
| `venues/weex/src/trading.rs` | `257046a5e47fb2e691f995b7340e6df7993bb189972fad4218002ab4f7dbee9b` | 2 | Held #1934: Preflight/Plan/journal and Context outcome |
| `venues/weex/src/trading.rs` | `a3929c62720c7893d6477453bb8092ee1dc649a8b5024ffd9867b8715c3afc2f` | 1 | Held #1934: Preflight/Plan/journal and Context outcome |
| `venues/weex/src/trading.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held #1934: Preflight/Plan/journal and Context outcome |
| `venues/weex/src/trading.rs` | `ea183a3f6fb38fae59d7c32102842bce797af23286a824eb9c17fa5cf43b38d5` | 1 | Held #1934: Preflight/Plan/journal and Context outcome |

## Executed original RED and local adoption receipt

Plan `39bf675e7e0` and test-only checkpoint `d5746d2d490` each received independent
source/adversarial, caller and full-map CLEAN reviews; architecture gate No.
The final original run compiled and executed all11tests on unchanged actual
`a828ede7ebb` production source:3native controls PASS and8intentional assertion
REDs (one for each seven selected capacity bypasses plus invalid clock).
An initial fixture compile mismatch and a query-order expectation were corrected
before freezing the original test commit; neither is counted as a regression RED.
Command: `CARGO_BUILD_JOBS=2 cargo test -p safe-cex-venue-weex --test request_context_local_history --no-fail-fast`.

Only the seven inline terminal branches plus their policy imports were adopted.
The same eleven assertions now PASS, including genuine256-root capacity,
recovery and surviving peers, fixed-clock300-cycle reuse/refill, exact native
metadata and precedence, and valid remote wire behavior at full Context capacity.
No held shared support/source, remote owner or dispatch code changed. The raw
manifest removes only this history path's seven selected singleton fingerprints;
its two remote fingerprints/three occurrences remain. Broader native/strict/
ratchet checks, independent implementation reviews and CI are separate gates.

Native gate: `cargo test -p safe-cex-venue-weex -p safe-cex-venue-binance`
passes506tests with0failed/ignored across40result targets (including empty
doctest targets). Strict WEEX all-target/all-feature Clippy, scoped rustfmt,
request-context source scan and all15ratchet tests pass on this adoption.
