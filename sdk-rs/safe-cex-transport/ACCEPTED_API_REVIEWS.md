# Bounded Transport API reviews

The first batch reviews 24 compiler-inventoried owners and aliases. Native/default
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


## Query and diagnostic reviews

A second bounded batch reviews nine complete compiler-inventoried owners. Native
and actual Wasm execution each provide eight runtime receipts and one explicit
classification. This batch adds contracts and controls to existing production
code.

| Owner | Evidence |
| --- | --- |
| `TradeTunnelTimeoutError` | Actual constructor fields, retained request context, independent diagnostic method/URL, route and Display |
| `axios_form_encode` | Native encoded bytes, key trimming, omissions, flat-array handling and invalid array-length refusal |
| `deserialize_query_wire` | Direct raw text plus stock `F64Deserializer` events; variant identity, exact numeric bits, key order and a reserved-looking literal property |
| `query_json_projection` | Actual `Option` presence, undefined omission, array null substitution, nonfinite projection and zero normalization |
| `response_data` | Encoded/native/invalid-body results, returned native kind, keys and numeric bits |
| `is_binance_style_signature_rejection` | Actual numeric/string coercion, predicate result and TypeError identity/message |
| `is_tealstreet_web_socket_url` | Actual URL parsing and host acceptance/refusal |
| `resolve_account_transport` | Executed typed conversion with independently authored flag/presence/region expectations |
| `default_proxy_region` | Specific selector classification, backed by the native preference test and its maintained-source corpus |

The selector returns an existing typed key or the fixed SG fallback. Its
classification supplies no decoder output receipt. The concrete query-field
function remains a decoder; the observation examines variants and bits before
JSON projection. Routing conversion also executes its own output witnesses.

`accepted-query-controls.py` discovers fresh production compiler owners, then
faults eight operations independently. Fresh receipts must report exactly the
named output mismatch; compilation failures are refused. The selector control
faults its fallback and runs
`native_preferences_preserve_undefined_final_operands_and_region_order`, which
compares 162 source cases and explicit default/key-order witnesses. Every fault
restores the exact source bytes and requires the corresponding restored run to
pass. Run it from the repository root, serially with other source-sensitive
inventory, probes and checks.


`accepted-query-source.mjs` independently compares five predicate cases against
the maintained TypeScript function, seven raw query inputs against engine JSON
parsing and six projection cases against engine JSON.stringify. It observes
numeric bits and key order before encoding its report. The two stock native
F64Deserializer event cases remain native witnesses.

## Body phases and secondary options

This selected follow-up reviews five owners. `RestBody` (four APIs),
`RestRequest` (two) and `RestResponseBody` (five) execute every compiler-listed
method. The secondary-options conversion also executes; the proxy fallback
helper has its own specific selector classification. No production code or
shared policy changes are part of this review.

The 58 native observations inspect actual values before JSON output:

| Owner | Observed contract |
| --- | --- |
| `RestBody` | Borrowed payload identity, encoded signing refusal, signed-data phase, retry identity, case-sensitive content type, JSON parse-before-trim and actual form/wire bytes |
| `RestRequest` | Ordered case-insensitive header merge with first spelling/last value, control filtering, both mutated retry and returned wire config, null versus absent body, retained native query bits and adapter extensions |
| `RestResponseBody` | Raw lossy UTF-8 input, current parser data, callback count and original error identity, explicit transformed provenance, physical-only BOM removal, JSON numeric bits and reserved-looking own keys, valid unrepresentable JSON refusal |
| `apply_secondary_options` | Exact credential and proxy overlay, absent versus empty secondary values, unchanged primary input and retained nested credential/native signer-type bits |
| `secondary_proxy` | Reviewed selector body plus actual 729-case TypeScript/native constructor witness, including provider fallback and factory/limiter order; this classification is not a runtime decoder receipt |

`RestResponseBody::json_value` parses encoded JSON for a model read, without
consuming a response transform. `into_data` instead supplies raw decoded text
from physical bytes. `transform` and `try_transform` always invoke the selected
callback on current data, including a string returned by an earlier parser.
The contracts observe these operations separately rather than reconstructing
phase from serialized output.

`scripts/accepted-body-source.mjs` executes installed Axios directly: 11 request
transform/byte cases, five response transforms and six actual dispatch adapter
observations. The native witnesses separately cover Rust's UTF-8 refusal,
remote adapter phases and fallible callback identity. The existing secondary
source oracle checks all 729 credential/proxy combinations and observes native
JavaScript signer numbers before fixture encoding.

Run `scripts/accepted-body-native.mjs --inventory=FILE --out=FILE` with actual
compiler inventory. `scripts/accepted-body-controls.py` discovers fresh owners,
then disconnects each of the eleven body APIs and the secondary native-number
copy independently. Every fault must compile, return named output mismatches,
and pass all five selected receipts after exact restoration. Two body faults
also affect the request that consumes them; the controller requires exactly
those declared dependent failures. The selector's separate fallback fault must
fail the named maintained-source constructor test, then restore all 729 cases.
Run these source-sensitive controls serially with all other compiler probes.

## Signature retries and remote operations

This bounded review observes five complete compiler owners: the raw-data resend
helper, rejected-response condition, resolved-response resend, combined read/write
retry engine and remote-signer operation union. It adds evidence to existing
production behavior; the TypeError carrier repair is a separate dependency.

The three decision helpers observe actual approvals, predicate counts and the
one-use stamp, including a stamp arranged through a prior production decision.
False predicates, spent stamps and thrown TypeErrors remain separate outcomes.
The rejected-response path also observes absence of a response; thrown failures
retain the actual failed/effective request and JavaScript error name/message.

The async engine runs immediate deterministic adapter receipts and an advancing
clock. Its nine cases observe every request method, URL and timeout, scheduled
zero-delay sleeps, callback count, stamp and returned success/failure. Network
reads may retry; network writes may not. Signature rejection permits one write
resend. Failed configuration rewrites survive replay, and measured attempt time
reduces the original timeout. Exhaustion consumes the approved stamp but starts
no further attempt. `accepted-retry-source.mjs` independently executes 13 cases
against the maintained signature helpers and nine traces through installed Axios
and axios-retry, with the maintained network/read and signature conditions.
Native request/error carriers are inspected separately from source Error objects.

The signer union replaces an inherited invalid `type: sample` fixture with the
actual HTTP and WS-auth variants. Native observations inspect enum identity,
account/exchange fields, HTTP method/baseURL/headers, optional null presence,
negative-zero parameter bits and untouched WS raw metadata. Valid objects and
invalid operation tags, field shapes and complete positional arrays pass through
ordinary owned, qualified and alternate decoder witnesses. The actual
`decode_response` method is executed: the same error-only wire reply must become
HTTP or WS according to its originating request; config, signature and correlation
id presence are observed without serializing a decoded owner.

`accepted-retry-native.mjs --inventory=FILE --out=FILE` uses fresh compiler APIs.
`accepted-retry-controls.py` separately faults both approval helpers, effective
error configuration, write replay configuration, timeout budget, read-network
retry, response-origin selection, WS account retention and unknown-operation
admission. Each fault must compile, produce exactly its declared named native
output/admission failures, then pass all five receipts after byte-exact source
restoration. Run these controls serially with all other source-sensitive probes.
