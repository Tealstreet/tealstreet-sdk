# Phemex dispatch HMAC signing migration

Plan-first,2026-10-24; owner codex-zs5e83; HMAC reviewer claude-4yjlw0.
Fresh branch feat/rust-phemex-dispatch-signing starts at actual
`15bd7821b52e378a9e32a60ecb897275c52109a4`. Read with RUST_SIGNER.md,
RUST_SIGNER_PREREQUISITES.md, RUST_PHEMEX_REQUEST_CONTEXT.md and the caller,
audit and quality ledgers. This checkpoint changes plan/tests only.

## Scope and owner coordination

The final ruling keeps dispatchSigning OFF and forbids partial injected arms,
key copy-back or dual raw providers. It supersedes the additive-only suggestion: **no recipe-only
ship**. Complete the ordinary native funnel and opaque provider custody together,
after the engine-minted uncapped raw-expiry seam lands. Commit the plan and
existing-API original REDs now; native write attachment remains held. ih35js may
continue only unsigned public-trades adoption meanwhile. No UTC-to-raw venue
projection or capped repair-bound substitution is authorized.

- ih35js owns history.rs, its bounded-history test and only the exact
  Pending::History stream/response/failure/typed callback branches in lib.rs.
  No claim on sign.rs, provider/setup or the common request funnel.
- 5c57jl owns whole public/private socket lifecycle, socket_codec.rs and the
  account connect/open/ping/close/timer/auth/snapshot glue. Explicit clearance
  covers ONLY replacing account_open's credentials.websocket_auth/send call with
  send_signed and a native WsAuth recipe. Preserve next_id1200, ping ordering,
  auth/snapshot reducers, accepted-epoch checks and the native deadline. Whichever
  integrates second composes the epoch lookup and signing hook with both owners.
- This lane owns sign.rs/new dispatch recipes, trusted construction/provider
  adapter, dedicated signer tests, common HTTP request construction and the
  signing-only write receipt/failure integration. No history/account financial
  reducer, socket executor or protocol retry rewrite.

Open PR files were checked before the plan. Phemex source/test paths are clear;
shared registry/setup/tooling files have other family PR claims. The overseer
explicitly approved distinct Phemex-only hunks: replace the Phemex registry arm,
add one Phemex dispatchSigning declaration and registry Cargo feature edge, plus
one exact temporary provider staging bootstrap and its exact checker test row.
Notify #1916/#1773/#1868 claimants; preserve all incoming arms/rows. #1916's
apparent shared claim is already-landed Aster ancestry; its owner confirmed no
own overlap. #1868 adds Bitunix setup policy; keep the Phemex row away from it.
Host admission remains disabled, like BingX: no hosted_signing.rs family enable,
legacySigning.ts support claim or remote-provider wire change in this migration.

## Exact native contract

REST signs `path + decoded_query_without_question_mark + expiry_seconds + body`.
Decode percent escapes as the existing native codec does; literal plus remains
plus. Preserve the native invalid-UTF8 fallback, percent case, original query
order and exact transmitted query/body bytes. Signing never normalizes the URL.
Expiry is `floor((protocol_now + 60000)/1000 + 0.5)`, preserving the native rounded
boundary. Use DispatchContext's full calibrated protocol time, not an enqueue
clock, provider clock, caller override or ad hoc offset.

Header order remains X-PHEMEX-ACCESS-TOKEN, X-PHEMEX-REQUEST-EXPIRY,
X-PHEMEX-REQUEST-SIGNATURE, Content-Type:application/json. Signature is lowercase
hex of32 raw HMAC-SHA256 bytes. API credential presentation uses provider Present;
only the provider owns secret material. The recipe attests exact unsigned method,
URL, headers and body both before canonical work and before applying results.
Validate full result count, receipt, UTF8/bounded header presentation and raw MAC
length before changing outbound bytes. Default callback-header policy is deny.

Ordinary GET selects Secondary else Primary. Active Orders reads (account.rs activeList/OrdersRead, lib.rs:208-209) and
client-ID lookup (history.rs::Request::is_lookup, lib.rs:208-209) explicitly
select Primary even on GET; identity.rs:75 validates the secondary container
and otherwise uses ordinary GET selection. Pin a separate control per path. Other methods select Primary.
Public products/tickers/fullbook/credentialed candles/public-trades requests
currently sign whenever selected credentials exist; retain that optional auth.
The existing true unsigned public-candle path stays unsigned. Preserve REST/testnet
hosts, empty query/body behavior, stream selector, primary route and existing
native codec/retry behavior. This migration does not create a new REST capability.

WS user.auth signs provider-owned credential bytes followed by rounded expiry.
The key occurs twice: provider Present is spliced into JSON params[1], and a
Credential payload part supplies the MAC prefix. Validate exactly two results
(Present,32-byte MAC), bounded UTF8 JSON credential presentation and the exact
receipt before apply. Serialize with serde; never hand-concatenate key material.
Preserve method/params/id field order, params(API,key,lowercase-hex MAC,expiry) and
id999. Sign and serialize one identical rounded expiry from one protocol-time
observation. Use send_signed on the live socket. Shared engine custody owns async completion, deadlines, epoch revocation,
stale reprepare and release; no venue signing timer, retry queue or nonce cache.

## Intended recipe and provider shape

Export `dispatch_signing::Http` with immutable opaque Identity, network, Method,
exact path, raw query and optional body. Its unsigned() returns the exact native
intent for both the ordinary staging funnel and ih35js's managed read helper after full cutover.
prepare uses existing shared HMAC-SHA256 work; apply inserts only validated native
headers. The sign receipt is expiry-only (`stamp_ms: None`): expiry
MUST equal the rounded seconds actually signed/sent multiplied by1000, not
protocol_now+60000. This retains the native half-second boundary exactly.
A corresponding WsAuth recipe owns only the opaque identity and native request ID.
Malformed percent escapes (%zz, trailing %, %4) pass through literally in native
decoded_query; only invalid decoded UTF8 falls back to the entire original query.
Pin each separately, plus literal+ and percent-encoded multibyte bytes; do not
replace this with TS decodeURIComponent throwing behavior. Credentialed GET keeps
all four headers in native order; truly unsigned requests have no auth/Content-Type.

The generic LocalSigner already supports HMAC-SHA256; no algorithm fork, crypto
Cargo feature or cryptographic dependency is needed. Prove injected Host and local
providers through the same engine dispatch, including malformed/refused results.

The final native Options retain refs and public metadata only. Trusted native/SDK
setup consumes raw inputs before venue construction; registry provider installation
and Phemex feature edge follow the landed BingX pattern. Keep declaration and
runtime cutover coherent: enabling dispatchSigning strips raw setup material, so
it must not activate while legacy WS/write signing still needs those bytes.
No key copy-back, silent fallback or partial private-account support flip.

## Hard gate: uncapped raw signature expiry

Phemex trading::Request.signed_until drives unknown-write reconciliation after
signature validity plus the native5s skew allowance. Today it is computed eagerly
from Credentials::expiry(cx.now). Delayed signing or protocol calibration makes
that enqueue value untrustworthy.

uufub4 confirmed actual15bd has no reviewed legacy release hook exposing raw expiry;
#1939 does not add one. on_signing_receipt supplies protocol UTC and no raw release
observation. Managed HttpRelease.signature_valid_until is capped by the lifecycle
bound and therefore cannot prove a60s signature expired after a20s HTTP deadline.
Neither is a valid replacement for Phemex uncertainty not_before.

The overseer assigned a separate engine PR to uufub4. Required contract: engine-
minted final-release metadata on the exact Pending target, validated clock revision,
uncapped raw projection of actual signature expiry, no Cx/send/retry authority,
no hook for stale/reprepared/refused work, and fail-closed missing/invalid expiry.
Await its reviewed API and actual landing; do not guess fields or implement it
inside a venue. Positive/negative calibration, delayed provider, signature expiry
later than lifecycle and raw deadline precedence need core and real Phemex controls.

Provider failure on an unsent write preserves its typed error and optimistic
Store rollback. Earlier effective legs, accepted native IDs or sent retry attempts
must prevent a later NotSent from laundering the command. Review the exact native
aggregation integration with 4yjlw0 using #1773/#1915's pattern and the landed error
classifier; no local HTTP-status classification or new retry ladder is introduced.
Financial reducers and post-release network failure semantics stay native.
Fixed review target: Bitget #1915 `86f528aaef8`, trading.rs around2124,
trade_sign_failed/trade_fail; resolve the precise Phemex adaptation with4yjlw0
without losing earlier effective legs, native IDs or the original rollback error.

## Additional WS ordering gate

Native account_open sends auth, sets next_id1200, then immediately calls
account_ping. Merely replacing auth with deferred send_signed can release the
ordinary ping before provider completion: current dispatch emits Action::Sign
and continues draining ordinary WsSend actions. The narrow replacement alone
is not ordering proof. Add an original control that parks the auth provider and
requires auth-before-ping after successful completion, and no credentialed auth
or speculative ping before it. Coordinate a reviewed ordering solution with5c57jl
and the engine owner; no per-venue signing queue/timer or guessed release hook.
The existing native supervisor/accepted-snapshot deadline must remain bounded.
This gate is additional to uncapped raw expiry; production stays held until both
contracts have reviewed consumer-fit controls.

## Ordered phases and original REDs

1. Commit plan/index and obtain independent source/adversarial, caller and full-map
   reviews plus HMAC-owner review. This is not a completed custody cutover.
2. Add NEW existing-API tests only, before production. Real Phemex commands must
   show that a deferred/refusing/missing provider controls signed REST release;
   old source's eager HTTP is an assertion RED, not a compile failure. Fixed native
   HMAC/query/expiry/WS vectors, secondary selection and true unsigned reads are
   working controls. A separate registry target pins opaque provider admission,
   missing-provider rejection and no raw credentials in generic construction.
3. Commit the original RED checkpoint and record precise first failing assertions;
   downstream release/receipt assertions blocked behind initial eager HTTP are
   not separately executed proof. Do not invent the absent raw-expiry API for a test.
4. **Hold full source attachment until the shared uncapped-expiry seam lands.**
   Recheck open PRs and current landing; freeze a reviewed concrete API before
   production. Add native delayed/calibrated write controls against that actual API.
5. Implement full native HTTP/WS funnel and trusted setup as one coherent cutover,
   preserve existing fixtures via trusted setup/oracle adapters, consume actual
   release metadata, then prune only proven Phemex material sites. The approved
   legacy sign_http bootstrap remains a remove-before-master obligation until
   each native caller moves to managed Context ownership.
6. Source/adversarial, caller/full-map and 4yjlw0 reviews must be CLEAN. Run scoped
   Phemex+Binance native tests, strict Clippy/fmt, signing/request ratchets and their
   tests. Actual SDK construction/typecheck/lint tests follow touched paths. CI
   owns WASM release/replay/size/timing; no new profiles, budgets or re-tape.
7. PR into feat/rust-rewrite, one slow CI watcher; green actual integration and an
   explicit train slot precede a separate full-map guard and head-matched merge.

Append CALLERS with actual landed setup/API/expiry behavior in the implementation
PR. No production/inventory reduction is claimed by this plan or original REDs.
Architecture gate: native canonical recipe and adapters use shared core custody;
no per-venue expiry, clock projection, scheduler, crypto or error-policy mechanism.

## Executed original assertion checkpoint

Test-only checkpoint `5f9e6ab8cd7` compiles and runs16 native tests on unchanged
actual15bd production:6 native/provider controls PASS,10 intended assertion
REDs,0ignored. The separate registry target compiles/runs3tests:2controls PASS,
1assertion RED,0ignored. Commands from sdk-rs, with CARGO_BUILD_JOBS=2:

```sh
cargo test -p safe-cex-venue-phemex --test dispatch_signing
cargo test -p safe-cex-venues --no-default-features --features phemex,dispatch-signing --test phemex_signer
```

Native REDs expose provider bypass/refusal, missing-provider local fallback,
eager read/write/WS release and raw-clock signatures under calibration. The
registry RED is the existing unsupported injected-provider rejection. Working
controls retain fixed HMAC/query vectors, ordinary Secondary and lookup/repair
Primary selection, truly unsigned headers and exact native auth-before-ping wire.
Fractional dispatch cases at +499/+500/-30000.4 require the receipt to equal the
rounded signed expiry seconds*1000 and compare complete native headers. They
currently stop at the initial eager-HTTP assertion; deeper receipt/release and
deferred auth ordering assertions are regression requirements, not executed proof.

Fixture compilation/bootstrap mistakes were corrected in follow-up commits and
are excluded from RED evidence. Both scoped strict test Clippy targets, scoped
rustfmt and diff checks pass. Source/adversarial, caller and full-map rechecks
at5f9 are CLEAN; HMAC owner claude-4yjlw0 also cleared its bounded recheck at5f9.
Architecture gate No for this test-only checkpoint; runtime remains held. No
production, manifest, feature, provider, construction or scanner changes exist.
No CI, replay, WASM, size, cutover or inventory-shrink PASS is claimed.

Shared prerequisite #1968 plan/original tests at093a9d0ad45 received read-only
Phemex consumer/API clearance: immutable engine-minted uncapped expiry at exact
final release, additive hook with old-hook delegation once, and unchanged capped
managed repair bound. Required expiry refusal must occur before release; a void
observation hook cannot abort transport. This is not runtime or actual-landing
clearance. Both that actual seam and reviewed WS auth/ping ordering still gate
all Phemex production attachment; dispatchSigning remains off.

## Pinned inventory and routing

Signing manifest at15bd: Phemex lib.rs14F/14O. Legacy sign.rs is excluded by the
unmigrated-family scanner, so those counts alone do not prove secret custody.
Phase2 adds Phemex to the migrated-family scanner and removes all14 lib material
sites, including the old private unit fixture. Any declarative provider-work
sites are reviewed descriptors, not frozen primitive-signing exceptions; no
raw HMAC or secret holder may remain in production. Recompute exact descriptor
map against the approved tooling before claiming a reduction.

Request manifest at15bd:35F/39O, not a signer-only burn-down claim. Account3F/3O;
history6F/9O (ih35js); identity3F/3O; income2F/2O; lib17F/18O; trading4F/4O.
The overseer approved one temporary provider attachment. Its exact fingerprint
SET and before/after counts (including any moved request or terminal site) must
be frozen once the source exists, with the exact approved checker-dict row; do
not pre-claim one added fingerprint or an inventory shrink before then.
All unrelated and foreign maps remain incoming-exact. Native RC terminal adoption
belongs to its venue caller slices; do not relabel those sites as signer prunes.

Signing fingerprints and destinations:

| Fingerprint | Existing line | Destination |
| --- | --- | --- |
| `31c4a378f691bfb9a36c5bfab06eefdbc87243936860af8d8bc1ff86e6060e84` | `pub secret: Option<String>,` | Opaque Options/provider-owned setup; remove old material site |
| `998051a7b82dfb9002576f90c080a63ad097bd78ea4f1b21f8ef67b3e1d30152` | `impl safe_cex_crypto::secrets::EraseSecrets for Options {` | Opaque Options/provider-owned setup; remove old material site |
| `3d0b1da9613f86f2f90197fe3d09161f9b775bbcd3eb7d61c0bbf4caf0f10573` | `fn erase_secrets(&mut self) {` | Opaque Options/provider-owned setup; remove old material site |
| `0219bc24bc618541b8bf84b12a617687f15c479a2a90d9a765b85c28878bfd7c` | `safe_cex_crypto::secrets::erase(&mut self.key);` | Opaque Options/provider-owned setup; remove old material site |
| `56b2f68f6e9d2a7e8d8d9c12697428fa0218289b90f6689ffb620baf490041e4` | `safe_cex_crypto::secrets::erase(&mut self.secret);` | Opaque Options/provider-owned setup; remove old material site |
| `b44b8c98339cc77eb4fd6a0ec89fa0c5a55f0dd263d57dfa5dc6fc3031594c04` | `read.erase_secrets();` | Opaque Options/provider-owned setup; remove old material site |
| `84a58c8cf37ba0b1dc8a11fa97c888c268fabc018cc0cc0213e8ad689a5a14df` | `options: safe_cex_crypto::secrets::Sensitive<Options>,` | Opaque Options/provider-owned setup; remove old material site |
| `b385287b7903b95152ecb2025d782dbe45d7ac64448fef388e9ac6056188ed13` | `let options = safe_cex_crypto::secrets::Sensitive::new(options);` | Opaque Options/provider-owned setup; remove old material site |
| `a5513fe5626d492c98a243727f65f36bd57c45a31ebc970339af175ed582c18a` | `.zip(options.secret.as_ref())` | Opaque Options/provider-owned setup; remove old material site |
| `ca602ef40bddb50a7fece4b891f3aed8b5a6f47a2c12f657af7122e6cff5cfcf` | `.map(|(key, secret)| sign::Credentials {` | Opaque Options/provider-owned setup; remove old material site |
| `f5337bace3eb12b97fe8ec3b8ff28168506324b89eb83b624688404891673788` | `secret: secret.clone(),` | Opaque Options/provider-owned setup; remove old material site |
| `20861b679b7c10377687b03415921648e95232082ba70cf253fa5e92dd5cce5a` | `secret: read.secret.clone(),` | Opaque Options/provider-owned setup; remove old material site |
| `9b258d4209b1982036d0bf8c004a7a1eb3eccabf81a295ecdc197d522623498d` | `secret: Some("private-secret".into()),` | Trusted test setup/oracle |
| `0c34e33203af9f5eb6bf128257c8bdf5caaab04d5808e870fcb70cc22a59a7c6` | `let _: &safe_cex_crypto::secrets::Sensitive<Options> = &venue.options;` | Trusted test setup/oracle |


## Resumed full cutover: actual release metadata and frozen socket consumers

Fresh branch `feat/rust-phemex-signer-cutover` retains original `78fb392c0ed`.
Actual landing `89b6a91efc5` (including actual #1968 `87bf458ba903`) was plain-
merged in `cfb257820cc`. Exact reviewed #1970 `04b0078dd29` was plain-merged
in `c901b3b62f3`; its four engine source blobs have no local policy edits.
The socket owner confirmed the API alone does not activate native Phemex. The
overseer therefore explicitly authorized exact reviewed whole-adapter #1974
`85fa7f5f939edf602c12c0df50488b1f76e165c7`, plain-merged in `d1915456576`.
Our PR must land after actual #1970 and #1974. Only the cleared authentication
recipe hunk changes the adapter; lifecycle/codec/reducers and accepted-snapshot
custody remain its owner's bytes/logic. Current unsigned PublicTrades #1969 and
its pre-429 delivery guard remain incoming-exact; ih35js has no additional claim.

Actual #1968 exports immutable `SigningRelease::{receipt,time,raw_expiry_ms}`
and `Venue::on_signing_release(SigningTarget, SigningRelease)`. REST/WS recipes
always declare the exact rounded signed seconds multiplied by1000 as receipt
expiry; provider echo and recipe application reject missing/wrong records before
release. The no-Cx observation hook updates ONLY Pending::Trade's raw expiry
from `raw_expiry_ms`, never from protocol UTC or capped HttpRelease. Request
metadata begins absent and is set only at the exact final release; retries or
repreparation never publish an intermediate receipt. Unknown-write native repair
keeps the existing5s skew after that actual uncapped raw bound. Missing metadata
cannot authorize early repair; the recipe/engine rejects it before outbound.
The callback itself grants no refusal/send/repair authority.

Options become testnet/public mode plus opaque primary/secondary Identity only.
Registry gets the approved Phemex-only provider arm/feature edge and one separate
construction dispatchSigning row, keeping hosted/delegated admission disabled.
No crypto feature change. All legacy tests move raw input into trusted test
setup; fixed raw-signing oracles live under tests/support rather than production.
No old secret holder or local-signing fallback is retained. Existing financial,
query, header, stream, routing and native optional-auth assertions stay intact.
Fixture changes in #1974 files are trusted-setup adapters only; account_open's
replacement is `send_signed` on the existing accepted epoch. The supervisor owns
all auth acceptance, queued one-shots and AOP1202 then ping1201 ordering.

Additional original controls use only the landed release API on unchanged
stacked native production: delayed/calibrated write release metadata, uncapped
expiry beyond the20s lifecycle, clock-revision reprepare, and no observation on
refusal. Exact provider refusal must survive into initial unsent-write failure,
remove that optimistic order or restore Canceling only if the native revision
still matches. A later refused leg retains accepted native IDs and cannot tell
the whole job NotSent. Keep existing native aggregation semantics, no HTTP status
classifier/retry rewrite; review the exact signer-only adaptation with4yjlw0.
Phemex has no separate protection-rollback error accumulator to invent.

Original WS tests' former immediate opening ping expectation is deliberately
superseded by the overseer's accepted-login ruling: before provider completion,
no auth/private send; after completion, auth999 only; after accepted ACK,
AOP1202 followed by retained ping1201. Preserve the original assertions in git
history and strengthen the active controls to this explicit approved contract;
never describe that contract update as a previously native behavior.

Before source, commit these refreshed gates and native release/aggregation REDs,
then independent source/caller/full-map and HMAC-owner review. The exact funnel
staging fingerprint set and before/after counts are frozen from actual source
with the approved checker row, not guessed. Any changed native trading issuance
fingerprint is the same physical site and needs an explicit owner-approved exact
re-expression map. The 14 old material sites are removed; new provider descriptor
fingerprints are separately reviewed, not raw-crypto exceptions. Recompute both
inventories against this actual+authorized stack, preserving all foreign rows.

HMAC owner refresh at `2c91` is direction-CLEAR with an additional required max
control: retain the maximum uncapped raw expiry across every actually released
attempt/leg of the same write job, not the last observation. Keep the per-request
released HTTP id and native active revision bound to its exact Pending::Trade;
a superseded job/revision cannot update the live job. Released-only job max is
preserved across sequential legs, while refused/stale/unreleased work contributes
nothing. Unknown-write repair is explicitly `max(raw_expiry_ms) + 5000`, never a
capped lifecycle bound. Pin two actual released legs where a negative calibration
makes the later leg's raw expiry earlier; that later observation must not shorten
the existing replay window. Native revisions/engine request custody provide this
metadata binding; no new venue execution owner or clock projection is introduced.
Current actual+authorized-stack request inventory is32F/35O (account3/3,
history5/8, identity3/3, income2/2, lib15/15, trading4/4); signing is14F/14O in
lib.rs. Independent refreshed-plan source/caller/full-map reviews CLEAR before
additional existing-API REDs. Production source is still unchanged.

Resumed test-only checkpoint `17b9ee461cb` executes22native tests:6GREEN and
16 intended assertion REDs (10 original provider-bypass REDs plus6 new release/
aggregation REDs),0ignored, on unchanged stacked production. Strict target
Clippy passes. New MAX/calibrated suffixes forbid early active-list reads,
exercise a genuine early host timer/rearm and require actual repair at the
maximum raw bound plus5000. They remain honestly unexecuted behind initial
provider bypass/missing release assertions until implementation. The one
set_signer-after-startup fixture error was corrected and excluded from proof.
Probe delegates every relevant native guard/socket/release hook. Source and
caller rechecks CLEAR after adding native guard delegation and early-repair
negative controls. Actual `6c7f4fcf292` was then genuinely plain-merged in
`12f16c0e85c`, preserving tests/Phemex source and automatic core composition;
the same22test run again produces6GREEN/16 intendedRED/0ignored.

## Full-cutover implementation and combined inventory boundary

The resumed candidate consumes actual #1968 and the exact reviewed #1970/#1974
stack authorized above. Native Options contain opaque Identity refs only; trusted
registry/SDK setup owns raw material. Every native credentialed REST funnel and
user.auth uses shared provider work. The sole account.rs delta is the Opened auth
recipe call; incoming socket codec, reducers and accepted-login custody remain
unchanged. Remote/hosted admission remains disabled. This is full native custody
cutover, not a RequestContext adoption of the remaining native callers.

HTTP/WS receipts use `stamp_ms: None`, with expiry equal to the rounded seconds
actually signed and sent times1000. The shared SignReceipt contract explicitly
allows expiry-only metadata (engine/signing.rs:331-357). Dispatch captures
prepared_revision separately (signing_dispatch.rs:419), rechecks it on completion
and final release, and checks receipt expiry from expiry_ms plus the prepared
protocol/raw clocks (:78-82); no timestamp is synthesized from a missing stamp.
Real HTTP/WS controls and the clock-revision reprepare control exercise this.
Trading records only engine SigningRelease.raw_expiry_ms on the exact pending
request/native active revision, keeps the released-only maximum per job, and
uses that maximum plus5000 for uncertain-write repair. Refused first writes keep
typed NotSent; accepted earlier IDs/effects prevent whole-job NotSent and retain
partial IDs. Native optimistic Canceling rollback retains its revision guard.

The original22 controls pass after two test-host completion fixes: missing
provider sends the real Signed(Unavailable) event instead of expecting the engine
to invent a host reply; calibrated repair tests unpark the provider only after
the manually completed write, allowing the later repair read to sign. No original
assertion was removed. Two further controls reject malformed Present/MAC/receipt
results on HTTP and WS before any native packet, covering six corruptions each.
The former native HMAC source is byte-exact in tests/support/native_sign.rs;
existing native tests use trusted LocalSigner fixture construction with all
wire/financial assertions preserved. Provider setup has an additional opaque-only
security control and SDK constructor-channel control.

Inventory boundary: actual6c7f4fcf292 before #1974 has Phemex lib17F/18O;
frozen #1974 removes 8e74b914... once and f9764ec2... twice, leaving15F/15O.
Cutover adds only one 6cf35cca... sign_http, reaching16F/16O. A standalone15->16
bootstrap aliases its own later single-site prune and is forbidden by the existing
negative test. Overseer therefore requires this cutover in the SAME certified
combo as #1974, immediately after it: #1970 -> #1974 -> full signer. The exact
STAGING_BOOTSTRAPS transition is actual17/18 -> combined16/16, never a landing
15->16 transition. Existing checker predicates and pruning controls remain exact;
checker owner claude-m26ps4 reviews the combined row. The current stacked-parent
15-map remains a known checker refusal until the composed candidate/receipt head
contains the final16-map. This is not a green checker claim for that parent.

The exact trading issuance re-expression is4F/4O: ce2317fc... -> e6f9b79a...,
replacing eager Credentials::expiry with initially absent release metadata. Both
whole-map rows and matching negative-control entries have explicit claude-ddi2il
approval, and are remove-before-master obligations. Signing inventory removes all
14 old lib material fingerprints; its only new entries are provider descriptor
import079483f8... once and hmac_sha256_work descriptor b94b333b... twice. No raw
cryptographic implementation or material holder remains in venue source.

Executed candidate proof: original256 native tests pass; the extended24 signer
and2 security tests pass; registry3 controls pass with the phemex feature alone;
strict Phemex all-target/all-feature Clippy passes. SDK constructor suites5 pass.
Independent stable-source/adversarial and caller reviews are CLEAR; full-map and
HMAC source reviews, fresh final checks/CI and actual #1970/#1974 remain gates.

Final shipping ruling: checker owner confirmed sequential PR landings would still
expose15->16 through origin/event-base refs, even inside a named combo. Overseer
therefore selected disposition(a): fold this reviewed full cutover INTO #1974's
own PR. codex-5c57jl merges this handoff head, cross-reviews the sole accountOpened
signing hunk, runs fresh CI and refreezes #1974 for combo-5b after #1970. There is
one actual17/18 ->16/16 boundary; no separate signer PR/landing, new checker
predicate or relaxed pruning control. The two whole-map rows remain exact.
