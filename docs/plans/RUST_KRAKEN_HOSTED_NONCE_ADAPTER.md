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

Use a separately bounded signing-only request capture, not a second nonce
allocator or a digest-only legacy WS fallback. Proposed shared-owner seam:

- The engine-side provider validates the entire owned Kraken operation graph,
  receipt and bounded HTTP descriptor before any callback. Nonce comes ONLY from
  the original receipt; exact canonical decimal survives the unchanged remote
  HTTP config as Futures Nonce header / Spot nonce form field. Selected opaque
  refs stay local, bound to the configured account/slot, never peer-selected.
- The signing peer accepts method, exact canonical path/form, supplied nonce,
  protocol stamp and clock revision in a closed capture input. It selects its
  own installed identity, checks network/method/path/form and nonce agreement,
  and builds the actual Kraken canonical work. Factor the existing pure recipe
  builder once for this capture; do not copy HMAC/preimage logic into TS. The
  capture returns SignRequest work only: it is not a SignStep and cannot queue
  or mint a NonceClaim. Dispatch preparation remains the only reserver.
- The shared provider supports the bounded SHA256/result-part/HmacSha512 graph
  and raw 64-byte MAC result with canonical padded Base64 presentation. Preserve
  the existing HmacSha256 families and their exact 32-byte validation. Family
  codec metadata owns the graph/encoding; no arbitrary algorithm fallback.
- A standalone HTTP codec assembles and validates the request with native byte
  order: Futures sorted form/query, stripped signing path, APIKey/Authent/Nonce;
  Spot exact insertion-order body with nonce LAST, API-Key/API-Sign. Preserve
  transport-owned headers but deny authentication replacement by callbacks.
  Never instantiate a TS venue, call its nonce-generating interceptor, upload
  material, parse nonce through Number, or reserve on the peer.
- The existing single-use hosted capture owns raw deadline, revision, receipt
  equality, abort/dispose and duplicate/late results. Peer completion attests the
  exact nonce and bytes; final engine Claim still enforces nonce release order.
  Stale Claim repair forwards its NEW receipt and recomputes its NEW MAC.

This is an API direction to review with m26ps4 and uufub4 before shared runtime
changes: exact descriptor/receipt transport and pure capture boundary are not
claimed to exist. Public capture cannot override a dispatch recipe's nonce.
No construction-rules runtime enablement until the complete proof is CLEAR.

## Evidence order

1. Commit compiling HTTP adapter REDs at the existing signHostedHttp boundary:
   Futures and Spot at 2^53+1, 2^53+3 and u64 max; literal exact wire expectations
   plus independent node:crypto MAC recomputation. Fixture-host codec tests are
   explicitly not actual WASM proof. Keep an existing HmacSha256 working control.
2. Add malformed/ambiguous nonce, duplicate nonce field, exponent/leading-zero/
   rounded-number, path/method/host mismatch, changed receipt/credential and
   callback-result-width controls. A refusal before family admission alone is
   not evidence that the future field validator ran.
3. Implement only the reviewed shared capture/provider/standalone codec, with
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
   Merge actual landing tip first; Kraken+combined sizes on composed tree and
   Bybit+Bitget too for generic engine changes, fixed budgets/profiles. JOBS=4,
   one build at a time; delete own targets after each measurement/review build.

Reviewers: uufub4 dispatch/nonce/capture fit; m26ps4 shared provider/setup and
remote wire; security review for the closed peer descriptor if requested by the
owners. Native namespace takeover, numeric owner windows and mandatory Mutation
barriers remain distinct gates. A passing adapter proof alone activates none.
