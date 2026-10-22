# Hosted signing through the owned provider

Plan, 2026-10-22. Fresh base: actual #1735
`b1dfdb43291d5a6de7dd8808b486adddda4cb118`. Separate shared prerequisite
before Bybit #1728; no dependency on its unlanded source or setup-policy #1738.

The public RustEngineExchange.signWsAuth callback still falls back to
BaseExchange, which reads options.secret. Dispatch-signing construction consumes
that material into the trusted provider, so the existing positive Bybit hosted
fixture fails. Native send_signed WS authentication does not prove the separate
hosted browser/server callback works. Retain and repair that positive contract.

1. Inventory all public sign* APIs, current remote/hosted callsites and exact
   input/output behavior. Include signHttpRequest and signWsAuth; distinguish
   pre-existing unsupported HTTP cases from behavior removed by migration.
   Share any additional missing shared canonicalization contract before adapting it.
2. Route migrated-family hosted signing through the SAME owned injected provider
   used by dispatch. Engine::signer() already supplies an opaque provider handle;
   add the narrow AnyEngine/WASM/JS host surface, using owned identity references
   rather than hard-coded names or caller-supplied signing-key references. Build
   bounded primitive SignRequest operations from the hosted semantic request;
   never expose a generic secret-export operation or restore raw engine options.
3. Reuse local sign_ready and the host's existing asynchronous signer callback,
   SignRequest/SignResult validation and sanitized failures. Preserve exact
   input bytes/output encoding and request IDs, cancellation, bounded timeout,
   disposal/late-completion fencing and zero network startup for signing alone.
   No new crypto implementation, venue credential storage, remote signing wire
   protocol or engine dispatch/request-lifecycle policy is introduced.
4. RustEngineExchange lazily creates its existing host and selects this path for
   migrated families. Unmigrated families keep their current legacy/kept-TS path.
   Inventory HTTP support explicitly and reuse actual recipe/canonicalization
   contracts for any supported public API; never return fabricated signatures.
5. Preserve the positive hosted fixture and add independent end-to-end provider
   tests for every migrated family/alias, including local trusted setup and
   async injected/keyless providers. Assert no retained raw secrets and no HTTP
   or WS I/O, exact independent signatures, malformed/error/timeout/disposal
   behavior and unchanged unmigrated compatibility. Bybit integration must run
   the same positive flow after its actual migration is enabled.
6. Run focused native engine/Binance/registry/host/SDK and strict gates, independent
   correctness/adversarial review, rebuilt-WASM CI and authenticated replay proof.
   Keep RequestContext host wiring coordinated with codex-uufub4; no engine.rs
   dispatch edits planned. Fresh own-only separate guard precedes merge, then
   #1728 real-integrates this actual pin and refreshes CI/proof; #1739 follows.

Architecture gate: shared hosted compatibility is a cross-cutting signing
concern, solved at one host/provider choke point. Venue recipes may describe
canonical bytes; no venue-local crypto executor or retained-secret fallback.
Source implementation starts only after the inventory/concrete API audit.

Implementation checkpoint: half 1 uses the small standalone request interceptors
shared by existing TS clients and the capture-only facade; no API factory or TS
venue class is constructed. These modules and their pure codec/site tables plus
capture utility are the only TS retained specifically for remote signing after
cutover. Native provider work and capture use one atomic primitive builder.
Local capture validates/consumes its token before exposing any result; injected
capture shares host abort/timeout/result validation and fatal-engine handling.
Unsigned public requests bypass provider access entirely. Full keyless routing,
REST-only feed policy, result headers and secondary/account binding remain half 2.
Actual Bybit provider HTTP/WS positives gate #1728 integration: this prerequisite
does not enable Bybit dispatch admission early. Its current legacy positive
fixture is retained, and its closed-admission assertion must be replaced with
the actual migrated positive flow there.

## Owner-confirmed scope and implementation sequence

Remote signing remains supported on Rust. Existing sign-request/sign-response
shapes remain unchanged; neither exchange-server nor signingHost changes its
wire. There are two required halves. Half 1 restores browser-held-key public
HTTP/WS signing and precedes Bybit #1728. Half 2 accepts existing remoteSigner
and PayloadSigner options through the primitive provider so supported keyless
Binance/Bybit accounts run Rust. Half 1 alone does not complete this plan.
Actual setup-policy #1738 `ecc84a798163` is integrated. Its additive README
conflict preserved both plan rows; no source conflict occurred.

### Half 1: reuse current serializers and provider recipes

Extract existing TypeScript prepare/finish shape helpers from Binance signing
and Bybit HTTP code. Keep real qs, AxiosHeaders, tryParseJson, JSON.stringify and
nanoid; do not recreate their semantics in Rust. Native SignSteps and hosted
capture share primitive work/result builders. Binance exact encoded legacy query
already contains timestamp/window in their original JS property positions;
Bybit uses exact query/body payload and configured window spelling. Browser HTTP
capture keeps the existing primary identity selection, independently of native
GET read-slot policy. Preserve public/listen-key bypass, requestId, body removal,
headers/site/demo handling and response id/config fields. Report any unavoidable
serializer reimplementation before implementing it.

A shared bounded signing-only capture service owns construction-resolved identity
references and uses the engine's installed provider, protocol clock and revision.
It accepts only closed family recipes and exact canonical bytes, never arbitrary
key refs or secret exports. Preparation returns an opaque capture token and
SignRequest; local immediate signing and injected asynchronous providers consume
the same request. Completion validates exact receipt/results, raw timeout and
clock revision before exposing signature and API-credential presentation. Tokens
are single use; cancel/dispose retire them; no network Action, fake command,
venue callback or account startup is generated. Share host timeout/abort/result
validation with ordinary provider signing rather than add another venue queue.
Lazy signing constructs the same one-use credentialed engine/host without start.

WS signs the ORIGINAL toSign bytes. In particular Bybit must not generate a new
expiry while the caller keeps the old expiry. Factor the native MAC/encoding
builder, not a second cryptographic implementation. Unsupported output formats
fail in the existing response envelope. Default captures use primary refs. The
existing external wire has no secondary selector; half 2 must preserve that primary-only contract, not require a new
acknowledgement or silently reinterpret a secondary request.

Estimated half 1 scope: engine capture/provider validation and WASM/host facade,
Binance/Bybit shared primitive builders, extracted TS shape helpers and SDK
lifecycle wiring: about 12-18 production files, 450-750 production lines plus
600-1000 regression lines. This is a planning estimate; CI measures artifacts.
First prove shared service policy, then one Binance end-to-end vertical slice,
then Bybit capture against the frozen migration recipe before actual admission.
Independent review and actual-WASM+SDK positive HTTP AND WS tests gate readiness.
Tests also cover original WS expiry, different secondary key, retries/reserved
query property positions, nested arrays/Unicode, exact JSON bodies, public bypass,
provider failure, timeout, clock revision, disposal and no startup/network I/O.

### Half 2: keyless options adapt onto the same provider

Create a trusted account-local provider setup capsule before raw options are
stripped. Resolve only owned refs; Present yields the configured API credential,
HMAC-SHA256 consumes exact native UTF-8 preimages, results strict-decode to raw32,
and receipt echo remains exact. PayloadSigner receives the selected public key
and exact payload. RemoteSigner can carry exact MAC preimages in the existing
ws-auth request shape; it must not re-run HTTP config signing, whose interceptors
would replace native stamps/request IDs/body serialization. This is a deliberate
semantic extension within the unchanged envelope, with cross-half tests.

Preserve REST-only PayloadSigner behavior through one shared construction-time
private-feed policy, adopted by Binance before listen-key/subscription/keepalive
work and Bybit before private supervision. This one policy applies to all nine
Binance/Bybit aliases. Binance Inverse currently lacks the TS guard used by the
other aliases; the owner approved correcting that exception centrally. Do not
suppress only socket egress or mislabel the account public. Preserve callback
result headers in bounded shared per-attempt metadata, validated and retired
with the signature/receipt; no side map or transport-time mutation. SignResult currently accepts only values and
receipt; extending it requires the shared provider/dispatch path, coordinated
with RequestContext ownership. Validate header names, CR/LF, count and aggregate
size before the same attempt can release; retire them on stale completion,
retry, cancellation and timeout. Preserve each venue's existing TS header
precedence exactly as declared per-venue data in this one shared policy, without
per-venue execution branches: Binance permits callback auth-header overrides;
Bybit fixes SIGN/APIKEY/TIMESTAMP but permits a callback RECV-WINDOW override.
Tests must prove both precedences; no behavior change for existing signers.

Owner-confirmed unchanged-wire schemas: RemoteSigner receives the existing
`{id, type: 'ws-auth', exchangeName, accountName?, wsAuth: {toSign,
outputFormat: 'hex'}}` request and returns `{id, wsAuthSignature?, error?}`.
No field, version marker or acknowledgement becomes required. Account selection
and optional accountName retain existing TS semantics; validate the response id
against the pending request. PayloadSigner keeps `{exchangeName, key, payload,
scheme: 'hmac-sha256', encoding: 'hex'|'base64'}` and returns the existing
`{signature, headers?}` shape. Strictly decode the signature to the required raw
MAC bytes and bind optional headers to that exact operation/receipt.

The legacy RemoteSigner wire cannot select secondary keys: use primary-only,
document it, and refuse an operation that actually requires secondary authority
rather than guess or sign secondary bytes under primary material. No external
signer or exchange-server update is required. An optional reserved acknowledgement
header may be accepted if present, but cannot be required for existing handlers;
none is necessary for the primary-only implementation. No secret/passphrase is
carried on the wire. Audit PayloadSigner's explicit public-key selector separately
and preserve only the secondary selection it already supports.

Preserve signer availability, disposal, authority precedence, public bypass and
private readiness. Route factory/worker supported aliases to Rust only once the
paired setup/provider is installed. Do not serialize callback functions across
workers or claim deployed external migration from mirror tests.

Estimated half 2: shared private-feed/header policy 250-450 production lines,
provider capsule/routing/selection 300-550, plus 900-1600 tests across shared,
native, actual-WASM and SDK layers. Nine Binance/Bybit aliases, primary-only remote
authority and required-secondary refusal, REST-only feed suppression, custom
headers, wrong account/id, unavailability, timeout/disposal, calibration and exact unchanged-wire round trips are required.
Keep the two halves reviewable; neither size estimate is implementation proof.

Architecture gate: both halves extend shared provider/setup/host services.
Serialization stays in existing helpers; family code supplies canonical data.
Private-feed eligibility and result-header custody are shared policies, not
venue-local workarounds. The later owner-assigned socket auth/epoch core follows
the bridge and Bybit migration and is not silently implemented in this slice.

### Half 1 local verification checkpoint

Shared capture/provider policy and standalone request interceptors are implemented.
Local checks pass: 1,063 native engine/Binance/Bybit/registry/WASM tests, 173 JS
host unit tests, 43 focused SDK tests plus five lint-rule tests, SDK typecheck/
lint and Rust format/bans. Five capture fault controls prove revision, raw lease,
single-use retirement, result shape and receipt fences; three transport-timer
regressions prove arm/clear/inline-callback failures before the shared fix.
Independent correctness/adversarial reviews are clear after public unsigned
provider bypass, fatal-engine handling, closed credential schema and local
finalization fixes. Ratchet additions are exact shared-work-builder descriptors
and cfg(test) fixtures, with no new runtime secret holders or crypto execution.
The actual-WASM HTTP/WS fixture requires rebuilt CI artifacts; no local release
build or replay was run. Fresh CI, replay preservation and final separate guard
remain gates, and half 2 is unimplemented.
