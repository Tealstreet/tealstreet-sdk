# Public managed HTTP header declarations

Owner: codex-5c57jl. Foundation: actual unsigned-header snapshot #1780,
RequestContext #1741, safe IDs #1789, and read validators #1792. Start from
landing `fa5d5fb90c46`; the socket diagnostic branch is separate and never merges.

## Problem and API

Hyperliquid's unsigned `/info` POST and GRVT's public JSON POSTs carry native
`Content-Type: application/json`. `Cx::request_managed` currently always has
empty headers, while the signed entry point requires a real credential and
recipe. Neither a fake credential nor a public signing recipe is appropriate.

Add `Cx::request_managed_with_headers(context, method, url, headers, body,
pending) -> Result<RequestId, (P, request_context::Error)>`, where `headers` is
`&'static [(&'static str, &'static str)]`. This is static public codec data;
credential presentation remains exclusively in the provider/signed path.

All three entry points use the existing private `queue_managed`. Use one small
private recipe declaration enum for unsigned headers versus a signed recipe,
rather than another sender or a larger argument list. Existing unsigned calls
declare `[]`; existing signed calls retain their recipe-owned header projection.
Do not change projection timing: validate Context, credential/recipe pairing and
capacity first, then read a signed declaration exactly once. Validate borrowed
headers with existing `signing_headers::validate_unsigned` before any resource
allocation or preparation publication; retain the existing single owned snapshot.

Preserve the complete subsequent queue, reservation, registration, lifecycle,
timers, route preparation, bind, final claim, delivery and failure bodies.
Private credential contexts still refuse unsigned work and return the original
pending value. New headers do not grant mutation, retry, publication or route
authority. The host receives the same existing `HttpAction::Prepare` request;
there is no new host, SDK, WASM or transport wire API.

## Scope and ownership

Production source is limited to `engine/src/http_lifecycle.rs`. Coordinate that
queue header region with uufub4 before editing; their Preflight/Plan, command
admission and callback custody remain unchanged. Tests use native HL/GRVT public
JSON intent bytes and the real managed prepare/bind/claim path. Coordinate any
actual venue builder adoption with codex-zs5e83; no unrelated family edits or
private conversion belong here. Consumer migrations remain explicit dependencies.
The caller section describes the additive API and its preserved authority rules.

## Tests, review and proof

Record the missing API as original RED. Add HL `/info` and GRVT public JSON POST
positives: exact URL/method/body/header bytes in Prepare and released transport,
no signer action/provider, route readiness, unchanged original timeout, final
claim and delivered response. Include empty headers/default unsigned compatibility,
invalid final header/size boundary before allocation, retained pending on refusal,
private Context rejection, stale Context and exhausted-ID atomicity. Existing
signed projection-once/reprepare, managed HTTP, response-proof, ownership, terminal
failure and read-delivery controls must pass unchanged.

Review the small private declaration refactor against the full original queue
body, not only the diff. Verify engine plus HL/GRVT native tests, all-target and
all-feature strict Clippy, formatting and bans. Publish a draft PR with caller
docs and reviewed API head. Final CI/proof must integrate current landing/build
inputs and preserve required raw outputs/gates under the standing evidence rules.
Announce before the separately authorized guard and head-matched merge.

## Reviewed checkpoint

The original HL and GRVT fixture compilations each fail solely because the new
entry point is absent; unrelated prototype fixture errors are retained separately.
The additive implementation then passes 1,009 focused engine/HL/GRVT native
tests, all-target/all-feature strict Clippy, formatting and bans. The ban scan
initially matched a new test name containing its reserved map-order token; the
test is renamed and passes again, with the scanner unchanged.

Both independent source readers recover the original lifecycle file byte for
byte after reversing the enum/API/header-selection additions. The complete
context/credential/capacity checks and all code from atomic namespace preflight
onward are unchanged. Actual native serializer tests cover HL candle POST and
GRVT live/testnet book POST intent through Prepare, Bind, final claim and response
delivery, while retaining every existing native assertion. These fixtures prove
the shared header contract; production venue ownership stays with the consumer
lane and is not claimed as migrated here.
