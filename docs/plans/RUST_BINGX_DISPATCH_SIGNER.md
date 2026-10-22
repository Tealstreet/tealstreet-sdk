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
