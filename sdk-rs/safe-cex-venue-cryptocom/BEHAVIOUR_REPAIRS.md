# Crypto.com behaviour repair and explanation triage

The subsequent [DTO retention repair](DTO_RETENTION.md) supersedes the eleven open DTO findings below; this document retains its historical measurements.

Integration imported first: origin53ee502232; merge parentdc7ef67113. The previously
delivered9416 unit was already in the user's gating batch but was not yet in that
origin ref. No push or self-merge is performed.

Both existing behaviour findings are confirmed mechanisms, not merely missing
explanations: the exported array was immutable natively, and request params could
not be mutated while a borrowed result lived. The implementation now retains the
actual mutable module array and a JSON-data identity graph. It copies primitive
values, shares container nodes and preserves old roots/children after rebinding.
No lock/borrow guard escapes a method or survives a call to a user callback.

The signing visitor directly follows source key/error order and cutoff. Static
review rejected two earlier projections: full graph expansion defeated the object
cutoff, and delayed coercion errors could visit a later array DAG before throwing.
The regression includes100-node object/array DAGs and an early error before a
lexically later sibling. Deliberate full-copy/cutoff controls must select safe
cases; they must not expand that intentionally enormous resource counterexample.

The previous13 explanation findings split **13 code-to-fix /0 rationale-to-write**.
No concrete consumer reason established that preserving parity would be worse.
This unit also repairs two small code cases: lossless private-write inputs and
live interval referents/UTF-16 errors. The other11 remain explanation findings;
they are not reclassified as eleven production behaviour bugs.

| Original explanation row | Result / concrete surface |
| --- | --- |
| isNonRetryablePrivateWrite | repaired: UTF-16 input; nine ASCII substring needles |
| mapCandlestickInterval | repaired: shared referents and UTF-16 errors; source consumers fetchOHLCV/listenOHLCV |
| CryptocomInstrument | open: decoder unknown fields; source fetchMarkets |
| CryptocomBook | open: outer and BookData fields; source fetchOrderBook |
| CryptocomTicker | open: decoder unknown fields; source fetchTickers |
| CryptocomTrade | open: decoder unknown fields; no consumer found in inspected source scope |
| CryptocomCandlestick | open: decoder unknown fields; source fetchOHLCV |
| CryptocomPositionBalance | open: nested balance fields; source fetchBalance |
| CryptocomBalanceSummary | open: outer and nested balance fields |
| CryptocomOrder | open: decoder unknown fields; source fetchOrders/mapOrder |
| CryptocomAccountsResponse | open: outer and identity fields; source getAccount |
| CryptocomTradeFill | open: decoder unknown fields; no consumer found in inspected source scope |
| CryptocomError | open: decoder unknown fields; no consumer found in inspected source scope |

Enumeration stops at row12 CryptocomError. Generic native REST/WS adapters use
opaque values; no adoption of these DTO decoders was found. Repairs to those
public carriers must not be reported as proven REST/WS corrections. Unchecked
Serde flattening into Value is insufficient: shared numeric-ingress controls
already detect lost overflow through Serde Content. Extras require numeric-safe
capture, direct native key/value/bit/order assertions, and nested-object coverage.
Observed UTF-8 refusal residuals remain separately classified.

Current original18-row cohort:3 rows become ported,4 retain supported explanations,
and11 remain explanation findings. Current venue census should be122 ported /
15 divergent /0 pending after reconciliation; SHA-bound measured counts belong
in the local gate attestation, not inferred from test counts.

The scoped witnesses execute maintained complete TypeScript modules. Native
identity, fields, bits and predicates precede source projections. They retain the
280 params and42 auth floors,29 URLs per classifier and31 intervals, adding the
explicit alias/code-unit observations. These cover seven distinct executable
reference rows in the original cohort; they add no new distinct rows to the
historical seven. Initial constant observations are separate. No exhaustive
behaviour-equivalence or assembled runtime claim is made.

Governing rules remain [§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy) and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
Current applications are declared only in [signing](../parity/MANIFEST-SPEC.md#cryptocom-signing-execution-boundary)
and [helper values](../parity/MANIFEST-SPEC.md#cryptocom-helper-value-boundaries).

Required lane gates are fmt, workspace build and host workspace clippy. Wasm,
wiring, layers and routing are centralized by the user; do not run or push from
this lane. Local SHA-bound receipts carry actual exits, runtime controls and any
survivor findings. Explanation support is never behaviour-parity proof.
