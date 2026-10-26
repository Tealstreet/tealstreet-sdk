# Kraken hosted exact-nonce adapter proof

Local prerequisite work for RUST_KRAKEN_PRIVATE_DISPATCH_ADOPTION.md; base
518d3953c09 (the docs plan landing). Native adoption and bridge activation stay
held. Existing local/hosted accounts and legacy routes stay working while this
unactivated adapter is proved. No NX work, release, endpoint call or key storage.

## Gap and proposed cut

The real native recipe already reserves only in DispatchContext, emits canonical
u64 decimal in SignReceipt, and builds Present -> SHA256 -> HmacSha512 work.
Current remoteSignerProvider accepts only HmacSha256, UTF-8 sign payloads and
stamp/expiry receipts; it rejects nonce and hash-result dependencies. Hosted
registry Input::Http has no nonce or request structure, its formatter expects
32-byte MACs, and signHostedHttp has no Kraken codec. Reusing those paths as-is
cannot certify a working Kraken adapter.

## Shared API disposition (m26ps4)

Direction CLEAR on c8b5513f108; source and API spelling remain gated on m26ps4's
companion envelope plan. This is the first HMAC descriptor in ONE shared
structured-preimage transport, also covering #8169's hosted Stark descriptors
later. Do not ship a Kraken-only capture API or a bare-digest WS fallback.

- Envelope: version, attempt id, original absolute attempt deadline, COMPLETE
  receipt (presence/absence significant), typed selected CredentialSlot, and a
  closed family operation descriptor. First operations: kraken.futures.http and
  kraken.spot.http; paradex.auth/paradex.order join through the companion plan.
  Kraken carries method, canonical path/form, nonce, stamp and clock revision;
  the peer recomputes the actual pure Kraken work. The deadline is never renewed
  or extended by transport. Current test kind krakenHttp is provisional only.
- Identity: the slot comes from the engine Selection, never an opaque KeyRef.
  The peer resolves its OWN installed identity for exactly that slot, verifies
  its bound account/network/trusted stable subject against the descriptor and
  refuses a missing slot. No Primary fallback or caller-chosen peer reference.
  Secondary remains read-only: no Mutation or SessionAuth; both engine and peer
  enforce it. Today's remote-Secondary refusal/hosted-Primary-only restriction
  can be lifted only through this typed slot field. On return the engine checks
  exact Present credential equality against the original owned operation graph.
- Nonce: canonical unsigned u64 DECIMAL JSON STRING in receipt and descriptor;
  never JSON number, JS Number, exponent, sign, leading zero or over-u64 value.
  The peer refuses descriptor/header/form/receipt disagreement before signing.
  Original dispatcher receipt is the sole authority; the peer never reserves,
  mints a claim, queues or mutates the retained recipe. Repair transports its NEW
  complete receipt and obtains a NEW MAC. The factored builder is signing-only
  work, never a SignStep or a dispatcher nonce override.
- Graph: family codec metadata declares Present -> Sha256 -> Result-part ->
  HmacSha512, exact 64-byte MAC and canonical padded Base64. Existing HmacSha256
  families retain exact 32-byte validation; no generic algorithm fallback.
  Validate the complete graph/descriptor/identity before callbacks. Standalone
  HTTP formatting preserves Futures sorted form/query, stripped signing path,
  APIKey/Authent/Nonce order and Spot insertion-order nonce-LAST body,
  API-Key/API-Sign order. Authentication headers cannot be substituted. No TS
  venue/interceptor, copied preimage/HMAC implementation or secret upload.
- Custody: attempt-id matching and full receipt echo; duplicate/late results are
  Stale under existing attempt custody. Cancel, timeout and dispose release once.
  Preserve clock-revision and final engine Claim/released-floor/repair fences.

Sequencing: D1 (#2132) READY first, then m26ps4's G3/G6 seams, then the companion
envelope plan (including this HMAC descriptor). Until that plan lands, this lane
stays at compiling REDs. No shared provider/capture runtime or construction-rule
activation before the reviewed companion contract. Local/hosted users keep their
working routes; #2126 remains HELD and NX mode 2 remains parked.

## Evidence order

1. Commit compiling HTTP adapter REDs at the existing signHostedHttp boundary:
   Futures and Spot at 2^53+1, 2^53+3 and u64 max; literal exact wire expectations
   plus independent node:crypto MAC recomputation. Fixture-host codec tests are
   explicitly not actual WASM proof. Keep an existing HmacSha256 working control.
2. Add malformed/ambiguous nonce, duplicate nonce field, exponent/leading-zero/
   rounded-number, path/method/host mismatch, changed receipt/credential and
   callback-result-width controls. A refusal before family admission alone is
   not evidence that the future field validator ran.
3. After the companion plan lands, implement only the reviewed shared envelope/
   capture/provider/standalone codec, with
   feature flags and live bridge admission unchanged. Scoped native registry,
   JS host and TS tests; existing hosted family controls stay exact.
4. Drive actual EngineHost plus actual Kraken-family WASM and real recipe in
   a separately hashed nondefault proof fixture. Use genuine reservations for
   2^53+1/+3 and native claim/release order; MAX is codec/capture/recipe MAC only,
   never a claimed near-limit reservation. Transport through the real remote
   adapter to a real signing peer and back, then native final Claim and one HTTP
   send. No fake engine or copied recipe may count as this proof.
5. Reverse final Claims and prove Cancel exact old preparation / same Attempt /
   new preparation, strictly greater repaired nonce and fresh MAC. Verify peer
   reserve call count zero, no late or duplicate send, and complete resource /
   signing-task drain on success, cancellation, timeout and disposal.
6. Receipt: exact heads, harness hash, Rust/bindgen/Binaryen versions, artifact
   SHA256s, commands/logs, literal vectors and independent Node recomputation.
   Merge actual landing tip first. Shared provider/codec changes are generic:
   measure Kraken, Bybit, Bitget and combined on the composed tree, even while
   bridge activation stays off. Keep fixed budgets/profiles. JOBS=4,
   one build at a time; delete own targets after each measurement/review build.

Reviewers: uufub4 dispatch/nonce/capture fit; m26ps4 shared provider/setup and
remote wire; security review for the closed peer descriptor if requested by the
owners. Native namespace takeover, numeric owner windows and mandatory Mutation
barriers remain distinct gates. A passing adapter proof alone activates none.

## Engine metadata consumer originals after #2163

The companion envelope plan #2156 and engine metadata #2163 are now landed;
817a488c9d2 is the first composition used here. The next local checkpoint first
pins the existing EngineHost's read-only forwarding seam, before any v2 capture
or transport implementation. The proposed provider context is
`{ signal: AbortSignal, meta?: SignMeta }`, with the exact engine-exported
task, owner, raw fractional deadlineMs, slot and full role. The owner stays local;
only the signing-task id, deadline, slot and role enter the reviewed envelope.
Peer-confirmed binding.generation remains independent of the engine owner.

The metadata is queried once at Action::Sign delivery, before its provider
microtask, and copied into a deeply frozen descriptor. It is never reconstructed
from timeoutMs, receipt stamps, route, method or socket. Missing metadata leaves
the existing v1 context unchanged; the v2 consumer separately refuses missing
ownership before transport. Retired tasks query neither export nor provider.
Exact callback API and validation are sent to m26ps4/fe7lgm/uufub4 before source.

Run `node --test sdk-rs/hosts/wasm/js/sign-meta.originals.mjs` explicitly. Its
non-default filename keeps intentionally RED controls out of ordinary host CI,
without skipped tests. It uses the actual EngineHost with a fixture WASM export:
no genuine engine ownership, full envelope, HMAC, reservation or Claim proof is
claimed. The closed F1 method/path allowlist, F2 replay tombstones and actual
EngineHost/family-WASM/peer resource-drain proof remain mandatory later gates.

The first metadata run executes seven controls: four assertion REDs (three
ownership shapes have no forwarded metadata, and no export is read at action
delivery) and three v1/retired-task GREENs, zero skipped. Deep-freeze and snapshot
suffixes after the missing-metadata assertions are not claimed executed. JS
syntax passes. Production host, provider, registry and venue code is unchanged.
