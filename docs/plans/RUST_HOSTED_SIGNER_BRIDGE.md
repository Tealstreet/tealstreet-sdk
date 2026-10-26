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


## Half 2 implementation slices (Binance first)

Plan, 2026-10-22. Fresh branch from actual signing-side bridge #1745
`5743c485d44e348e3d29039c4ebe83f6a78f0186`. Implementation remains local during
#1746's build-policy window; switch back to finish Bybit #1728 when it lands.

1. Add a shared account-local remote-provider adapter and declarative alias
   policy. Preserve exact primitive preimages, owned ref checks, existing
   ws-auth request/response envelope and primary-only RemoteSigner authority;
   reject required-secondary work. PayloadSigner keeps its explicit public-key
   selector and existing precedence. Declare the nine-alias REST-only private
   feed rule centrally, enabling runtime adoption for Binance first.
2. Extend bounded optional callback headers through the existing SignResult
   validation and dispatch custody, coordinated with RequestContext ownership
   before touching its files. Declare venue precedence as data: Binance callback
   overrides, Bybit fixed SIGN/APIKEY/TIMESTAMP with callback RECV-WINDOW allowed.
   Never keep an independent header map or mutate after transport bind.
3. Wire Binance trusted setup/SDK/factory/worker routing to the shared provider;
   admit keyless construction only with its owned configured provider. Suppress
   private subscription/listen-key work for PayloadSigner at its policy boundary.
   Add real-WASM plus SDK paired hosted/provider tests, preserving public bypass,
   ref/account/id/shape validation, abort/disposal/availability, deadlines and
   native retry/unknown outcomes. Bybit runtime adoption follows actual #1728.
4. Integrate actual build/RequestContext dependencies before final exact-head CI,
   full authenticated replay proof, independent review and separate merge guard.

Architecture gate: this extends shared signer/setup/provider policies. No venue
crypto, credential fallback, socket lifecycle service or pacing policy is added.
The unchanged external wire gains no required field or acknowledgement. Focused
local unit/type/lint/native gates use existing scripts; CI owns WASM/replays.


Half-2 local foundation checkpoint (not wired): the engine has a pure shared
`signing_headers` validator/merge policy, default-deny, with trusted HTTP protected
names. Bounds are 32 callback headers, 256 bytes/name, 8192 bytes/value and 32768
aggregate UTF-8 bytes; names are ASCII tokens and values reject controls except
HTAB. All metadata validates before outbound mutation. Case-insensitive callback
last-wins collapses all touched native aliases, preserving unrelated native data
and leaving empty callbacks inert. Five focused native tests pass, including a
compiled RED/green duplicate-native regression and exact aggregate boundaries.
Independent adversarial review and strict engine Clippy pass; no dispatch/capture,
SignResult, routing, feed or live provider path uses this policy yet.

RequestContext's owner agreed this lane owns signing result/header regions.
Headers must apply before ManagedHttp Bind, with existing same-attempt custody;
Pending managed/route-ready and prepare raw/protocol/revision/entropy contracts
must survive actual integration. Active http_lifecycle.rs test literals remain
with that owner until the reviewed API/head is shared.


Half-2 standalone provider checkpoint (not installed): `remoteSignerProvider.ts`
accepts captured account-owned public credential/ref bindings, with RemoteSigner
precedence over PayloadSigner and no fallback. It validates the complete bounded
primitive batch before callbacks, presents only owned public credentials, and
supports exact HMAC-SHA256 UTF-8 Bytes/Credential preimages. RemoteSigner uses the
unchanged ws-auth/hex envelope with accountName and fresh response-bound id, and
refuses required-secondary signatures. PayloadSigner receives its selected
public key and exact preimage. Optional headers are bounded attempt-local result
metadata, never a transport side map; multiple-signature header results refuse.
Abort/disconnect cleanup, malformed results, reference/receipt bounds, legacy
empty error compatibility, UTF-8/BOM and sparse arrays have focused regressions.
All 38 SDK provider tests, package typecheck/lint and independent adversarial
review pass. Construction, routing and private-feed admission remain unchanged;
this standalone checkpoint does not yet make keyless accounts run on Rust.


Half-2 shared result checkpoint: optional callback headers now cross the same
SignResult through Rust and the JS host, with unchanged empty-result serialization.
Trusted SignStep policy is copied into Pending; only HTTP/single-sign recipes may
opt in. Full metadata validates before recipe application and is merged into the
same fresh action afterward, including removal of dynamic protected aliases even
for empty callbacks. Unsigned recipes never retain callback values. Existing raw
operation deadlines, calibration revision/freshness and final release fences
remain in force; capture work defaults to denying headers. Dense bounded JS
snapshots reject sparse/provider-overridden arrays and mutable getter swaps.
All 383 native engine tests, 178 JS host tests and strict Clippy pass; two
independent reviews are clean after the protected-shadow and snapshot corrections.
No family recipe opts in yet; the standalone SDK adapter is still not installed.


Paused local work after actual #1746 landed: shared construction declarations now
cover all nine Binance/Bybit aliases (one JSON source for SDK/native generation),
with REST-only PayloadSigner intent separate from public/account eligibility and
explicit protected-header data. Engine policy/config accessors use the regions
agreed with RequestContext's owner. Initial Binance producer guards and signed
HTTP header-policy adoption are implemented locally. Registry cargo check passes;
this policy/adoption slice still needs final native regressions, independent
review, TS checks and runtime setup/provider wiring. Bybit runtime adoption is
not implemented; its current policy test intentionally pins the pre-#1728 admission.
The delegated setup capsule passed 60 focused SDK tests (38 provider +22 setup);
its native-readCredentials scrub finding is fixed with hostile-input coverage.
Final independent delta review and integration checks remain required.
No half-2 source is pushed or merge-ready. Finish #1728 first on actual #1746,
then resume this branch and integrate the actual predecessors before final proof.


### Half-2 runtime integration plan after the actual Bybit landing

Actual Bybit #1728 `3b20e602258` and KuCoin Spot profile #1753 `35949503f7f`
are now integrated by porcelain merges. Independent integration review retained
all incoming and existing source; the only conflict combined Bybit dispatch
flags with the existing remote policy declarations. Seventy focused SDK tests
and seven shared registry code-generation tests pass. This is not runtime
remote-signing verification.

Next committable phase, Binance first:

- Extend the existing shared construction declaration with closed delegated
  admission, consumed by native construction and the central SDK factory
  predicate. Declaring headers/feed behavior alone never enables a family.
- Select `DelegatedSignerSetup` before generic options are retained; capture
  public-mode and callback intent, remove callback functions as well as raw
  credentials, and install its owned provider into the existing `EngineHost`.
  RemoteSigner has authority precedence with no local fallback; configured
  PayloadSigner intent still selects REST-only feeds if both callbacks exist.
- Narrow native payload rejection only for the admitted provider-owned path.
  Preserve all public bypass, secondary read-routing metadata and unsupported
  family behavior. Close the capsule on construction/host initialization failure
  and disposal; do not add a provider queue or another signing timeout.
- Prove all four Binance private-feed guards, owned canonical signing/headers
  and genuine REST account readiness. Add factory/SDK tests and actual-WASM
  browser-half1-to-keyless-half2 round trips with independent MAC expectations.
  Keep callback provisioning worker-local; functions are never serialized in
  ordinary worker init messages. Review/narrow the existing MCP caller guard.
- Include the actual contract, callers, remaining deployment/worker limitations
  and mapping requirements in `RUST_CUTOVER_CALLERS.md` in this same PR.

Bybit follows on this branch only after its native header/feed adoption and
regressions. Hyro's WS-owned readiness makes PayloadSigner REST-only admission a
separate explicit protocol boundary: the TS subclass also inherits a payload
connection guard that prevents its handshake. This was reported to the overseer;
no fake readiness, weakened snapshot invariant or new recovery policy is planned.
RemoteSigner streaming and the four ordinary Bybit aliases remain independent.

Architecture gate: one trusted setup/provider/host admission path plus declared
codec policy, not a per-venue signing, scheduling, auth or session service. The
RequestContext lane retains HTTP release/managed lifecycle ownership; this lane
owns result headers before bind and construction/feed configuration only.


Half-2 declaration/feed verification checkpoint (still no SDK runtime admission):
`remoteSigning.enabled` now separates adoption from future codec declarations;
Binance's four aliases are declared enabled, while Bybit stays disabled until
its native adoption. `payloadSupported` is false for Hyro, true for the other
eight aliases; the overseer approved typed Unsupported/notSent for private Hyro
PayloadSigner setup because TS never completes that handshake today. Hyro
RemoteSigner remains in scope. One generator validates the shared dispatch,
header and REST-only prerequisites before enabling a declaration. Nine SDK
admission assertions were observed RED before adding these fields, then GREEN.

A native four-alias regression completes actual account REST reads and asserts
loaded balance/orders/positions, then fires queued timers through 24 seconds and
requires continued REST reconciliation with no listen-key/session/subscription
work. Disabling only the four private-feed guards produces a compiled RED leak;
both source files were restored byte-for-byte and GREEN reverified. The policy
remains immutable after startup. Existing fixtures keep default streaming/deny
policy; no existing assertions were removed.

Local gates pass: 79 focused SDK tests, eight registry code-generation tests, the
four-alias native feed regression, package typecheck/lint, strict all-target
Clippy for engine/registry/Binance, format and the signing ratchet. Independent
reviews clear the declaration-only slice, fixture/test and optional-provider
type narrowing. These checks do not yet prove keyless SDK setup, actual-WASM
provider round trips, Bybit feed/header adoption or worker caller migration.
Those runtime gates and the same-PR caller-contract section remain required.


Half-2 runtime admission checkpoint: Binance's four enabled aliases now select a
private DelegatedSignerSetup before generic options are retained. The captured
callback/public/feed intent overrides forged native JSON; RemoteSigner wins,
configured malformed callbacks reject, and no raw local material can become a
fallback. Host initialization failure and disposal retire even retained signer
wrappers. Registry admission independently requires enabled policy, supported
PayloadSigner lifecycle, injected provider and valid primary refs; a bare flag
is rejected before legacy local credentials are consumed. Native four-alias
startup verifies callback-header precedence and no private socket/listen-key work.

Owner-approved receiver contract: PayloadSigner is bound to a frozen private
snapshot containing the selected public key and declared non-secret metadata.
Secret/passphrase fields and nested signing/callback/transport objects are
excluded. Callbacks reading this.secret must hold their own material. The new
receiver regression was observed RED on the unbound call and GREEN after the
shared snapshot projection; caller documentation is included in this PR.

Verification: 77 focused SDK tests, package typecheck/lint, native construction
and delegated-provider tests and strict registry all-target Clippy pass. Two
independent reviews clear shared admission/lifetime and receiver authority.
Architecture gate: one setup/provider/registry path plus per-venue declarations;
no new crypto, retry, socket state or timing mechanism. These checks are native
and mocked SDK boundaries, not actual-WASM proof. Actual-WASM paired hosted
round trips, MCP guard adoption, Bybit feed/header adoption, current landing
integration and complete CI/replay proof remain required before readiness.


Next codec-adoption phase (Bybit): pass the already declared shared header policy
through Bybit Options into its existing HTTP SignStep, retaining default deny in
independent native fixtures. Apply that policy to normal and environment-probe
HTTP work; WS work stays default deny. Gate private connection creation with the
engine private-feed policy, preserving ordinary REST startup/polling and Hyro's
existing WS readiness. Enable the five aliases only after native regressions:
four REST-only account aliases plus all-five RemoteSigner/hosted pairing. Private
Hyro PayloadSigner remains setup Unsupported/notSent per the owner decision.
Test protected signature/key/timestamp versus overridable recvWindow header
precedence, private-feed suppression/reconciliation, and hosted provider wire.
No WS auth state/reconnect or outcome policy is added in this codec phase.


Hosted callback-header closure: signing-only captures currently deny all result
headers, which would reject a keyless PayloadSigner HTTP callback that supplies
headers. Add a trusted-policy capture entry point while preserving default deny
for all existing primitive/WS/credential captures. Store the declared HTTP policy
beside the same work/metadata/fences; validate receipt, lengths and all headers
before output. Registry HTTP formatting applies the shared protected-name policy
and returns optional bounded headers; the standalone SDK interceptor receives
these as its existing PayloadSigner result, retaining its original precedence.
No new external sign-request/response field, host map or unsigned recipe state.
Add RED/GREEN capture tests, codec policy tests and actual-WASM callback-header
round trips. Coordinate additive API with RequestContext before final integration.


Owner wire refinement: preserve legacy WS envelopes byte-for-byte, including
Bybit's shared `bybit` label for all five aliases and `{expires}`, and Binance
Spot's alias, fixed subscription request id and `{apiKey, recvWindow, timestamp}`.
Declare label/extra-data codec/fixed WS id in the same construction policy.
A shared codec derives metadata only from the owned public credential, captured
receipt and exact prepared preimage; it never reads time independently or grants
key authority from advisory Context. Ordinary HTTP primitive envelopes keep
unique correlation ids and no synthetic WS metadata. Regression tests invoke the
legacy TS WS methods with fixed clock/id and compare complete serialized
requests for each alias; production code imports no TS venue classes.


Codec/capture implementation checkpoint: four ordinary Bybit aliases load and
continue REST account polling under the shared immutable RestOnly policy; no
private auth/socket/supervision work is created. Disabling only the private
connect guard produces compiled RED, then exact source restoration is GREEN.
Normal/probe HTTP work adopts the same declared header policy, with native auth
fields protected and callback recvWindow precedence retained. Hyro RemoteSigner
keeps streaming; private PayloadSigner rejects typed at setup.

The shared hosted capture now retains trusted HTTP policy beside its exact work
and existing single-use/raw-deadline/protocol-expiry/revision fences. WS and
credential captures stay default deny. The typed output carries only bounded,
validated, policy-filtered attempt headers into the standalone interceptor.
Native capture/codec tests cover accepted HTTP, denied WS/credential headers,
malformed headers and unchanged lifetime fences.

Legacy WS wire tests compare complete serialized requests to the actual TS WS
methods for all five Bybit aliases and Binance Spot. Pure HTTP controls preserve
original aliases and unique correlation even for a Binance Spot query with the
same bytes as WS auth. The trusted canonical builders distinguish that collision:
HTTP uses opaque Bytes plus a validated stamp; native Spot WS delimits an owned
Credential part, while hosted WS has no HTTP stamp. These invariants are part of
the codec contract and must be kept when adding builders. Full admission maps
validate codec/receipt facts before any callback; a malformed later operation
cannot cause an earlier signing call. Independent review caught and cleared both
the original HTTP label/grammar collision and late-batch validation issues.

Local gates: 113 focused SDK tests, 178 host JS tests, 35 MCP tests, SDK/MCP
typecheck and lint, strict engine/registry/Binance/Bybit all-target Clippy and a
WASM debug target check pass. Native touched-package gates pass after updating
the intentional Bybit enablement assertion. Independent reviews clear Bybit
codec adoption, shared capture policy, exact WS envelope and actual-WASM test
oracles. CI run 39782904871 at ccde934c4da passed all 18 new actual-WASM
Binance/Bybit browser-to-keyless HTTP/WS, callback-header and Hyro refusal tests.
That run is not final proof: six older WASM tests and five unit expectations
still asserted the superseded TS routing/refusal behavior. Replace those with
positive declared-provider construction and actual Binance account startup:
RemoteSigner retains streaming; PayloadSigner signs REST with same-attempt
headers and creates no private session. Independent HMAC checks require actual
signed requests, and capability-only construction counts zero transport calls.
Keep native bare-constructor/no-provider and unadopted-family typed refusals.
The updated legacy unit tests, package typecheck and lint pass locally; CI must
run the updated actual-WASM tests before readiness. Owner queue places this PR
after the AsterDEX financial-fix window (#1756). Final replay proof uses that
actual landing, followed by independent evidence review and a separate guard.

The updated run 39786263884 at 50fcacdaf2b passed all 603 SDK actual-WASM
tests, 11 server, 9 MCP and 2 CLI tests. It then exposed a real mobile import
regression: delegated-provider module initialization constructed UTF-8 codecs
before native-registry startup installed them. Initialize codecs lazily at
provider work, keeping strict UTF-8, BOM and size validation unchanged. A new
focused regression reproduces the original import failure, then signs exact
Unicode bytes after host installation. Keep the existing real native-registry
WASM fixture unchanged as the end-to-end acceptance gate. This failed workflow
is historical evidence, not final replay proof.
