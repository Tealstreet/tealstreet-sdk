# Bounded Transport API reviews

This batch reviews 24 compiler-inventoried owners and aliases. Native/default
and actual Wasm probes execute 19 runtime contracts and five explicit
classifications each. These are selected-owner results; remaining Transport
findings require separate review.

| Reviewed owners | Native evidence |
| --- | --- |
| `ResolvedTransport`, `TransportMetadata` | Object fields, optional flag presence, native region order and complete-array refusal; two independent guard-disconnect controls |
| `TransportFuture`, `RestFuture`, `FreshHeadersProvider`, `RateLimitWrapper`, `IsSignatureRejection` | Exact expanded compiler/root identities and actual capability trait probes; existing generic-alias policies remain unchanged |
| `TunnelEgress` | Actual `HowEgress` alias identity, both native variants and tagged-object refusal |
| Both signing modules' `SigningConfig`, `SignedConfig`, `HttpSigningRequest`, `HttpSigningResponse` | Native fields and variants, exact `baseURL`, optional data/params presence, retained negative-zero query bits and complete-array refusal |
| Both signing modules' `present_json` | Actual field decoding, payload values and present-null behavior; these functions remain classified as decoders |
| `DirectTransport` | Complete two-API owner; two actual URL inputs return `None`, projected by matching the returned `Option` |
| `NetworkFailure`, `RestFailure` | Constructed native error fields, retry disposition and actual request/error provenance |
| `TransformedData` | Real request-body transform output and getter reference identity |
| `WsDirectFallback` | Actual fallback URLs, attempt index, failure threshold and reset state |
| `RetryPolicy` | Stored delay callback output, retries and read-method classification within the declared callback profile |

The DirectTransport constant observation uses the finite reviewed policy from
PR #347. It retains the full owner and reports a returned capability as `Some`;
it does not fabricate an output from the input URL. Other constant methods do
not acquire permission from this review.

Run `python3 sdk-rs/safe-cex-transport/scripts/accepted-value-owner-controls.py`
from the repository root to discover fresh production compiler owners, then
disconnect five producer/getter/state operations independently. Each control
requires fresh native receipts with exactly its named output mismatch, restores
the original source bytes and requires all 24 selected receipts to pass.
Compilation failure is refused as evidence. Run source-sensitive inventory and
native probes serially with these controls.

The native runner accepts an actual inventory file and an output receipt path:
`node sdk-rs/safe-cex-transport/scripts/accepted-value-owner-native.mjs --inventory=FILE --out=FILE`.
No replacement API inventory or alternate contract input is supplied by the
mutation controller. Signing and resolution schemas retain their existing
numeric, field-presence and opaque JSON policies.
