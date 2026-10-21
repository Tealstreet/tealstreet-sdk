# Rust account authentication results — Phase 6b plan

Status: **The core/host/SDK foundation is implemented and reviewed; venue
classifiers and end-to-end native credential proof are still pending.** Phase 6a's scheduler and SDK option propagation
are separate work. Calling Base auth methods manually does not prove native
credential backoff. Phase 4 getAccount inventory and its future implementation
batches also remain separate; this plan implements neither getAccount nor
network resync.

This audit covers all 26 native venue crates, including their spot/other product
variants. It describes existing source evidence, the minimum additive bridge,
and verification required before claiming actual credential backoff works.
Existing business errors, retry budgets, deactivation, signing, credentials,
payloads, transport routes and subscription behavior retain their semantics.

## Required behavior

A definitive rejection of the credential used by a private account read must
increase that credential's polling backoff. A later accepted private account
read may clear that same credential's streak. Public data, unsigned wallet
lookups, another credential, transport failure, malformed envelopes or missing
required envelope data, rate limiting, and unrelated business rejection must not
clear or manufacture auth evidence.

A parseable signed success envelope with its required envelope-level fields
proves that the credentials work. Individual row parse/mapping failures,
uncatalogued markets and held rows do not block Accepted and never produce
Rejected. They remain data problems handled by the existing row diagnostics and
financial reducers. A valid empty list is accepted evidence. Validate required
list shape before grouped auth state changes, so a malformed success cannot
clear an endpoint rejection before a sibling succeeds.

Use explicit native classifications of typed response fields/status where the
venue contract establishes their meaning. Never derive authentication results
from logs, formatted error strings, body substring/regex matches, header names,
proxy authorization, or a generic HTTP-success bit. Existing retry predicates
that use older parsing retain their behavior; they are not an auth classifier.
An unknown classification emits no result. Do not substitute a universal auth
code allowlist: ApeX's existing trading fixture uses code 10003 for insufficient
funds, while Bybit uses 10003 for invalid API keys.

The bridge is account/session scoped and carries no key, secret, JWT, cookie,
account name, raw payload or credential fingerprint. Primary and Secondary are
configured signer slots, not inferred account identities. The same slot means
the same construction-time signer within an engine generation; generation
replacement must fence late evidence.

## Implementation refinement before the foundation

Keep the selected signer and read scope on the engine's existing in-flight
request entry. `Cx::tag_account_read(request, credential, scope)` is called at
dispatch, after signer selection; no credential/header inference or retained
result ledger. The optional pure venue hook is
`account_auth_outcome(&self, pending, response, now) -> Option<AuthOutcome>`.
Core emits the captured context plus verdict once before the existing callback,
with Accepted suppressed for a missing expected streamed list. Transport and
signature failures remove the same entry without credential evidence.

Scopes are Account, Balance, Orders, Positions and Settings. Track bounded
streaks per credential AND scope in the SDK, and reflect their maximum through
a protected Base streak setter. A successful balance read must not clear an
orders-permission rejection on the same key. A successful login clears only
Account; unrelated successful public/unsigned/other-credential reads never
reset failed scopes. Legacy manual Base auth calls retain Primary/Account
meaning. This is still a small fixed set of counters, not a permission system.

Foundation names: `CredentialSlot::{Primary,Secondary}`, `AccountReadScope`,
`AuthOutcome::{Accepted,Rejected}`, `Notice::AccountAuth {credential,scope,
outcome}` and WASM action `accountAuth`. Native server notices preserve the same
account-scoped context. JS host validates and calls `onAccountAuth`; the SDK
updates resolved polling policy without inventing user errors or interpreting
REST bodies. Existing terminal startup behavior is a separate queued review.

The original audit below describes the evidence and pitfalls; where it proposes
pending-field capture or a combined result hook, this refinement is the chosen
implementation seam. Venue classifiers/tags follow the core/host foundation.

## Additive native and host contract

Proposed names, subject to the core implementation's final naming:

- `CredentialSlot::{Primary, Secondary}`.
- `AuthOutcome::{Accepted, Rejected}`.
- `AccountAuthResult { credential: CredentialSlot, outcome: AuthOutcome }`.
- An additive `Notice::AccountAuth { ... }` and corresponding WASM action.
- A default no-op trait hook:
  `Venue::account_auth_result(&self, pending: &Self::Pending, response: &Response<'_>) -> Option<AccountAuthResult>`.

The native venue knows which typed pending jobs are credential-bearing private
account reads. It returns `None` for public requests, unsigned reads, trading,
ordinary histories/commands, socket traffic and other unrelated operations.
Authentication acquisition that gates account polling can report a definitive
credential result, but session expiry/retry and malformed login replies alone
are not proof that the underlying key is rejected.

Core emits the returned notice before the existing completion callback. No new
Store, retained auth ledger, HTTP payload interpretation in JavaScript, or
VenueError rewrite is required. Native host notice forwarding remains ordered;
the native account/session boundary must preserve account isolation. This is
not a command notice and must not be routed through command-pending ownership.

### One final-response observation point

`Engine::handle(HttpResponse)` already converges with streamed HTTP events on
`Engine::http_end` in `sdk-rs/engine/src/engine.rs`. Observe the pending request
and complete response once, immediately before the existing callback:

1. A finished streamed array with a validated UTF-8 envelope calls the hook,
   then the unchanged `on_response`.
2. A complete valid JSON envelope missing the expected streamed array calls the
   hook, then the unchanged `on_stream_error`. Allow a typed Rejected result;
   suppress Accepted centrally on this path. Missing required data must not
   manufacture recovery evidence.
3. A complete buffered UTF-8 response calls the hook, then the unchanged
   `on_response`.

Do not call it on chunks, HTTP transport failure, invalid UTF-8, truncated or
invalid streamed JSON, or a pending request already consumed. Venue classifiers
must also reject malformed/unknown ordinary envelopes. Do not globally forward
`on_stream_error` into `on_response`: some venues would otherwise accept a
missing list as an authoritative empty snapshot. Seven crates currently have
stream-error overrides; the optional core hook avoids adding equivalent
feedback overrides to the other 19. A stream override recursively invoking its
own `on_response` must not invoke the core hook again.

A response with its streamed array removed still preserves envelope metadata.
Classifiers use that typed metadata and envelope validity, rather than row
mapping flags; they must not require access to rows already streamed and consumed.

### Capture the actual signer at dispatch

`RequestRoute` currently means egress (Primary/Direct/PublicCors), not signing
identity. It is not sufficient for this purpose. Record the selected credential
slot at the same point the native signer selects its credentials, before I/O.
Carry that small slot in the relevant pending wrapper/request. Reuse existing
wrappers such as Bybit Rest, Aster SignedRequest and native queued requests;
keep private module helpers local rather than exposing secrets or all fields.
The core hook reads the captured slot, not current options or current degraded
state. Existing generation/session fences must apply before emitting evidence.

This matters concretely for Bitget: account GETs normally use the saved read
key, while `primary_balance_read` switches degraded balance/UTA-assets repair
to the primary signer. Degradation can change while a response is in flight.
Selecting its slot from current state would misattribute recovery.

### Retry admission is part of classification

A response that native code will re-sign, refresh-session-and-resend, or otherwise
retry must not prematurely report terminal credential rejection. Return None
on that intermediate attempt; classify the accepted or finally rejected result.
Use a pure predicate shared with the existing retry-admission branch where
necessary, so the hook and callback cannot disagree. Preserve actual current
retry decisions, including their existing legacy message fallback, without
using those messages to decide whether an error is an auth rejection.

Existing useful state includes:

- Bybit `Pending.rest.signed` / `rest.resent` and pending category; inspect the
  pending category rather than whichever category the venue currently selected.
- Aster `SignedRequest.public`, version, `signature_retried`,
  `timestamp_retried` and its existing retry budget.
- Bitrue `Pending.signature_resent` and attempts. Its missing-list path already
  recognizes a typed signature rejection before resending.
- ApeX `Pending::SignedAuth.legacy`, retries and nested pending job.
- Crypto.com account Request.retries for the one 40101 resend.
- GRVT `Pending.auth_resent` / `auth_generation` before session refresh.
- Kraken Spot private Request.signature_retries and network retry state.
- Other venue-specific read retries must be checked at implementation time;
  do not invent a resend simply to make a classifier uniform.

The hook is observational. It cannot enqueue a retry, change signing, mutate
account readiness, deactivate an engine, or change the existing final result.

## Audited native inventory

P = primary signer; S = configured secondary GET signer with P fallback.
An available typed error code is not automatically an audited auth allowlist.
Rows explicitly identifying a missing classifier require documented codes or
precise existing fixtures before enabling Rejected. No generic nonzero-code or
all-403 classification is acceptable.

| Crate / variants | Actual polling credential | Existing classification evidence and implementation seam |
| --- | --- | --- |
| Binance futures/PM/inverse/spot | S for signed account GETs | `src/lib.rs::signed_request`, `src/account.rs::venue_error`, `wire::ErrorBody.code: i64`. Capture signer before request; match typed account-read jobs. Listen-key POST/PUT, socket auth and signed public/startup metadata do not prove the polling read key. No shared native auth allowlist exists yet. |
| Aster v1/v3 | S for signed GETs, using the selected version/signer | `src/request.rs::dispatch_signed` / SignedRequest, `src/account.rs::venue_error`, typed ErrorBody numeric code. Account selection differs from write/listen-key credentials; preserve v1 signature and timestamp resends. Do not classify builder-attribution rejection as invalid credentials. |
| Bybit linear/inverse/spot/demo/Hyro/options | S for private GETs | `src/lib.rs` Status.ret_code and Rest retry wrapper. Existing TS auth set is 10003/10004/10005/10010/33004 (`bybit.api.ts`); first retryable signature rejection is intermediate. Signed public markets/tickers can succeed with a rejected key: exclude them explicitly. HTTP-only CDN/proxy failures have no reliable auth classification without venue evidence. |
| Bitget classic/UTA/inverse/USDC | S normally; P for degraded balance/UTA-assets repair | `src/account.rs::signed_headers`, `primary_balance_read`, account generation wrapper; `wire::Envelope<T>.code: Number`. Capture slot at dispatch, including mixed-credential cycles. Add an explicit venue auth allowlist; generic nonzero codes include business failures. |
| BloFin | S for account GETs | `src/account.rs` headers and response handler; `wire::Envelope.code`. Existing TS auth codes are exact strings 152401/152406. Replace body-contains auth counting with typed equality when implementing the signal; code 0 succeeds only on relevant authenticated account reads. |
| BingX | S for private account GETs | `sign::Params::into_request` selects read credentials; `lib.rs::success` already parses a required integer code. Extract/reuse the typed envelope without collapsing its code to a String error. Listen-key maintenance/socket success is unrelated. Auth allowlist still needs explicit evidence. |
| Backpack | S for account GETs | `src/account.rs::signed` selects credentials; REST account handlers currently rely on HTTP status and typed successful payloads. The wire Envelope/WsError is a WebSocket shape, not a REST auth classifier. Add only a verified REST auth response/status classifier. |
| Bitrue | S for account GETs | `src/lib.rs::issue` chooses signer; typed `wire::Envelope.code` accepts 0/200. Signature rejection -1022 has an existing once-only resend, including missing-list delivery. Distinguish auth allowlist from rate/business codes; Markets' primary signed request is not account-poll recovery. |
| Orderly and aliases | P | `src/account.rs::account_response`, `wire::Rest.success`. Current Rest drops numeric auth-code metadata; add only the narrow typed field/shape actually required. Generic `success:false` is insufficient to classify credentials. Socket auth failures are not private-REST successes. |
| Kucoin futures and spot | S for account GETs; private bullet POST remains P | Futures `src/lib.rs::signed` / `signed_streamed`; spot `src/spot/mod.rs::request`. Both have typed code strings (success 200000). Preserve explicit primary definitive-order lookup routing. Code 400005 has existing signature fixtures; final auth allowlist/retry eligibility must be verified for account reads, not copied from trading. |
| MEXC | S for account GETs | `src/account.rs::signed`, `wire::Envelope { success, code }`, where code is already parsed numerically. Observe only account mode/assets/positions/order reads, not unsigned public success or command-only leverage results. Auth codes require an explicit allowlist; 429/paused_until remains rate evidence only. |
| Phemex | S for recurring account GETs; primary definitive lookup is separate | `src/lib.rs::request` selects read credentials; `wire::Envelope.code` and optional raw error are available. Add a small typed nested error only if required by real auth responses. Preserve failure/rate floors; do not treat every nonzero Phemex code as auth. |
| WEEX | S for account GETs | `src/lib.rs::signed_request` selects credentials. Account REST mostly has raw array success and status handling; `wire::Envelope` is a socket shape. A verified REST rejection decoder/status rule is needed; generic HTTP/body failure is not enough. Deferred cooldown reads retain their existing behavior. |
| ApeX Omni | P | `src/lib.rs::Pending::SignedAuth` wraps typed Code/error metadata and legacy-signature fallback. Observe nested account reads only, after final fallback. Existing 20016/401 signature-retry evidence is available; code 10003 is an insufficient-funds trading fixture, not a universal auth code. |
| BreakoutProp | P login/session | `src/lib.rs` Operation, auth_retried and session generation. Existing Login HTTP 401/403 is a definitive rejection/deactivation branch; non-Login 401 refreshes session once. Observe account-session acquisition/final private polling evidence, not public quote success. Keep existing stop/deactivation policy. |
| Coinbase futures/spot | P JWT today | `src/rest.rs::dispatch` always signs private requests with the primary Credentials; `account.rs::private_on_response` explicitly rejects 401. 403/404 can mean product/portfolio probing rather than bad credentials and must retain those branches. Saved read credentials currently reject at the JS routing guard; accepted normalization alternative below avoids a new JWT feature. |
| Crypto.com | P for posted private account reads | `src/account.rs::Read` identifies private/get-positions, user-balance, open-orders and advanced orders; typed RestEnvelope.code. Existing auth code 40101 is re-signed once. Posted reads retain primary credentials even if saved GET read credentials exist. Public GET success and user-stream auth do not reset account polling. |
| GRVT | P API-key/session credential | `src/lib.rs::Pending` has auth_resent/auth_generation; `wire::Envelope` has c/code and nested error code. Existing 1000 triggers session refresh once. First expired-session evidence and malformed login cookie/account headers are not automatically rejected API-key evidence. Classify explicit final auth/login responses and accepted private summary reads only. |
| Hyperliquid perpetuals/spot | No polling signer | `src/account.rs` /info account and spot-state reads are unsigned user-address reads. They provide no evidence that the API-wallet signing key works. Return None for these reads. Local wallet/key mismatch and rejected writes remain existing validation/command behavior; do not manufacture polling auth success. |
| Kraken futures/spot | Futures S on GET; Spot P because private reads are POST | Futures `src/account.rs::signed_parts`, `wire::Envelope {result,error,status}`; existing fixtures include exact authenticationError / unauthorized tokens. Spot `src/spot/account.rs` signs primary POST, and `spot/wire::Envelope.error` is a typed array of canonical strings. Match exact known EAPI/permission tokens rather than searching formatted result errors; preserve signature resend and retry policy. |
| Lighter / Robinhood | P signer token for active-order reads only | `src/account.rs::request_orders` uses account_auth; public account Snapshot/Lookup (and unauthenticated limits where applicable) must not report auth recovery. Local Status has typed code/success; API error code exists in wire. Add a verified token-auth rejection allowlist and capture actual token/signer slot without exposing the token. |
| OKX variants / EU | S for private GETs | `src/lib.rs` private request signing and Pending::Account; typed `wire::Envelope.code`. Existing 50113 signature fixtures demonstrate available structured evidence; define the complete auth allowlist separately. Exclude public history/charts and keep valuation/GC independent. |
| Paradex | P Stark credential for JWT acquisition, then its session | `src/account.rs::Request::Auth`, queued requests and token refresh; current wire::Envelope.error is RawValue and status checks lose structured error details. Add only required typed auth error fields. An expired JWT may require refresh; missing JWT/malformed config/transport failure is not proof of a rejected signing credential. Preserve delegated signer and direct auth routing. |
| Polymarket | P derived/configured L2 credentials for balance/orders; positions/time unsigned | `src/account.rs::Request` differentiates Time/Derive/Balance/Orders/Positions. Existing generic error is raw/string metadata, not a typed auth-code enum. Only authenticated L2 reads and definitive credential derivation rejection may emit; unsigned positions or server-time success cannot clear an L2 rejection. |
| Txflow | No polling signer | `src/account.rs` explicitly implements unsigned main-wallet reads. Header.success/code describes API/business success, not validity of the signing key. Return None for these account polls; keep signing/write outcomes separate. |
| WOO X | S for account GETs | `src/lib.rs::signed` chooses S; account Request types identify Info/Positions/Orders/Leverage. `wire::Envelope.success` exists but drops auth-code metadata. Add the narrow documented typed error code/status shape; generic success:false or business errors cannot count as auth. Preserve primary separate-history routing and ordinary repair. |

The 26 rows are implementation coverage, not a claim that 26 auth classifiers
already exist. Existing explicit typed code evidence is strongest for Bybit,
BloFin, Crypto.com, GRVT/session handling and Kraken; other rows need bounded
verified auth fixtures/code sets. Unsigned polling rows deliberately emit no
credential evidence. Do not count unsupported/non-authenticated paths as solved
by emitting success on arbitrary account HTTP 200s.

## SDK integration and credential isolation

The JS host validates the additive action and forwards an optional typed auth
callback. It must not classify network responses itself. Use the same contained
callback/error and dead/disposal boundaries as other host notifications.
RustExchange handles only its live host generation, updates the matching slot's
small bounded streak and forwards the resulting resolved polling policy through
the already implemented phase 6a path.

Base/RustExchange needs one streak per configured credential slot and an
effective streak equal to their maximum. An Accepted Primary result clears
Primary only; a failed Secondary keeps its backoff when Primary or public reads
succeed. Existing legacy no-argument Base methods can retain their primary-slot
meaning; use an explicit optional slot or a protected aggregate update seam,
without replaying a fabricated sequence of failure calls. Cap streaks as before.
No timed dedup, new durable ledger or account/key broadcast is needed.

Private endpoints can differ in permissions even for one credential. The
classifier's allowlist and Accepted scope must not claim that public or
non-auth-validating endpoints prove account access. Implementation tests should
include partial permission/error responses before making a broader recovery
claim; do not silently widen this plan into a new error taxonomy or endpoint
permission system.

## Coinbase accepted compatibility alternative

Current source is explicit: Coinbase native Options has no read_credentials,
private REST JWTs always use self.credentials, and
`transport/engine.ts::assertEngineAccountRouting` rejects secondary signing
keys for Coinbase/coinbasespot. There is no implemented read-key JWT reuse.

For saved Coinbase accounts with secondary credentials, use one effective
options copy before routing validation, transport construction and native
options serialization. Remove secondary_key, secondary_secret,
secondary_passphrase and secondaryProxyUrl from that copy. Emit one warning per
exchange instance that the saved read credential/proxy is ignored and private
reads use primary Coinbase credentials. The warning contains no credential
values; leave the saved account model unchanged. Both aliases use the same rule.

Use the same normalized copy for every boundary. Dropping only native fields
would leave the secondary GET egress active and could send a primary JWT to
the secondary proxy. Preserve primary overrideProxyUrl, public routing, normal
Coinbase JWT generation, and Authorization/Coinbase-Authorization proxy alias
handling. The explicit warning/drop alternative is the accepted small plan;
implementing a second native Credentials/JWT signer and typed secondary routing
is larger, unnecessary work for this phase.

## Required verification and acceptance boundary

- Core hook: exactly once for buffered/streamed success and rejection, including
  an HTTP-200 auth envelope without the required array; no Accepted from missing
  arrays, malformed/truncated/invalid-UTF8 data or transport failure. Preserve
  every existing callback, error result and dirty-frame/business oracle.
- Native routing: actual private read key selection and emitted slot agree for
  primary and secondary configurations. Public/signed-public success, unsigned
  account lookups, primary socket/keepalive success and other-slot successes emit
  no misleading recovery. Capture Bitget mixed-slot requests across degradation
  changes and late responses.
- Native retry controls: first signature/session rejection emits no terminal
  result when an existing resend is admitted; final rejection reports once and
  successful retry reports the correct slot. Include Bybit, Aster, ApeX,
  Bitrue, Crypto.com, GRVT and Kraken Spot's applicable paths.
- Each implemented venue classifier gets real typed auth rejection plus accepted
  private-read fixtures, ordinary business/rate/malformed controls and public
  isolation. Exact numeric/string error fields must be pinned; no fixture log or
  body-message matching substitutes for a typed response.
- Host/SDK: additive wire validation; wrong-slot success does not clear failure;
  matching accepted evidence restores policy; Infinity remains paused; server
  multiplier composes; disposed/replaced host generations cannot alter policy;
  callback failure cannot corrupt command outcomes or sibling engines.
- Actual WASM proof must drive a native invalid credential envelope through the
  host into Base-resolved backoff and later credential recovery. A manual call
  to recordAuthFailure/Success is only a propagation test, not acceptance.
- Coinbase tests prove one no-secret warning, saved options preserved, primary
  JWT generation/routing for futures and spot, and no secondary proxy request.
- Run relevant native/wasm32 strict checks, no-Value bans, targeted SDK/host tests,
  then rebuild the official combined artifact and repeat required consumer and
  harness controls. No threshold, fixture deadline, oracle, signing or request
  semantics weakening is permitted.

Only after native classification, all wire boundaries and actual consumer tests
pass can Phase 6b claim real credential backoff. This document records a plan,
not that acceptance result.


## Time-sensitive acquisition refinement

The classifier receives the current response-event time as a final `now:f64`
argument. Paradex discards successful acquisition replies older than its native
five-minute admission window; reporting Accepted for such an unused token would
incorrectly clear a credential failure. The optional hook remains pure and does
not gain Store or transport access. Core passes the same event time already
used by the unchanged response callback. Default/ordinary classifiers ignore it;
venue session-age predicates share the existing admission rule. Update the
foundation trait, its four initial WIP implementations and core/native tests,
then re-run focused/full foundation gates and independent correctness review
before resuming classifiers. No serialized host contract changes.


The time-context foundation passes 207 core/native cases in nine nonempty
suites, including the new boundary control; strict all-feature core/native/WASM
Clippy, scoped fmt and bans pass. Independent time-hook review passes. The
all-venue feature build also caught and fixed the initial Lighter observer name
collision (the existing token generator is unchanged), and an incoming Orderly
Warning log's missing LogLevel import. These are compile repairs to preserved
WIP/integration; no completed classifier coverage is claimed by them. Logs:
/tmp/bridge-auth-response-time-{tests,clippy-complete,fmt,bans}.log.

## Landing follow-ups (2026-10-21)

- BingX classifies the final answer of the landing's re-signed reads; a first
  100001 that will be resent is not evidence, and the resend keeps its tag.
  BingX, MEXC and WEEX share `AuthOutcome::accepted_list`: after typed venue
  success and existing fences, a valid required list proves acceptance regardless
  of row mapping failures. Missing/malformed envelope data yields None before
  MergedScopeAuth changes. Non-list settings keep their typed schemas; see
  Required behavior above for the single envelope-level rule.
- Polling policy flips re-arm every poll from the moment of change. If one
  scope alternates accepted/rejected faster than a slower poll's interval, that
  poll can be postponed repeatedly. Anchoring re-arms to each poll's cycle
  start fixes it; not yet done because it shifts every venue policy test's
  expected deadlines.


## Envelope evidence consistency follow-up

Implemented rule (overseer decision, 2026-10-21): a valid signed success envelope
proves credentials regardless of individual row parse/mapping failures, uncatalogued
markets or held rows. Row diagnostics and financial reducers remain unchanged.
Malformed envelopes or missing envelope-level required data yield no outcome;
typed credential rejections still report first.

One pure `AuthOutcome::accepted_list` helper uses the existing ArraySplitter
and each request's exact selector to validate the required list and surrounding
JSON, without decoding row fields. BingX, MEXC and WEEX use it after their
existing typed success/rejection and fence checks, before MergedScopeAuth can
mutate. Non-list Mode/Metadata validation is preserved. Row-quality auth gates
are removed; Required behavior and Landing follow-ups now state this same rule.

Regressions: mixed/bad rows still accept credentials while existing financial
behavior holds; empty lists accept; missing/null/wrong-shaped/truncated required
data stays inert; malformed success cannot clear a rejected grouped endpoint
before a sibling succeeds. The PR records the two false-row expectation changes.
Verification covers all three venues plus Binance, scoped engine auth tests,
strict Clippy/fmt, independent reviews, full CI and a separate landing guard.
No new venue classifiers, caller API shapes or explicit network resync.
