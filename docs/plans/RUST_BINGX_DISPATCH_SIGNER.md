# BingX native dispatch signer checkpoint

Plan, 2026-10-22. Fresh actual base
`5f6488c7664b68cf8b866d2f7f914a3fc99e5fc6`. #1718 inventory is retained in
`RUST_SIGNER_HMAC_MIGRATIONS.md` on its separate plan branch. No OKX/Bitget or
unmerged half-2 source is inherited. Keep #1737/#1757 watches active.

WOO was checked first and deferred without edits: its write gaps are recorded at
staging, so parked-provider + fast-ACK physical pacing belongs to RequestContext.
BingX has no equivalent write-gap/expiry dependency and uses HMAC-SHA256 hex,
without passphrases. Native HTTP adoption can proceed on the landed signer API.

1. Replace raw Options and sign::Credentials with opaque primary/read Identity
   references. Trusted shared setup owns all material; registry/WASM BingX feature
   gates select dispatch signing without Binance. Incomplete primary retains
   public construction; malformed secondary remains an early rejection. Keep
   native user/account IDs, no key-derived identity or new stable-ID fallback.
2. Keep Params/BTreeMap native typed serialization: canonical RFC3986 sorted
   query including shared integer protocol timestamp, HMAC-SHA256 hex signature;
   final query re-sorts signature normally. POST sends exact typed JSON (numbers,
   booleans and u64 IDs stay native), other verbs query/no body. X-SOURCE-KEY,
   X-BX-APIKEY and Content-Type header order remain exact. Public quote work stays
   unsigned. Listen-key HTTP is atomic Present-only, no timestamp/signature.
3. Real issue/resend and independent fetch_identity builders stage SignStep with
   immutable Params recipe and selected refs. Ordinary GET selects secondary;
   writes/listen POST/PUT select primary. Preserve account read scope/slot tags,
   streamed ownership, primary/secondary egress and existing signed wrapper.
   Same-callback reprepare uses protocol time; raw timers/deadlines remain raw.
4. Preserve one eligible100001 signature resend (including streamed rejection).
   Provider failures do not invoke it. Typed refusal unwraps the existing pending
   owner: identity/history strict failure; trade first unsent notSent, prior
   resent attempt Unknown, existing accepted effects retained. Read-owned
   metadata failure cannot regress newer rails or become native auth rejection.
   Background account cleanup retains existing generation/recovery checks; no
   new retry, outcome ledger, calibration or scheduler.
5. Existing listen-key acquisition/renewal and socket subscription/reconnect
   lifecycle stay unchanged. No static WS HMAC exists and no new token custody
   path is added; the owner WS-auth core owns later session standardization.
   Present-only work uses the shared provider; no venue key read or raw restoration.
6. Preserve old exact vectors as cfg(test) oracles, migrate native fixtures to
   trusted provider setup, and add actual-builder refusal/late/delayed/reprepare,
   POST type/body/client-ID, slot/primary listen presentation, metadata and resend
   controls. Include runtime RED owner-routing regressions and preserve all
   financial/history/socket assertions. Shrink exact ratchet and scan sign modules;
   pure work descriptors/test oracles do not authorize runtime custody/crypto.
7. Include same-PR caller and BingX CLAUDE notes, worker/provider bootstrap and
   draft exposure boundaries. This native checkpoint does not claim hosted
   signHttpRequest/keyless admission; standalone request shaping/policy/actual-WASM
   positives follow actual half-2. No encoding/passphrase extension is required.
8. Focused BingX+Binance/engine/registry native gates, strict fmt/Clippy, SDK setup
   type/lint checks, independent correctness/adversarial audits. Final actual
   dependency integration/full CI/authenticated replay and own-only guard precede
   merge. No local release builds/full replay/size/timing runs.

Architecture gate: existing engine SignStep/provider/setup adoption. No new
per-venue cross-cutting concern; WOO pacing was reported rather than patched.
Settings/store freshness, row admission, financial execution and metadata owner
work remain intact and coordinated with their lanes.

## Native recipe checkpoint

The additive `sign::dispatch::Http` recipe stages opaque provider refs and uses
the existing typed renderer. Production builders remain on their old path until
the next checkpoint; this commit does not claim custody migration or readiness.
The five dispatch tests cover native method/body parity, Present-only listen
requests, changed-intent rejection before provider work, malformed provider
results, and stale-result reprepare with old-attempt suppression. BingX/Binance
native gates and strict checks pass. Independent correctness and adversarial
source reviews found no blocker; the latter's missing negative controls were
added before committing.

Owner approved the existing `job.effective/error/ids` migration adapter: a prior
sent leg must never become wholly NotSent; uncertainty stays Unknown. Add RED
cases without new state flags or ledgers, and identify the BingX finalizer as a
RequestContext consolidation target in the PR body. Account signer cleanup must
compare the existing in-flight revision before clearing a latch. The separate
unmerged metadata core is not inherited, and its publication fences stay intact.

## Native staging checkpoint (draft)

Real issue/resend and independent identity builders now stage the shared recipe;
venue Options/Credentials contain opaque refs only. Registry and standalone WASM
feature gates select trusted setup. Existing native tests use provider setup,
with the old full signing implementation retained exclusively as a test oracle.
All BingX native tests and the BingX-only registry build pass; strict gates cover
the touched crate and registry. Independent reviews preserve success bytes,
secondary GET/primary writes, streamed ownership, resend tags and existing
metadata fences. Review caught and restored the valid-primary fixture in the
invalid-secondary/no-fallback test.

This is an intermediate checkpoint, not PR readiness: typed owner failure routing,
existing-state conservative outcome RED controls, revision-fenced cleanup, caller
section, exact ratchet shrink and hosted/CI proof remain. Work pauses here for
the owner's #1757 final integration priority after actual #1737.

## Typed owner-failure checkpoint plan

Resumed from `f0849701476`, genuinely integrated actual #1762 `2cf8c4c91dd`
at `c5472306109`. Owner approved the construction JSON conflict: retain incoming
Binance/Bybit policies verbatim, add only BingX dispatchSigning. The existing
registry build generator regenerates its table; construction-codegen checks and
independent full-tree preservation review pass. No hosted admission is enabled.

1. Add original runtime RED controls through the actual BingX engine and provider
   failure events: identity, history and metadata retain typed kind/outcome/code;
   first unsent trade is NotSent; already resent write is Unknown; mixed successful,
   refused and unsent legs preserve existing accepted IDs and conservative evidence.
   Add account owner controls proving old revisions/epochs cannot clear newer
   read reservations or retire replacement sessions. No raw signer values appear.
2. Route `Venue::on_sign_error` by the existing Pending owner. Trading uses its
   existing job/error/effective/ids state and finalizer; history removes its owned
   read then rejects strictly; metadata propagates the original typed error.
   Account background cleanup compares the captured request revision against its
   existing in-flight token (not the current financial revision), retaining native
   backoff and listen-key epoch checks. Position mode keeps its sole-owner bool.
   Provider refusal never becomes venue auth evidence or a signature retry.
3. Keep prior-attempt uncertainty through existing Signed.resent; never call an
   operation wholly NotSent after a write reached the venue. Keep the current
   partial-ID contract, with the BingX finalizer explicitly a RequestContext
   consolidation target. Do not add a ledger, evidence flags, retry policy or core.
4. Tighten the signing ratchet to the actual scan; add the caller section in the
   same PR. Audit TS hosted canonical bytes, labels and public sign methods.
   Hosted admission is allowed only if existing hex/API-key presentation suffices
   and actual WASM/SDK positives cover every supported public/remote API. Any
   missing shared presentation/session contract blocks that part for owner review.
5. Verify BingX plus Binance, registry generation/checks, strict fmt/Clippy and
   focused SDK type/lint/tests; independent correctness/adversarial source reviews
   before committing. Keep #1763 watchers active. Full CI and authenticated replay
   proof/guard remain required before any merge; this is a native checkpoint.

Architecture gate: pending-owner adapters consume the shared signer failure,
using existing recovery/outcome state only. Shared outcome consolidation belongs
to RequestContext, not a new BingX implementation.

Checkpoint evidence: the unchanged resumed implementation reproduced five typed
owner failures (identity, history, metadata, first unsent write and signed retry)
and a stale-reservation cleanup failure at runtime; compatibility controls passed.
The owner adapters correct those controls without changing native success bytes.
The migration ban now scans BingX sign modules, and its exact allowance shrinks
517 to 508, equal to the scan. The two retained HMAC entries are work-builder
descriptors with opaque refs and no secret access; existing test-only crypto is
outside production venue source. All non-BingX allowance values are unchanged.
Caller scope is recorded in RUST_CUTOVER_CALLERS.md; hosted admission remains off.

Final focused validation: all BingX/Binance native targets passed (459 tests before
the added late controls); the final BingX owner suites pass 27 tests, including
12 actual-engine provider cases and four account-owner groups. The late controls
cover both cancellation orders with unclassified HTTP responses, buffered later
history-page refusal, exact retry work/progress, released settings reservations,
and a native WS settings update before typed margin failure. The shared dispatch
queue remains ordered; no test requires a newer HTTP write to bypass a parked
older request. Strict Clippy, fmt, signing bans and negative scan tests pass.
Default-feature registry/generation/provider suites pass 51 tests; the standalone
BingX registry build passes. Five focused SDK suites pass 105 tests, plus package
typecheck and lint. Correctness and adversarial reviews cover the adapter and
late controls; CI, full replay proof and hosted acceptance remain pending.

## Post-RequestContext integration: managed admission required

Actual #1776 `35ba549959cfb6e00fe7bccf2fb06826fc1b10f7` is genuinely
integrated. Two independent reviews preserve all incoming content, all 27
nonoverlapping source/test/config blobs and both appended caller/index blocks
verbatim once. The checkpoint passes the BingX native targets, but is not ready
for cutover: the new RequestContext inventory rejects its legacy staging delta.

Owner disposition is no growth. Move source-local request-driver tests into
integration tests, consume the separate shared SignStep unsigned-header
projection prerequisite, then put private request builders on
`request_signed_managed` with explicit semantic Context and the existing opaque
SignStep. The same static public-header data must serve legacy rendering and
managed unsigned intent; credentials and signatures remain provider/apply-owned.
Do not weaken unsigned attestation, exempt legacy staging, manufacture a public
Context for a private read or duplicate release/pacing/outcome ownership.

Preserve original typed-failure and prior-sent uncertainty regressions while
adding real managed release/provider tests. The legacy request inventory must
shrink, with every unrelated entry unchanged. Hosted admission stays disabled.
Metadata #1747 consumes the actual family recipe/context seam after this port;
WEEX is the next family ahead of the remaining HMAC queue.

## Managed rebuild audit and session disposition

Read-only actual-API audit confirms ordinary financial requests can use the
shared core after actual #1780. Preserve the native selected identity even for
quote REST: Markets/Daily/Premium/Candles sign secondary GET when credentials
exist; public history Trades explicitly stays unsigned. Pending labels alone
must not determine credential authority. Identity has a separate builder and
must migrate too. Stream shapes and every existing account/metadata revision,
settings generation and publication fence remain native codec constraints.

The owner explicitly permits exactly one temporary `sign_http` staging site for
listen-key POST/keepalive PUT only, using primary opaque API-key Present (no
HMAC/timestamp). Keep the existing two HTTP transport sites for that session
boundary; do not generalize this adapter to financial or quote signing. Record
the exact final fingerprint in the PR body and independently verify that the
whole legacy inventory still shrinks. Remove that allowance when uufub4's shared
background-session Context lands. Sessions are not financial Reads or invented
commands; the shared extension owns their eventual retry/expiry/token boundary.
Move actual-engine test drivers out of production source, preserving assertions.

Before production wiring, map each logical command/query and physical leg's
Context lifetime explicitly. Account snapshots have background Read authority;
metadata's two reads retain command generation, history owns pages/windows, and
trading retains command-wide evidence with bounded child legs. A multi-page
query must not silently inherit a first-page 20s budget, and an issued leg must
never gain a fresh deadline on signature retry. Mint/retire page authority at
the actual bounded page boundary and preserve any enclosing command bound.
Bring an unrepresentable native lifetime to the owner rather than fabricate it.

Managed mutation release requires `capture_http_reconciliation`; use bounded
nonsecret correlation or the landed reference's bounded empty capture/no-op late
codec where ordinary fenced reconciliation owns healing. Never bypass default
capture refusal. Record each validated native ACK/refusal with `http_write_result`
before cancellation/settings patches, native job advance, sibling issue or
Context retirement. Separate validation from the current parsing closure's
mutations enough to preserve that ordering. Shared RequestContext owns aggregate
outcomes; native planning and accepted-ID correlation do not become a new ledger.
No late ACK may resurrect a command or acquire financial authority from advisory
fields. Actual release, empty-ID effects, partial replace and stale/refused
recording controls are mandatory.

One native signature retry remains eligible only for a definitive native refusal.
The old broad message-text or code-100001 matcher cannot by itself prove a
financial write was rejected: gateway 408/5xx and unclassified/malformed ACKs
remain Unknown and must not authorize another mutation. Record the exact rejected
attempt before `http_retry_context(DefinitiveRefusal)`, preserve immutable intent,
client IDs, slot, stream shape and the original raw leg deadline. Provider/route
refusal is not venue auth evidence and never enters native signature retry.

Required regressions cover actual managed route/sign/release, original-deadline
expiry, definitive refusal versus text/gateway unknown, released-first sibling
failure in both orders, empty-ID settings/cancel effects, capture refusal versus
functional release, strict later-page history/provider refusal, stale account
reservation/session cleanup, both metadata-stage failures, and the temporary
session-only fingerprint. Hosted admission remains disabled. Architecture gate:
codec/owner adapters consume shared provider/lifecycle/evidence APIs only.

### First managed builder slice

Exact reviewed #1780 `ea9924f9523` is preintegrated at `7f62bd5b603` under
owner authorization; actual #1780 remains a merge prerequisite. Start with
credentialed startup quotes, independent identity and background account snapshot
reads: each has one engine-issued 20s Read Context, selected credential routing,
original-deadline one-signature retry and retained native pending owner. Signed
holds that Context through preparation, response/stream/refusal and terminal
cleanup; it does not mint authority on retry. Native account revision and auth
classification remain unchanged. Metadata's paired root and command writes are
the following slice, not accidentally swept into this single-owner conversion.

The owner also extended the same single temporary provider stage to history:
exactly one physical `sign_http` site, still overall inventory shrink. History
remains explicitly legacy-owned until the shared Completion bridge and read-only
validator are actual; no invented root bound or numeric-command continuation.
Keep session/history conversion targets in the body and remove the temporary
stage when both shared boundaries are adopted. Hosted admission remains off.

First-slice implementation checkpoint: original `4e63f492b85` reproduces two
actual-builder preparation assertions. The managed builders correct both; six
route/sign/bind/claim controls also cover exact context completion, native
rejection, provider and route refusal, duplicate result and original deadline.
Existing account/auth/identity assertions remain intact through the explicit
managed fixture handoff. Identity's original no-retry path is declared Never and
is pinned with a native100001 response; it does not inherit snapshot retry policy.
Both source reviews are CLEAR, including identity context completion and fixture
preservation. Strict Clippy uses boxed error Pending and grouped method/path
arguments, with no lint exemptions. Full inventory closure, candles, metadata,
write evidence/aggregation, same-PR final caller rewrite and CI/proof remain open.

Validation at this intermediate slice: 490 BingX/Binance all-target tests pass;
the late identity-no-retry preservation correction repeats all 204 BingX tests.
Strict all-target engine/BingX/Binance Clippy and final BingX Clippy/fmt pass.
The legacy inventory remains an explicit full-port gate; this checkpoint neither
updates allowances nor declares the remaining legacy builders ready for merge.

### Managed candle command slice

Preintegrate the refreshed reviewed #1780 head `a869e95dbc7` (actual #1775 and
#1781 included); actual #1780 remains mandatory before this family merges.
Next move credentialed FetchCandles through the existing managed Read adapter,
with one command-scoped 20s Context, secondary GET selection and original-deadline
retry. Public unsigned candles retain the native path. Preserve the existing
query spelling, stream path, row validation, sorting and seconds conversion.

The outer Signed pending owns the Context. Pass that exact authority to candle
completion and failure; never complete a managed command by numeric ID or mint a
replacement root at a response. Share the native candle result codec between
legacy and managed completion instead of copying sort/validation logic. Keep
provider error kind/outcome and native error text. Streaming errors and route/
provider/timeout cleanup must settle the original Context once, with duplicate
callbacks inert. Original-RED controls must show the actual credentialed command
fails to publish managed preparation, then verify route/sign/bind/claim and
success/native refusal/malformed row/provider refusal/original-deadline expiry.
No metadata, history, session or write authority changes enter this slice.


Candle implementation verification retains the original actual-builder
preparation assertion RED. A second reviewed regression reproduces buffered rows
from a refused100001 response escaping into its successful retry (and malformed
rows poisoning that retry). The adapter now clears only candle rows/valid state
when starting that native read retry, retaining the engine Context, request
intent and original deadline. Thirteen managed read controls cover both buffer
variants, HTTP/native errors, missing/null/truncated lists, row decode, exact
completion, typed route/provider refusal, duplicate callbacks and a retry parked
until the original20s boundary. Metadata, writes and full inventory closure
remain separate unfinished implementation phases.

### Actual unsigned-header integration and paired metadata API audit

Actual #1780 `02340e17712780fa526f4e07a029cd4e20fb00f5` is genuinely
integrated at `812c9d0654a`. Both preservation reviews retain all 33 non-CALLERS
owned blobs, the complete 5,281 unowned incoming entries and all shared engine
header blobs. Caller content is the entire incoming document plus the own BingX
append once. Construction adds only the authorized BingX dispatch flag. All 497
BingX/Binance native targets, strict Clippy and fmt pass. Hosted admission stays
off; this integration is not full-family readiness.

The next metadata slice is the existing serial leverage/margin read, with its
existing settings revision, symbol generation and pending-write publication
fences unchanged. The planned adapter carries one engine-issued command Read
root through both stages, and a separate 20s child per stage. The root bound is
40s (two original 20s logical-leg bounds); a native signature retry retains its
child's original deadline. No continuation mints a new command root. The root
stays in the existing metadata pending record; the Signed wrapper owns the
currently delivered child. Terminal success/failure uses root `done_context` or
`fail_context`; numeric completion remains only before authority is created.
Each child retires only after its logical stage and all attempts complete.

API review with the shared owner found a true prerequisite. Actual #1783's
parent API admits and fences a delivered child, but exposes no external
prepublication read validator for the pair. The shared follow-on will provide
`validate_read_http_delivery(root, leg)`: exact delivered-leg identity, live
issued Read, matching original root owner/generation, and read-only completion
authority before Store mutation. uufub4 owns this extension. Production metadata
wiring waits for its actual pin or an explicitly authorized frozen stack; no
venue validator, numeric fallback or unmerged API assumption is permitted.
Original-RED controls can establish the actual builder's legacy transport while
that shared API is completed.

After the prerequisite, retain the existing account metadata codec, success
bytes, current-rails fallback and all write/WS/read freshness fences. Validate
root/child before any Store setting mutation or companion completion. Pass the
same root into stage two and preserve each child's credential slot and immutable
native query on retry. Strict failures in either stage, provider/route refusal,
original deadlines, duplicate/late callbacks, stale settings and newer-query
supersession need actual managed route/sign/bind/claim controls. Adapt existing
fixtures without weakening their wire/state assertions. Existing history/session
legacy ownership and its single temporary provider stage remain separate.

Write adoption is still a subsequent audit/implementation slice. Consume the
landed command journal and shared execution-group authority; do not reproduce a
multi-leg ledger, outcome aggregator or bounded group lifecycle in BingX.

### Authorized exact read-validator stack

The owner permits an exact-head stack on reviewed #1792
71b2681bc6d286d972cd286d45a1ca99b1be3f0e for the paired-read adapter.
Its matching actual landing remains mandatory before this family merges. This
also genuinely integrates actual1783 and the final history chain. Private
financial writes and joined observer groups remain gated on the owner's shared
plan/reservation API; no authority is inferred from these read validators.
Implement only the existing metadata leverage/margin pair described above:
retain one40s original Read root in its native pending request, derive each20s
child through http_leg_context, retain original child deadline on signature
retry, validate_read_http_delivery(root,leg) before decoding/publication or
companion completion, and use done_context/fail_context on the root. Existing
native publication fences and current-settings fallback remain unchanged.
Shared finish handles only child retirement after that logical stage, never
renewing root authority. Preserve both actual-builder originalRED controls and
add managed release/refusal/expiry/duplicate/supersession controls; keep session,
history, writes and hosted admission explicitly at their existing boundaries.

### Actual read authority and safe IDs; corrected native retry lifetime

The checkpoint now genuinely integrates actual read-authority #1792
`8e53b1bd39e0fd03534ffa4453349390251e4585` and actual SafeIDs #1789
`7641650e7b1fc54c55593880ac16299171f60a90` at `de02811a094`.
The complete shared HTTP/header/validator/allocator source and all 5,291 unowned
incoming entries are preserved. The only conflict was independently appended
CALLERS prose; full incoming text precedes the prior own suffix verbatim once.
Both preservation reviews are CLEAR. The existing completed read slices pass
504 BingX/Binance native controls; the two paired metadata builder assertions
remain original REDs, not a claimed production adoption.

The earlier generic 40s-root / retry-retains-child-deadline proposal is superseded
for the actual native fresh-retry contract. A metadata read that permits one
signature retry has two physical attempts, each with its native fresh 20s window:
one bounded read-call therefore declares 40s, and a serial two-read execution
with that retry allowance for each read declares 80s. A 40s execution describes
only a no-retry pair or a different shared-call budget explicitly supported by
the native contract. Do not clip valid retries to a guessed root deadline.

Use the shared owner's reviewed Preflight/CallWindow execution declaration once
available: one original read execution owner, checked total declared bound, and
separate immutable physical attempt caps. A reprepare cannot renew its current
physical attempt; only an allowed native signature retry may start the next
fresh native attempt inside the originally declared execution. No venue-local
root, timer, numeric-owner fallback, cloned parallel send authority or arbitrary
120s cap is permitted. The read-delivery validator is actual, but by itself does
not implement that retry-lifetime owner. Paired production wiring waits for the
complete shared ownership seam; the original REDs stay intact until then.

Retain the same root across both logical reads. Before any metadata decode,
Store publication or companion completion, validate the exact root/leg delivery
with `validate_read_http_delivery`; keep all existing settings revision,
symbol-generation, pending-write and current-settings fallback fences. Failure
cleanup still settles the owned root exactly once even when no delivered-leg
validator can succeed (provider/route refusal or expiry); invalid callbacks gain
no publication or numeric completion authority. Retire a child only after that
logical read and every declared attempt have finished.

For the later shared scheduler adoption, the response lease covers the declared
read-call chain (40s for two native 20s attempts), while the release/lifecycle
physical cap stays 20s and bounds each lease handoff. Admission remains
idempotent; a signature refusal is not DATA and never progresses or renews the
scheduler lease. Only validated first-read DATA may perform the existing
scheduler progress before the second logical read. No unscheduled fallback or
new Scheduler API is introduced by this mapping. Add actual-path RED/GREEN
controls for the late first response, fresh signature retry, second read,
80s total boundary, refusal/expiry, duplicates and stale publication fences.

Financial write groups remain a separate shared Plan/reservation prerequisite:
serial eight-delete native walks can validly take 152s, and the declared
fresh-signature-retry worst case is 320s. The same physical journal/root owner
must handle writes and observer groups; no BingX-local ledger or timer owner is
created. History and session ownership retain their previously documented
explicit legacy boundaries until their own shared completion/session seams.

## Lane takeover status (claude-4yjlw0, 2026-10-23)

Checkpoint `c9fa7dfa879` confirmed: 24 commits ahead and 19 behind landing
`73d04380dd9`. #1779 head `f2136f8154c` is historical. Single-owner reads and
candles are managed. The paired leverage/margin metadata reads stay parked as
two original REDs on uufub4's reviewed Plan/CallWindow pin, which declares 80s
execution, 40s read-call and 20s physical attempt (uufub4 confirmed
2026-10-23). Writes stay parked on the shared execution-group API, now owned by
claude-fe7lgm. One temporary `sign_http` staging site remains for history and
the listen-key POST/PUT.

Next:
1. Merge pinned landing `73d04380dd9`, which includes #1808. Run focused
   BingX/Binance tests, then push to refresh #1779 and CI.
2. Reverify the #1814 rows (missing 100412, signature-text resend) through
   shared tables.
3. Wire paired metadata once uufub4 sends the reviewed pin. Wire writes once
   fe7lgm's execution-group API is actual. No local root, timer or ledger.
4. Remove the staging site once cxxyqr's history Completion bridge and
   55ao7t's session Context are both actual.

## Remote signing admission (claude-dazzy3, 2026-10-29)

BingX now admits `remoteSigner` on Rust, so the three modes work: an exchange-server
account with local keys, a keyless exchange-server account whose browser signs, and a
browser wasm account posting directly.

- **Construction row:** `"remoteSigning": {"enabled": true, "payloadSupported": false,
  "protectedHeaders": ["X-BX-APIKEY"]}`. The signature is the default hex HMAC-SHA256
  in the query or body, so no `wire` is needed.
  - `payloadSupported` stays false because the crate has no REST-only private feed.
    A `payloadSigner` account is a typed refusal, as for Bitget, OKX and Bitunix.
- **Keyless engine, browser signs:** the dispatch recipe is already plain HMAC work
  (`Present` key, `Sign hmacSha256 Bytes(query)`). The shared remote signer provider
  therefore sends one `ws-auth` hex request per signed request, and the listen key is
  only presented, never signed.
- **Keyed browser engine answers:**
  - The registry `hosted_signing` family gains `bingx`, and the registry and wasm
    `bingx` features gain `hosted-signing`.
  - `legacySigning.ts` gains a plain hex codec (`exchanges/bingx/bingx.signing.ts`)
    with the legacy client's `localeCompare`-sorted repeat-format preimage, for
    `type: 'http'` requests. The livenet listen key stays unsigned.
- **Pinned by:**
  - `registry/tests/bingx_hosted_signing.rs` checks against a Node crypto golden.
  - `rust.bingx-hosted-signing.test.ts` checks that the codec and the legacy
    interceptor send the same bytes for place, cancel and amend.
  - `rust.bingx-hosted-provider.wasm.test.ts` checks that a keyless remote-signed
    account sends the local-key bytes for place, amend, cancel and the listen key,
    with Node HMAC over the legacy preimage.
