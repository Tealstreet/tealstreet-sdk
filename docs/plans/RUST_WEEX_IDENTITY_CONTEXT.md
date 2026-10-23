# WEEX FetchAccount Read entry and terminal custody

Plan-first,2026-10-24. Owner codex-zs5e83; explicit unstarted-slice handoff from
codex-uufub4. Fresh branch feat/rust-weex-identity-context starts at actual
`a828ede7ebb54bc9c032ea61365213abac5e3dca`. The overseer was notified before
coding. This checkpoint changes only the plan/index.

## Scope, collision and inventory

Only `sdk-rs/venues/weex/src/identity.rs`, NEW
`tests/request_context_identity.rs`, shared docs and the exact identity manifest
key. Fresh open-PR lists clear both source and new test. No larger file-clear
eligible slice is available, confirmed by uufub4: lib.rs is held by #1934;
account/trading by #1747 and #1934; remote history belongs to uufub4, with local
history now frozen in #1951. Shared helper managed.rs belongs to9czdjh and stays
unchanged; no helper edit without separate coordination. No lib/account/trading/
history/sign/session/engine edits, no #1939/#1945/G1/G2 dependency.

Actual base WEEX inventory is42fingerprints/51occurrences. This slice removes
exactly identity.rs2F/2O, yielding40F/49O. #1951 independently removes7history
occurrences; its reviewed combined remaining35F/44O becomes33F/42O after this
slice. Do not credit those history prunes here. Both own identity singletons:

- `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548`:
  the legacy finish helper's success arm, now provably unreachable (all callers
  supply Err; successful HTTP already uses managed::complete).
- `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`:
  the finish helper's numeric fail, shared by local refusal and old issuance
  refusal. Replace all callers before deleting the helper; no lexical workaround.

Every foreign map and same-hash sibling stays incoming-exact. Shared docs keep
incoming first plus own suffix once; never rewrite another lane's prune.

## Native decisions and proposed engine path

FetchAccount reaches fetch_identity directly from lib.rs before trade dispatch.
Preserve this dispatch and native validation order:

1. Invalid selected read credentials: exact
   `WEEX read credentials are incomplete in trusted signer setup`.
2. No usable primary or no selected GET signer: exact
   `WEEX account identity requires a non-public primary key`. Secondary-only and
   public-data-only never substitute for a usable primary namespace.
3. Missing primary stableId: exact
   `WEEX account identity requires a trusted primary stableId`, kind Unsupported,
   outcome NotSent. This existing caller error is preserved on successful Read
   admission; canonical shared entry refusals themselves have no financial outcome.
4. Global rate pause: exact `WEEX account reads are rate limited`.

Each local branch consumes the genuine initial Read loan with credential None,
timeout20,000ms, pacing None and RetryPolicy::Never, then calls existing
identity_failed with its original error. That function already emits checked
command_error_context before checked fail_context through managed::reject.
No numeric command_error/fail fallback, no diagnostic after rejected entry and
no unused owner on another branch. Rejected already emitted one engine failure;
Revoked returns immediately. Native admitted errors retain their exact metadata
and one CommandError before one CommandFailed, without HTTP/timer/Store changes.

For valid remote reads, replace the old managed::read call with direct existing
shared initial admission and request_signed_managed at this single native builder.
Choose the same trusted credentials.select(GET) identity and slot, retaining its
exact credential reference. Admit Read with that Credential,20s/Never/no pacing.
Construct the EXISTING dispatch_signing::Http recipe for GET
`/capi/v3/account/accountConfig`, no body. Use its unsigned URL/body and the same
Pending::Identity fields (context, invalid=false, skipped=[], seed_generation).
Queue through request_signed_managed; no pre-signing, legacy request or second
owner. All signing/headers/stamps/route/deadline/publication policy remain in the
existing engine and native recipe; no new wrapper, retry or policy mechanism.

A queue refusal retains the existing managed helper's exact error
`WEEX managed read lifecycle is unavailable`, kind Internal/outcome NotSent,
and uses managed::reject on the original Context without a new companion event,
matching the existing queue-refusal path. Initial capacity/clock refusal now
comes from admit_http_command, one canonical typed failure and no old numeric
finish/CommandError. This closes the pre-authority numeric fallback instead of
trying a second admission after managed::read failed.

Keep identity_response, native JSON/status validation and identity_failed
semantics unchanged. Remote success remains exact primary `{"userId":...}`
regardless of selected secondary Read credentials; optional affiliateId stays
absent. No settings, balance, positions, reconciliation, polling or account
snapshot is mutated by this rare validation read. Preserve late/repeated reply
fencing and native global429 cooldown in held lib.rs.

Caller-visible change: valid shared entry is required for local identity
refusals as well as remote validation. Shared capacity/invalid clock emits one
typed failure without stale numeric diagnostic or financial outcome. Native
admitted metadata and remote bytes/results remain as above.

## Ordered gates and original controls

1. Commit plan/index; independent source/adversarial, caller and full-map plan
   reviews before test edits. Architecture gate: No. The native builder consumes
   existing Cx admission/signing/completion; it creates no per-venue lifecycle.
2. Commit NEW tests only before source. Existing identity.rs tests and held
   support/setup stay byte-exact. Trusted synthetic setup fixtures only; no new
   private keys or fabricated credentials/Contexts. A test-only forwarding Venue
   retains256 genuine Engine command Read roots for capacity, including keyless
   cases, and delegates FetchAccount unchanged. Fixture roots issue no HTTP.
3. Original assertion REDs: independent full-capacity denials for four local
   branches plus valid remote issuance; invalid-clock control checks all branches.
   Expected canonical failure, kind and absence of CommandError are asserted.
   Native working controls pin local precedence, exact companion-before-terminal,
   stableId Unsupported/NotSent, public/secondary-only refusal, rate pause, remote
   primary/secondary HMAC/URL/body/result and unchanged account Store.
4. Recovery retires one genuine peer by checked Context, then repeats local or
   remote command and refills capacity. Duplicate IDs cannot complete peers;
   repeated fixed-clock local calls and a full256refill expose leaked owners.
   Remote success/failure/late duplicate cannot mutate Store or emit twice.
   Existing provider-refusal/HTTP validation tests remain unchanged and run.
5. Freeze original RED checkpoint and independent reviews; only then implement
   identity.rs direct cutover. The original assertions must pass unchanged.
6. Exact two singleton identity-only manifest prune; native WEEX plus Binance,
   strict WEEX all-target/all-feature Clippy, scoped rustfmt, bans and both
   request-context checks. CARGO_BUILD_JOBS=2. No local WASM/replay/perf runs.
7. Record CALLERS/AUDITS/QUALITY and native receipts; three independent final
   reviews; actual landing integration, draft PR and one120s CI watcher per PR.
   #1951's existing watcher stays intact. No merge without train authorization,
   fresh separate full-map guard and head-matched merge.

## Complete pinned WEEX routing appendix

Paths relative to sdk-rs. Every unselected entry remains with its current owner.

| Path | Full fingerprint | Count | Routing |
| --- | --- | ---: | --- |
| `venues/weex/src/account.rs` | `081836d9c7754374dd638220175c8309762a6051bf903c4086657abeaa132928` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/account.rs` | `1780f5566f27a646218b3f4bd05ca0105232d2564f706a1a1b073bd62617aa6b` | 2 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/account.rs` | `66e350394c2f8827347498da5d2d080b56a08e9d8fb58e6643810b8b6868353f` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/account.rs` | `7b3506f07b5fc4e26ae133a6f96e0e77d2c65bd80f117982dad67dfd800a3c7c` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/account.rs` | `ad97274a8e870ff3a426895727bd6f3c1a3b32db2a5a53000851ed27da09b3b5` | 4 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/account.rs` | `bbe154e63f6392afb4e62ef9d8cd214a20497c58bcffd3f4811ae22309ede175` | 2 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/account.rs` | `f0ba50adb6e075b899a05e5cc6033e2e9fd34b68771a89d5c56c393c88ef362c` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/account.rs` | `f647ab9388bbdecfb96530a66f5c9c87a6cd014cbd48e17f4263f7c00d3447e8` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/depth_query.rs` | `7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/depth_query.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/history.rs` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `57e9ef562b19c29667a0398bc3f7852967c59c395f72ddf62163007a6d41b0ed` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `5d609353fd1b58bf8e3fe5a6394312bb4a2fd94bfacec1fba9dbcd044854b8e6` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `6d43c66aaf7abcd460dbf1de393805b744df14fbe16199b207541204dfdb892a` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `a71f8e454d2b13bf32e7e5f0480e52dec36d8826551ab16e583fab196fd8a695` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `bbe154e63f6392afb4e62ef9d8cd214a20497c58bcffd3f4811ae22309ede175` | 2 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `e1915deeaaaaf8a9e3750ca925ab6babf3d80d0e98872e4fa1e3605b0157faf0` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `eedb1e386409cb0cf603aec0da2556ed50ae27b991a5d95dd7803bdcd031fefa` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/history.rs` | `fad37049b9b5fba400c6c5026df2ac50477f39d4e606f6edbbf92e51eddb9f7a` | 1 | Held: #1951 local terminals; uufub4 remote Plan history |
| `venues/weex/src/identity.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | This slice: initial Read/checked terminal; delete unreachable success arm |
| `venues/weex/src/identity.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | This slice: initial Read/checked terminal; delete unreachable success arm |
| `venues/weex/src/lib.rs` | `067aa233d0ebbb24c837ae5cb4549d6aa68e9f29bf4bdde0ba0630bda9f466c1` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `0d6bbb8e01da4f9bc9a177c0e8ca9d0d015c922780050cfeda6b33807cac8e78` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `1805af84e51e32d19a267749aeea1b85d4e801cb4df04d796274edac35db929d` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `43cf91072f928d61f9f061f65a83f846ce5b37f7a7328105f8f2c390ba4ccf62` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `61034a5650efd3373a0ab46c4948145e4c4022d60a9c7f0898563e273d100159` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `6f0b47296411f75eb3a85b182bf43c08378b387a1a584d8848b0e811f29b01f9` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `869f03854972431807651e3f560b5dbb7232088a7fcc89705e7a58f747bb3e2a` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `93e77dda58930ad87b59e2334bedbf6fbbef499f277e47697e979bc8e474f9e3` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `a40239e5961362c920c8e84905898477b16673e64c7d94eacaa5b51ba63a1152` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `ab3206b428125b95aea7df47c71c9bfb8cef1809ff9c4432ba4ab8f34db5bac9` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `cd66da21afeeef22b98026c1d44238cf77b537c7b5ea67ee5d4fb4464905d893` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/lib.rs` | `fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/secondary_tests.rs` | `f758cbc2754c2b39d2f81d56901f6920f66dcc953a7fe7faa9b5c8f2a5b46c90` | 1 | Held: native managed call/Read/Background ownership with existing owner |
| `venues/weex/src/trading.rs` | `257046a5e47fb2e691f995b7340e6df7993bb189972fad4218002ab4f7dbee9b` | 2 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/trading.rs` | `a3929c62720c7893d6477453bb8092ee1dc649a8b5024ffd9867b8715c3afc2f` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/trading.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |
| `venues/weex/src/trading.rs` | `ea183a3f6fb38fae59d7c32102842bce797af23286a824eb9c17fa5cf43b38d5` | 1 | Held: uufub4 #1934/#1747 native Plan/Read custody |

## Original RED and initial GREEN receipt

Plan `af65a1577b2` and original test-only `a55a561b921` received three independent
CLEAN reviews. Bounded test-only refinement `902335f2b6f` removes a no-op
map/collect from the header expectation, preserving its exact equality; three
reviewers cleared it and strict test Clippy passes. The final original run on
unchanged actual a828 compiles/runs9tests:3native controls PASS,6assertion REDs
(four local capacity, remote initial fallback and clock). A fixture Prepare-drain
mistake was corrected before freezing; it is not counted as a RED.
Command: `CARGO_BUILD_JOBS=2 cargo test -p safe-cex-venue-weex --test request_context_identity --no-fail-fast`.

The same nine tests now PASS unchanged. Only identity.rs uses the existing
shared initial admission, checked diagnostic/terminal and signed-managed APIs.
The existing Http recipe, selected credential, exact request/Pending fields and
remote reducers remain native; managed.rs and every held source/fixture stay
untouched. Its removed finish helper had only Err callers; valid HTTP success
already uses managed completion. The raw manifest removes exactly the two
identity singleton records and preserves every foreign entry/byte. Native,
strict, ratchet, final independent reviews, actual integration and CI remain
separate verification gates.

Pre-integration native verification on actual-a828 inputs passes504WEEX/Binance
tests,0failed/ignored, across40result targets including empty doctests. Strict
all-target/all-feature WEEX Clippy and scoped rustfmt pass. The full ratchet
stopped on incoming Aster inventory shrink at newer origin15bd7821b52; no Aster
entry was changed locally. Genuine landing integration and fresh native/strict/
ratchet checks are required before PR certification; no full bans PASS claimed
for this pre-integration checkpoint.

## Actual integration and reviewed verification

Genuine porcelain integration `e336f67b7de` includes actual15bd7821b52.
The sole CALLERS append conflict kept the complete incoming file first plus the
exact own743-byte suffix. Own identity source remains byte-identical to d933;
the original test remains byte-identical to902335. Eight own paths only; every
unowned incoming mode/type/blob and every foreign manifest prune is preserved.
Three independent final source/adversarial, caller and full-map reviews are
CLEAN; architecture gate No. Independent scan confirms40F/49O and identity0/0.

Fresh integrated WEEX/Binance native run passes504tests,0failed/ignored, across
40result targets (including empty doctests). Strict WEEX all-target/all-feature
Clippy, scoped rustfmt, full bans, request-context scanner and all15checker tests
PASS with CARGO_BUILD_JOBS=2. This resolves the historical pre-integration Aster
ratchet failure through the actual incoming source/map, without exemptions.
CI and an authorized train slot remain separate; no merge permission is claimed.
