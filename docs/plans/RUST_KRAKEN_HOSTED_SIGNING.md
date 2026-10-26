# Kraken Futures and Spot hosted signing on Rust

Owner: codex-9czdjh. Plan-first on rewrite tip
`b24911c14d562e257a901f64355d6d75cdee3c2e`. Both products are beta IN.
One shared PR carries the plan, originals and source after review. Shared
provider/setup and codec review: m26ps4 and 4yjlw0; dispatch/nonce: uufub4;
engine API: fe7lgm; auditor GO: 8x5tm1. This checkpoint changes docs only.

## Result and current gaps

A Rust account with a remote or payload signer must perform its private reads,
commands and authentication without uploading the secret or constructing a TS
venue. Local-key accounts use the same Rust recipes. Enabling a construction
row alone does not accomplish this.

| Boundary on this revision | Existing implementation | Required change |
| --- | --- | --- |
| Construction | Kraken has no remoteSigning row; Spot declares only spot=true. Registry constructs native Options with raw key/secret and does not install the provided signer/identities. | Trusted setup and installed identities/provider; dispatchSigning and remoteSigning admission only with the complete working route. |
| Production signing | lib.rs and spot/mod.rs retain Signer; account.rs and spot/account.rs mint legacy nonces and MACs before raw HTTP. Futures challenge signing still uses that Signer. | Adopt the existing atomic dispatch recipes at the genuine managed HTTP and Session owners; remove production raw signing custody. |
| Existing recipes | sign/dispatch.rs builds Present, Sha256, HmacSha512 with a Result reference, and has challenge_work. These are not production adoption. | Reuse these builders, including exact nonce/body/header attestation; no second preimage implementation. |
| Shared v2 foundation | #2221 supplies hosted-operations.mjs, kraken-operation-codec.mjs, the closed HTTP descriptor, native executor, WASM exports and actual-engine proof. | Install it in the Rust account/provider facade and connect both hosted halves. Extend it for the genuine Futures Session challenge. |
| Legacy half 2 | remoteSignerProvider.ts accepts HmacSha256 only, rejects nonce-bearing receipts, and validates 32-byte MACs. | Kraken selects the structured-operation provider; existing HmacSha256 recipes and receipt/result validation remain unchanged. |
| Legacy half 1 | registry hosted_signing.rs and legacySigning.ts admit the enabled HMAC families, with 32-byte output assumptions. | Route Kraken through the closed operation executor/peer, preserving 64-byte MACs and the complete receipt. Do not shoehorn it into the generic HMAC path. |
| Existing WS wire | kraken-challenge exists for Futures WS; payloadSigner exposes hmac-sha256 or stark only. | A legacy WS challenge format is not authority to sign REST work or an arbitrary digest. Require negotiated structured operations. |

The enabled Binance/Bybit/OKX/Bitget rows and Bitunix plan #2262 establish the
same two-half obligation, per-venue algorithm advertisement, result-header
protection, and refusal without fallback. Kraken differs because HTTP consumes
an exact dispatcher nonce and binary chained work. Bitunix's fused-work change
must not accidentally make arbitrary HmacSha512 or nonce work admissible.

## Construction and provider contract

The proposed setup surface is one optional operation-channel capability shared
by RemoteSigner and PayloadSigner adapters, resolved once by trusted delegated
setup. Its exact TS spelling is a reviewer gate before source; the contract is:

- authenticated, confirmed `OperationCapabilities {v:2, recipes, slots}` and
  immutable per-slot `OperationBinding` snapshots, plus an exchange function
  carrying the existing shared module's bounded JSON frames and AbortSignal;
- exchange/account/network, public API credential, trusted stable subject and
  binding incarnation must match installed setup before start; KeyRef text is
  never a subject, and a request cannot supply or change installed bindings;
- remoteSigner retains existing precedence over payloadSigner, including when
  both are configured. Neither may fall back to a local secret or the other
  callback after a refusal, timeout or capability mismatch;
- hosted construction refuses a nonempty raw secret alongside either configured
  hosted signer, before local installation or work. The public API key remains
  a presentation input checked against the binding. Pin no dual custody;
- plain callbacks or old peers lacking this capability refuse before private
  task allocation or transport. Export presence and HmacSha512 advertisement do
  not establish peer support. Do not infer support from kraken-challenge;
- pass the real `{signal, meta}` through the setup wrapper. Its current wrapper
  drops meta; fix that without fabricating task, slot, role or deadline. Other
  families keep their current callback context and wire behavior;
- install createOperationProvider and createKrakenOperationCodec directly.
  Do not teach the generic provider to concatenate arbitrary binary parts or
  transport bare hashes. No secret, KeyRef or local keystore material crosses.

A channel adapter for the beta consumer uses this SDK module; there is no
NX-local crypto/descriptor implementation or production-bundle change. This
plan does not claim an old shipped consumer can negotiate the new route.
Actual beta eligibility requires both halves and their capability negotiation.

The intended construction rows declare dispatchSigning, protected headers
(APIKey/Authent/Nonce for Futures; API-Key/API-Sign for Spot), Base64 output,
and structured-operation support for both configured callback modes. The
existing policy schema cannot express all these predicates today. Extend the
TS/native schema and gate together, with negative controls; never enable an
unconditional remoteSigning flag while the bridge still drops the provider.
Advertise HmacSha512 only on the Kraken route, with the pure Sha256 graph
supported as a graph operation; retain existing algorithms for other families.

Futures Read selects Secondary only where the existing selection policy allows
it, with Primary required and history pinned Primary. Each offered slot is
independently negotiated; never route Secondary through Primary's remote
callback. If no Secondary capability exists, refuse that configured setup.
Spot remains Primary-only and validates then ignores its configured read slot
under the existing ValidateOnly policy. Public-only accounts remain public-only.

## Exact nonce and production custody

Follow RUST_KRAKEN_PRIVATE_DISPATCH_ADOPTION.md and the newer beta error fixes.
The production cutover includes retained background/command/history Read owners,
serial Mutation Plans, Futures challenge Session work and Spot token Session
work. Use existing Selection/NonceClaim, Prepare/Bind/Claim and final release
checks. No fake Session, command or signing task is introduced to obtain meta.

- HTTP uses the genuine dispatch receipt. Futures signs SHA256(form + decimal
  nonce + native signed path), then HMAC-SHA512; Spot signs path followed by
  SHA256(decimal nonce + the exact nonce-LAST form). The peer rebuilds from its
  own refs through the same native builder and never reserves or Claims a nonce.
- Nonces stay canonical decimal u64 strings throughout JS and transport,
  including above 2^53. No Number conversion, new counter, receive-time nonce,
  nonce-bearing callback header, minting in capture or substitution on repair.
- Preserve the closed 18 method/path table in sign/hosted.rs. Withdraw,
  WithdrawCancel, WalletTransfer and all Futures transfer/withdrawal calls
  refuse before callbacks; no prefix/wildcard expansion. Descriptor context is
  recomputed from the whole graph, not trusted as caller authority.
- The original frozen engine deadline, complete receipt and selected slot/role
  bind every response. Apply the shared 5 s business cap and engine freshness,
  clock-revision and final Claim fences. Repair cancels the exact preparation,
  keeps the real Attempt, and obtains a fresh receipt, nonce and MAC; it grants
  no write transport retry.
- Keep F2 tombstones by account/task/receipt independently of correlation id
  until max(original deadline, codec freshness horizon), including pending
  operations. Retain the bounded 64-decision/job policy and sanitized refusals.
- Keep beta #2247's structural partial IDs and Unknown/reconcile after effects,
  definitive refusal rules, guards and native order/cancel economics. No earlier
  None-after-progress disposition is carried into this cutover.

Namespace continuity is an explicit source gate, not an assumed deployment
fact. The legacy domain uses SHA256(decoded secret); dispatch uses a tagged
trusted stable subject. The current core still exports the bare legacy u64
reservation and no completed takeover API. Trusted local setup must associate
installed bytes/slot to the subject without exporting the digest. That association remains inside the trusted
LocalSigner/executor and surfaces only an opaque tagged subject; no secret hash
becomes stable_id, binding metadata or a notice. Delegated
setup receives the peer-confirmed subject rather than inventing a random one.

Before switching an owned issuer, fence its generation, join actual provider,
HTTP/body/socket and worker/process work, then atomically import both reserved
and released maxima, including the target's existing maxima. Reuse the reviewed
native-drain and NX worker/process retirement components, but do not claim those
components already prove a complete issuer census or atomic import. Exact
association/fence/import APIs require the nonce owner's review and their own
originals before shared source. Do not add a raw seed or drained boolean.
External same-key applications are outside the owned census, matching native
exposure; invalid nonce is a definitive pre-execution refusal, not authority to
replay a write. Resource retirement is not proof of remote nonexecution.

## Futures authentication and Spot token

Engine API review at 5624866e3ec found a blocking gap: current sign_meta reads
managed HTTP or address-setup tasks only. Existing send_signed WS Pending is
ownerless and cannot produce this metadata. A real Session in the venue alone
does not fix that. fe7lgm owns the proposed separate shared entry
`Cx::send_session_signed(socket, &Session, text, step)`: it must validate the
live Session and socket epoch, retain Owner::Session/SessionAuth/Primary,
stamp nonce-less work from the dispatch clock, preserve same-socket ordering
and the original epoch/deadline, and avoid the unrelated HTTP sequence barrier.
Its exact API, originals, implementation, non-adopter costs and assignment are
separate source gates. Do not call an imagined current entry or fake HTTP work.
HTTP and token metadata need no new engine entry after their actual adoption.


Add the separately negotiated closed `kraken.futures.challenge/1` recipe to the
shared Kraken codec. Its descriptor contains the actual challenge, environment
and engine-prepared signing stamp; its owner is the genuine current Session,
role SessionAuth, Primary slot. It contains no REST path, caller key or nonce.
It builds the same Present/Sha256/HmacSha512 graph as challenge_work and returns
exactly a 64-byte MAC, Base64 encoded for the existing subscription frames.

The signing step supplies a genuine stamp-bearing, nonce-less receipt and a
frozen deadline; the peer checks the raw deadline first and stamp freshness at
the signing moment. replayUntil uses that finite freshness end, not an
unbounded challenge lifetime. Validation and rechecks after async work happen
before results can release subscriptions. On every recipe, including the
challenge, returned Present bytes must equal binding.expectedCredential exactly
before headers or subscription frames are rendered; a peer cannot substitute a
public key under otherwise valid result widths. Preserve the current socket epoch,
one accepted challenge, three distinct ACKs, initial payload completion and
shared retirement. A stale challenge or old Session cannot publish frames.
Do not enable Futures hosted eligibility without this required recipe.

Spot obtains GetWebSocketsToken through its existing Primary Session-owned
private POST, using the HTTP descriptor and exact receipt. The returned token
belongs only to that socket epoch; it is not a signing credential and never
enters an operation frame. No separate Spot WS MAC capability is needed.

## Originals, qualification and landing

1. Keep the existing private-adoption originals and independent Node/Rust MAC
   vectors. Add real installed-provider and real EngineHost/WASM assertions
   proving provider work before any private HTTP or subscription publication.
   Disclose first failing custody assertions and unexecuted suffixes honestly.
2. Cover both products, both Futures environments, all selected roles and both
   callback modes. Local/provider/peer wire equality covers method, full URL,
   form/query, body absence, ordered headers, challenge and subscriptions.
   Compare independent 64-byte MACs, not just descriptor equality.
3. Refuse old/missing negotiation, wrong account/incarnation/slot/ref, malformed
   graph, arbitrary digest, forbidden path, wrong/missing/extra result, wrong returned Present, protected
   header injection, unsafe/ambiguous nonce, mismatched receipt and stale task.
   Add setup negatives proving no secret-derived stable_id/notice/binding and
   no local provider installation when hosted mode and a raw secret coexist.
   Pre-validation refusals make zero callbacks and no HTTP; post-callback
   failures make no HTTP, subscription or partial result.
4. Exercise >2^53 nonces, genuine reversed Claim/repair, complete replay inside
   freshness after the original deadline, held/deferred/aborting providers,
   disposal, epoch replacement and independent-account/slot positives. Keep the
   original provider promises until actual settlement where resources depend
   on them; abort wrappers are not join evidence. No new setup-owned PublicKey
   or PublicAddress task is needed for symmetric Kraken signing.
5. Preserve existing HMAC-family and Bitunix controls, with per-venue algorithm
   and output-width checks. Run scoped TS tests/typecheck, native Kraken tests
   and strict clippy/fmt/bans, registry/host feature modes, Rust replay and actual
   browser-host public gates. No native-only beta readiness claim.
6. One local composed combined measurement for this shared PR, jobs 4 for that
   single WASM/size build. Tests/checks use CARGO_BUILD_JOBS=2, one cargo
   invocation per lane at a time; rerun an exit-137 resource kill. CI records family sizes. Report the actual measured
   tip, delta and Kraken/Spot cap; no local family matrix. The urgent cap fix is
   separate. Inventory changes are removal-only evidence after real adoption,
   never cosmetic relocation or diagnostic/oracle deletion.
7. Plan/API CLEAR precedes originals and source in this PR. Freeze and request
   auditor GO on correctness green; CPU/certification is nonblocking. The
   structured peer route must work end to end before its construction rows
   grant hosted eligibility. No deploy, secret upload, TS venue fallback,
   public-only override or unrelated Paradex/#2126 activation follows.

The owned channel API, nonce transition and Session challenge extension are
concrete review gates. This document is not source/API approval for those seams.
Coordinate shared provider files with Bitunix #2262 and Hyperliquid's hosted
owner; family-specific descriptors/graphs stay in their native codecs.
