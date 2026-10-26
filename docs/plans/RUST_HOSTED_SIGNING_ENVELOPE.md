# Shared hosted-signing envelope and structured preimages

Status: docs-only draft on actual b5df8834b77 (#2152, after #2150). Owner: codex-9czdjh, assigned by
claude-ddi2il. Required reviewer: claude-m26ps4, credential-core owner; uufub4
reviews dispatch/nonce fit. This folds m26ps4's c8b5513f108 direction. m26ps4
keeps D1 (#2132), then G3/G6; drafting this plan need not wait for those source
lanes. Their APIs and this plan must land before consumer source uses them.

This is ONE shared companion contract: Kraken HMAC is its first consumer;
Paradex Stark is its second declared consumer. No Kraken-only capture API,
parallel Stark envelope or blind digest oracle. The earlier P8 sketch in
RUST_SIGNER_REMOTE_WIRE_AND_IMPORT.md is background for this contract, not a
second wire implementation. Its unrelated Polymarket credential-import scope
stays with its owner. Existing v1 HMAC behavior remains byte-identical.

## Existing surfaces and missing seams

Action::Sign already carries a signing task id, SignRequest and timeoutMs.
SignRequest/SignResult are serde types; result parsing/width validation and
SignReceipt's canonical string nonce already exist. EngineHost owns task
correlation, abort/deadline, result receipt checks and final signed delivery.
However its provider callback currently receives only signal: it does not expose
the original immutable absolute deadline, retained Selection slot, or structured
preimage. New read-only metadata must come from the actual owning task/Selection,
not from callback options or advisory SignRequest.context. The exact export and
original session-login task mapping are shared-owner source gates.

Current Selection does not retain the full role/PrimaryReason, signing Pending
does not retain selected slot/role, and send_signed/connect_signed may have no
HTTP Context. The managed path sees Context Credential and routing intent;
Selection::sign's non-managed path currently forwards the request/step then binds,
leaving only route/read tags. Neither path currently exports the full role.

Proposed carriers, requiring m26ps4, fe7lgm and uufub4 review before source:
Selection retains its exact CredentialRole, including PrimaryReason and scope.
The selected role rides managed Policy/Credential into Context and the retained
signing task. A Selection-taking signing entry carries the same authority for
non-managed and Session-login send_signed/connect_signed; route/read tags cannot
substitute for that carrier. Slot comes from the owning selected Credential.
Retain complete ownership at admission through the genuine owner;
missing v2 metadata refuses before callback. Never infer Primary or SessionAuth
from method, route, socket or algorithm. Existing v1 gains no invented metadata.

Keep Action::Sign byte-identical. Prefer a feature-gated read-only accessor,
Engine::sign_meta(signing_task_id) -> Option<SignMeta>, called by EngineHost while
handling the existing action, linked only by Kraken and Paradex consumers.
SignMeta retains task id, owner, absolute deadline, slot and exact role; owner
distinguishes a managed Attempt/preparation from a genuine Session task and
Session generation. Missing metadata is None and refuses v2 before callback.
Do not add metadata fields to every family's Action or grow the v1 host wire.
Exact accessor, carriers and feature forwarding remain source/API gates; none
of these proposed APIs is claimed present or approved by this document.

Export the absolute bound already computed at emission: minimum of the owning
Context deadline and the signing-task bound, frozen at first emission, never
recomputed on Bind re-sign. Session login uses its own login-task deadline,
with its actual Session generation. A stale-Claim replacement task stays capped
by the original owning window. EngineHost exports this read-only; receive time,
timeoutMs and advisory context cannot reconstruct authority or extend a bound.

Current remoteSignerProvider accepts HmacSha256, UTF-8 sign payloads and
stamp/expiry receipts; it refuses nonce, result parts and remote Secondary.
Registry hosted capture selects Primary, has no nonce/descriptor and formats a
32-byte MAC. Paradex's legacy payload_request(hash) is a bare digest. None of
these facts proves the two proposed consumers work today.

## One versioned request/result contract

Reuse the existing authenticated sign-req/sign-res channel and its outer routing
id; keep t first for the existing frame fast path. Proposed operation version is
2 (the existing P8 reservation), with type op, never http or ws-auth. Exact field
spelling is fixed by required review before implementation. Schematically:

```text
sign-req.d = {
  id, type: op, v: 2, exchangeName, accountName,
  request: {
    attemptId, deadlineMs, receipt: SignReceipt, slot: CredentialSlot, role,
    binding: { account, network, stableSubject, expectedCredential, generation },
    descriptor: closed family descriptor
  }
}
sign-res.d = {
  id, type: op, v: 2, attemptId,
  result: SignResult
} OR { id, type: op, v: 2, attemptId, error: closed sanitized code }
```

- id is unique per provider invocation/preparation. Wire attemptId is ONLY the
  Action::Sign.attempt signing-task RequestId, not a generated request/generation
  pair; it grants no authority. It is distinct from the stable managed
  request_context::Attempt and preparation generation retained at the origin.
  Initially restrict v2 to managed work with a genuine Attempt generation and
  Session signing tasks with a genuine Session generation. Other non-managed
  tasks without defined ownership refuse v2; preserve their existing v1 path.
  Session auth never fabricates an HTTP command. The accessor retains the real
  managed-owner or Session-owner generation locally; binding.generation is the
  peer-confirmed account/slot generation from capability negotiation, constant
  for one engine instance. It is not an engine installation counter: none
  exists today; set_identities/set_signer refuse after start and no event
  replaces or clears credentials. Any peer generation change reconstructs the
  account. Never infer it from RequestId or conflate it with an owner generation.
  Exact typed owner
  representation is part of the engine API gate. Stale repair gets a NEW signing
  task, correlation and preparation plus new receipt/MAC, retaining the managed
  Attempt; an old reply can never attach to the repair.
- receipt is complete, including significant presence/absence. Nonce is a
  canonical decimal u64 JSON STRING end to end, never Number. A nonce-less
  request cannot acquire one in transport. Results echo exactly the original
  receipt; omitted, changed, extra or malformed facts refuse.
- slot and role are typed metadata from the immutable engine Selection; role
  preserves the exact Read{scope}, PrimaryRead{reason,scope}, Mutation or
  SessionAuth intent, never authority inferred from HTTP method. Peer family rules double-check
  role/path/method/slot agreement. Exact metadata export is reviewed with m26ps4,
  fe7lgm and uufub4 before source. No KeyRef, extra-role reference or
  caller-selected peer key crosses the channel.
  accountName routes only within the authenticated account binding; never
  broadcast to other accounts. Envelope claims alone confer no authority.
- Existing serde SignRequest remains the LOCAL provider work type. The wire
  request carries its structured preimage/receipt/binding, not its opaque local
  key refs or arbitrary operation graph. The peer rebuilds a real SignRequest
  with its OWN installed refs. SignResult uses existing raw byte-array values,
  receipt and existing strict parsing; do not invent a second signature format.
- Family metadata fixes the complete operation graph and widths. The origin
  checks its OWN original work against the outgoing descriptor before sending;
  the peer alone rebuilds work with its own refs. On return the origin checks
  count, widths, encoding and exact receipt echo, then maps the result only to
  that pending task. Stark additionally verifies r/s against the bound public
  subkey. A secretless HMAC origin cannot verify the MAC or inspect the peer
  graph; venue rejection remains the cryptographic backstop. No HMAC-origin
  verification claim or test substitutes for that boundary. Unknown/duplicate
  fields, tags, versions, mixed result/error or illegal graph dependencies refuse.
  Preserve
  existing 1 MiB work, 32 operation and 256-part limits. Proposed new op-wire
  bound is 1 MiB UTF-8 encoded JSON, checked before parsing and after encoding;
  review this additional bound before source. It does not alter legacy v1.
  Correlation/subject ids are bounded to 256 UTF-8 bytes, accountName to its
  existing 4096-byte cap; no unbounded reply, descriptor or pending-task map.
- A peer refuses duplicate (id, attemptId, receipt) while pending and retains a
  replay tombstone until the original deadline, refusing rather than re-signing.
  Changing id does not permit reuse of the same signing-task/receipt authority.
  Bind the cached decision to the account and descriptor; a changed descriptor
  under that authority refuses too. This grants no peer nonce reservation or
  counter. Real repair has a fresh task and receipt, as specified above.
- Declared follow-up (security F3; not a plan-landing blocker): bound concurrent
  work and replay state per authenticated account, with sanitized full-map
  refusal before callback. Pin the existing limit or a separately reviewed
  numeric limit in that follow-up, preventing cross-account starvation and
  eviction that permits replay before the original deadline. No unbounded map
  is authorized in the interim.
- Callback headers cannot replace authentication. First Kraken/Stark descriptors
  need no callback header authority; existing HmacSha256 presentation policies
  remain unchanged. Error text never includes preimages, refs, keys or results.

## Peer ownership, clocks and capabilities

The peer resolves exactly its installed slot under the authenticated owner,
account, network, stable subject and generation. A missing/mismatched slot
refuses; no Primary fallback. A generation change is Stale, never a relabelled
or replayed old write. Secondary is Read-only; both selection and peer enforce
no Mutation or SessionAuth, plus each family's stricter path/method constraints.
Compare Present's exact credential with expectedCredential and the original
engine graph before any HTTP publication. Validate binding/descriptor/whole
batch before invoking a signing callback; no partial signed output on failure.

For Paradex, credential means the account address presented as
PARADEX-STARKNET-ACCOUNT through Present; signing is the Stark scalar ref.
The expected public subkey is metadata, verified with the same provider's
PublicKey(StarkDigest) at startup. No empty/fake credential or public/private
key-length overload. Local uses trusted setup; Hosted uses injected identities
with signing refs (C8 at options parse), no local secret, and the confirmed host
algorithm declaration. Paradex is Required because auth itself needs Stark.
The approved #2119 distinction remains: inline sign_ready MissingKey may latch;
asynchronous host MissingKey refuses only its request; rotation is
Unavailable/Stale then retire_session/reconstruct, never MissingKey.

The absolute deadline is the effective ORIGINAL signing-attempt bound, copied
before transport wait and never renewed by reconnect/retry/callback. A read-only
host metadata seam must expose the retained bound; never reconstruct it as
receive-time + timeoutMs. Use finite raw UTC milliseconds within MAX_SAFE;
preserve legitimate fractional bounds, do not round them. Peer own raw time at
or beyond deadline refuses. Retain the existing 5,000ms hosted-capture ceiling;
no cap raise is authorized. Never extend an earlier caller/root/physical bound.
Longer/future supplied deadlines refuse before signing. Clock skew is a refusal,
not permission to widen a deadline. Bind the originating clock revision and
retain the peer's own revision fence through completion; revisions from another
clock are not authority to reset local calibration.

Capabilities are negotiated before account start: version + exact closed
recipe ids/versions + compiled algorithm support + bound ready slots. Only a
confirmed peer can cause the fixed engine-wide set_host_algorithms declaration;
never infer it from method presence, key length, a plausible signature or an
algorithm name alone. Stark also requires the startup PublicKey check. Unknown
or unadvertised operations refuse Unsupported/NotSent locally, before queue or
provisional effects; Prepare checks again. Negotiation changes reconstruct the
account, never mutate the live provider's ownership. The peer also checks every
request. Old peers receive no op frames; no else/WS-auth or digest fallback.

## First consumer: Kraken HMAC

Closed ids are kraken.futures.http and kraken.spot.http. The descriptor supplies
method, canonical original wire path, exact encoded form, original nonce text,
stamp and clock revision; binding carries the expected public credential and
selected subject/slot. Derive the canonical signing path with the SAME native
builder, including /derivatives stripping, not a copied TS preimage routine.

Allow only the native private consumer methods/paths from
RUST_KRAKEN_PRIVATE_DISPATCH_ADOPTION.md. Futures Read GETs outside /api/history/
may use Secondary; PrimaryRead History and all writes remain Primary. Spot
uses Primary only. Peer method/path/role mismatches refuse before signing.
The closed HTTP allowlist is exactly this current native set, with wire path
spelling preserved and no prefix/wildcard grant:

| Family | Method | Exact native wire paths |
| --- | --- | --- |
| Futures | GET | /derivatives/api/v3/accounts; /derivatives/api/v3/openorders; /derivatives/api/v3/openpositions; /derivatives/api/v3/leveragepreferences; /derivatives/api/v3/fills; /api/history/v3/account-log; /api/history/v3/orders |
| Futures | POST | /derivatives/api/v3/sendorder; /derivatives/api/v3/cancelorder; /derivatives/api/v3/batchorder; /derivatives/api/v3/editorder |
| Futures | PUT | /derivatives/api/v3/leveragepreferences |
| Spot | POST | /0/private/Balance; /0/private/OpenOrders; /0/private/GetWebSocketsToken; /0/private/AddOrder; /0/private/AmendOrder; /0/private/CancelOrder |

Every other method/path refuses before callback. In particular Spot
/0/private/Withdraw, /0/private/WithdrawCancel and /0/private/WalletTransfer,
and all Futures transfer and withdrawal endpoints are explicitly excluded.
No /secondary-test fixture path is a production grant. The
secretless origin cannot verify a peer MAC; this closed peer check must prevent
an origin from obtaining signatures for arbitrary Kraken account operations.

Reject noncanonical/signed/exponent/leading-zero/over-u64 nonce, JSON number,
duplicate nonce field, or descriptor/header/form/receipt disagreement. The
original dispatcher receipt is the sole nonce authority. The peer never reserves,
seeds, mints a Claim, queues or mutates a retained engine recipe.

Factor the actual pure Kraken builder once, shared by native recipe and bounded
signing-only capture. It returns SignRequest work, not a SignStep or nonce
constructor override. Metadata fixes Present -> Sha256 -> result-part ->
HmacSha512: SHA256 is exactly 32 bytes, MAC exactly 64, presentation canonical
padded Base64. Existing HmacSha256 consumers retain exactly 32-byte MACs.
Futures sorted form/query, native host/path, APIKey/Authent/Nonce/header order and
Spot insertion-order body with nonce LAST/API-Key/API-Sign stay byte-identical.
Stale final Claim cancels the exact old preparation; same Attempt, new request
id/preparation, higher nonce, new receipt and newly recomputed MAC.

## Second declared consumer: Paradex Stark

Closed ids are paradex.auth and paradex.order; order has a closed create/modify
variant. Cancel-by-signature, onboarding/subkey registration, transfer, withdraw,
permissions/account changes and unknown typed data are excluded. Registration
stays device-held under the main authority. No batch claim beyond the current
explicit delegated limitation. GRVT is secp256k1/EIP-712; Lighter is ECgFp5:
neither is enabled or treated as Stark by this plan.

The same closed allowlist rule applies to Stark: only the rows below authorize
a signature; every other op/method/path or type refuses before callback, even
with otherwise valid fields. This is not a /v1/ prefix grant.

| Descriptor variant | Method | Exact native target and constraint |
| --- | --- | --- |
| paradex.auth | POST | Signed Request path is exactly /v1/auth, empty body. Native HTTP target is /v1/auth/{subkey}, with subkey derived from the provider-confirmed bound public subkey, never an arbitrary path argument. |
| paradex.order create | POST | /v1/orders, or /v1/orders/batch for the native single-order representation. |
| paradex.order batch | POST | /v1/orders/batch with 1..=10 items, each exactly native plain-LIMIT batch eligibility; separately negotiated capability. Amended by the overseer's batch ruling: mode 2 replaces today's live uploaded-key signing, which places these batches (see [RUST_PARADEX_HOSTED_STARK_CONSUMER.md](RUST_PARADEX_HOSTED_STARK_CONSUMER.md)). No other multi-operation expansion. |
| paradex.order modify | PUT | /v1/orders/{encoded_order_id}, the one encoded component derived from the same typed modify id in the signed preimage and native body. |

The trusted network selects the existing native host. No arbitrary host, route,
query, schema or body can expand these rows. Onboarding, subkey registration,
withdrawal, transfer, wallet/account/permission changes, cancel-by-signature and
unknown operations remain excluded even under a valid-looking auth/order tag.
The signed /v1/auth path and native subkey-suffixed HTTP target are distinct
native facts, not interchangeable inputs. Pin positive targets and rejected
method/path/type substitutions in originals before source.

The descriptor carries the exact named domain/schema revision and typed fields,
not arbitrary type definitions, a digest or free-form JSON. Peer binds account,
chain/network, expected public subkey, slot and generation to its own account.
Auth freezes POST, signed /v1/auth, its bound native subkey target, empty body,
timestamp seconds and expiration seconds.
Order freezes native market, BUY/SELL, order type, decimal size/price strings,
signature timestamp milliseconds and modify order id where applicable. Reject
unknown market/side/type, malformed decimal/felt and field/unit substitution.
The family owner pins the complete native economic/body field projection before
source so unsigned fields cannot be swapped around a valid signed digest.

Export the actual native canonical typed-data builder as a PURE WASM function;
use shared safe-cex-crypto Stark primitives through the existing provider.
The peer recomputes the digest from those fields. If an optional diagnostic
digest is supplied it must match exactly; it never selects what is signed.
Do not invoke legacy payload_request(hash), sign arbitrary 32-byte input, copy
Stark math to TS, or let the provider recompute time/price/size/domain.
Result is exactly the native 64-byte r/s representation, ranges checked and
verified against the bound public subkey; the native codec alone renders the
canonical decimal felt pair for the venue. Wrong key/account/network/digest or
noncanonical result refuses NotSent, with nothing sent.

Signing execution deadline and signed auth validity are different: native
Credentials::auth uses timestamp=floor(nowMs/1000), expiration=timestamp+300s.
Preserve that 300s relationship; it does not extend the <=5s signing window.
Order timestamps are native milliseconds and remain frozen across any separately
authorized same-command re-sign. No new automatic resend. Expired command or
signed validity refuses; reconstruction applies only to future commands.
Security approved these peer time-policy pins: read peer wall time at the
signing moment and check the original effective deadline first; equality or
later refuses. Auth expiration must equal timestamp + 300 seconds exactly;
auth timestamp seconds must be within [peer_now - 30s, peer_now + 30s] inclusive.
Order signature timestamp milliseconds must be within peer_now +/-30,000ms
inclusive. These checks never widen or re-derive the <=5s execution bound
(minimum of the original deadline and capture cap). Out-of-skew or malformed
fields refuse NotSent before signing. Effective signed-auth validity can reach
about 330s from peer time; this is accepted and must not be silently shortened
by changing native expiration. Security approval of these numbers is plan-only,
not source authorization.

JWT uses the landed Session-bearer revision fence: /auth is Session login;
refresh moves the Bearer revision, session_ready follows refresh, NotReady parks
inside original windows, and retire_session on rotation/revocation retires old
browser/server/WS generations. This envelope grants no HTTP send or Session
readiness by itself. Mode 3 custody/KMS provisioning remains outside this SDK
plan and waits for Sam's explicit infra go; no resources are created here.

## Deferred native-only setup gate

The same shared companion records #8169's native setup prerequisite without
implementing mode 3: shared safe-cex-crypto owns Stark CSPRNG key generation
(rejection-sample [1,n-1]) and public-key derivation using the same Stark code as
the provider. Return Zeroizing<Vec<u8>> directly into a trusted native-only byte
install by move, with StarkDigest declared and public account-address credential
bytes; produce the same Required Identities. Existing insert(KeyRef, Vec<u8>)
clears algorithm declarations and is not this trusted install. Exact APIs stay
with m26ps4's credential-core review; none is claimed present.

No scalar JS String/JSON, Debug/export or second Stark implementation on the
server path; retire/remove drops refs and zeroizes their bytes. This gate does
not pretend existing device-held LocalSetup never handles a JS string. Mode 2
browser/worker custody and mode 3 service custody stay distinct. #8169's approved
Pantheon/KMS/encrypted-store topology is external work, with no resources,
provisioning or source enabled by this document; mode 3 still waits for Sam.

## Rolling safety, evidence and delivery

No bridge/private mode is enabled by planning or by an algorithm enum addition.
New browser/old server and old browser/new server must retain a working allowed
route or receive an explicitly reviewed visible refusal before private startup.
No local-secret/digest/HTTP/WS fallback is introduced. Critically, #2126 stays
HELD: scalar exclusion for live Paradex must ship with the working typed route,
never earlier, and must not flip the account publicOnly. Polymarket migration is
separate. This plan does not authorize breaking existing hosted Kraken users.
Secrets never travel in init/options/envelope/logs/Sentry/diagnostics. Signatures
travel only in bounded SignResult/native payload; JWT only at native auth/WS.

Originals precede source and retain existing hosted HmacSha256 wire controls:

The closed family allowlists (security F1) and duplicate-refusal rule (F2) are
mandatory before this plan lands, per the overseer. Their RED controls below
must execute before source; F3 remains the explicitly declared follow-up above.

1. Closed schema/version/op/capability/slot/subject/network/generation failures
   invoke no signing callback and do not enter the server fatal-error streak.
   Cross-account routing, slot fallback, unknown op disguised as http/ws-auth,
   bare digest under an allowed tag and added arbitrary graph/KeyRef all refuse.
   The 1 MiB op-wire originals exercise pre-parse receive refusal and
   post-encode send refusal, including UTF-8 byte length rather than JS length.
   Every excluded Kraken withdrawal/transfer/wallet path under an otherwise
   valid descriptor refuses before callback, alongside exact allowlist positives.
   Paradex auth/create/batch/modify positives and every excluded operation,
   changed method/path, auth subkey substitution, a 0- or 11-item batch, a
   non-eligible batch item and an un-negotiated batch also pin the same
   no-callback refusal; a valid tag never bypasses the closed rules.
   G6 Optional intentional signing absence refuses SigningNotConfigured during
   preparation, before any Action::Sign, sign_meta lookup or v2 frame. Pin that
   ordering without importing Optional behavior into Required Paradex startup.
2. Complete receipt presence/equality, exact nonce/descriptor/header/form,
   malformed u64 and 32/64-byte graph/result controls; literal vectors plus
   independent Node/Python HMAC and independent Stark verification.
3. Original/fractional/equality deadline, future deadline, clock-revision change,
   held callback, cancellation, duplicate/late result and dispose release once;
   no late signature can cause HTTP, Store effects, Session ready or retransmit.
   Repeating a pending or completed request before its original deadline, with
   the same ids or a fresh correlation id over the same task/receipt, invokes no
   second callback. Pin per-account full-map refusal and account isolation.
4. Actual EngineHost + actual family WASM + actual canonical builder + real peer:
   genuine Kraken reservations 2^53+1/+3; MAX codec/work/MAC ONLY. Reverse Claims,
   exact Cancel/new preparation/same Attempt/higher nonce/new MAC, zero peer
   reservations and complete resource/sign-task drain. Mock codec REDs do not
   count as this proof. Paradex covers local/hosted auth, order/modify, PublicKey,
   JWT refresh/WS and stale-generation refusal without main-key upload.
5. Explicit feature/flag OFF and ON controls, no enablement of GRVT/Lighter or
   other P8 families, native wire/economic/financial/retry preservation and
   no regression of existing v1 HMAC callback/headers/cleanup/late-result behavior.

Docs-only review and landing first; m26ps4 keeps D1 then G3/G6 source ownership.
Exact source/API export pins, security time policy and consumer fits precede
implementation. Do not create an NX-local typed shim while this shared lane is
pending. Then reviewed shared seams, Kraken proof first, Paradex second; actual
native adoption and hosted activation retain their separate gates.

Generic provider/codec work measures Kraken, Bybit, Bitget AND combined on the
actual composed landing tip, Node/web hashes and own deltas under unchanged
budgets/profiles, plus Paradex when adopted. Serial JOBS=4, one build at a time;
delete only own scratch targets after each measurement/review build and keep
sources/logs/receipts. No pattern process kills; stop only recorded owned PIDs.
Scoped tests/strict checks, one freeze push and one PR run, then READY to8x5tm1
and exact-tree GO before guarded head-matched landing. This document changes no
runtime, test, Cargo, inventory, flags, transport, infra, resources or deployment.
