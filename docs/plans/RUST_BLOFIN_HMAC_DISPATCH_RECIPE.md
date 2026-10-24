# BloFin managed HMAC dispatch recipe

Owner codex-5c57jl, 2026-10-24. Fresh local branch
`feat/rust-blofin-hmac-recipe-plan` starts at actual
`4ae5671e7dab94b5d351bebe97ba957dbe984238`.
This is the BloFin component of #1718, not a replacement for its family plan.
Plan and original executable REDs precede production. No push or CI is requested
for this checkpoint. The local target directory was 118 MB; no clean was needed.

## Ownership and existing work

The shared `feat/rust-blofin-dispatch-cutover` branch already has a test-only
`src/sign/dispatch.rs` prototype and component oracles, last checkpoint
`2d1cb21ea0b`. It is not an actual production dependency. Coordinate its exact
handoff with codex-zs5e83 rather than copying a stale consumer or claiming its
old raw `sign_http` probe is a managed release proof.

claude-55ao7t owns the later FetchLeverage consumer. It has no parked source
checkpoint. Its required endpoint is the non-streamed GET
`/api/v1/account/batch-leverage-info?instId={id}&marginMode={cross|isolated}`.
The existing unknown-market and missing-mode local refusals precede credentials
and stay unchanged. Shared custody reviewer: codex-uufub4. HMAC/native recipe
reviewer: claude-4yjlw0. No account, trading, history, WS, registry or provider
arm changes belong in this recipe checkpoint.

## Native bytes and credential contract

`sign.rs::signature` computes HMAC-SHA256 over exact bytes in this order:
`pathIncludingQuery + METHOD + integerTimestamp + nonce + exactBody`.
The output is **base64 of the 64 ASCII lowercase hex characters**, not base64
of the raw 32-byte MAC. Preserve method case, query ordering/escaping, absent
GET body and POST body bytes. The timestamp is `num::format(wire_stamp_ms(now))`.

HTTP headers retain this exact order:

1. `ACCESS-KEY`
2. `ACCESS-SIGN`
3. `ACCESS-NONCE`
4. `ACCESS-TIMESTAMP`
5. `ACCESS-PASSPHRASE`
6. `Content-Type: application/json`

Primary credentials remain required: public-only or empty primary key/secret
cannot gain private access merely because Secondary exists. Native refusal
precedence/text is `BloFin requires API credentials`, then
`BloFin requires a passphrase`. A configured Secondary is selected for GET;
otherwise GET uses Primary. Writes use Primary. Absent Secondary passphrase
inherits Primary; explicit empty does not fall back. Start-time secondary
validation remains earlier than discovery. The component does not introduce
new credential selection, bypass public-only policy, or reinterpret strings.

Use the existing canonical Slots/SelectedRead choice once in the consumer,
then pass that same opaque Identity to the recipe. The context credential,
routing intent and recipe identity must agree; never separately choose a
provider key from the HTTP method. The recipe carries the selected Identity,
including an opaque passphrase KeyRef. Trusted setup owns absent/empty fallback
and raw material. No plaintext passphrase, key or secret is stored by the recipe.

## Nonce, preparation and result contract

The native helper is named `uuid_with_entropy` but implements a nanoid-like
mapping, not an RFC UUID. One fresh signing-entropy draw fills exactly 21 bytes;
mask each with 63 into the existing URL alphabet, then filter `-` and `_`.
Do not substitute hexadecimal UUID formatting, force length 21 after filtering,
use identity entropy, retain a staging nonce, or add a local RNG.

The managed SignStep draws through DispatchContext::signing_bytes when the
shared dispatcher actually prepares provider work after route readiness.
Protocol stamp and nonce belong to that preparation. Shared clock/freshness
re-preparation gets a new stamp and fresh nonce, retaining the caller/physical
deadline and attempt semantics. Provider refusal/invalid output releases no HTTP.
Entropy failure is typed and occurs before any provider or transport release.

Provider work presents the selected credential, signs the five exact preimage
parts using its signing ref, and presents its passphrase ref. `apply` accepts
only the matching receipt, exactly three results, a 32-byte MAC, valid nonempty
credential/passphrase values and no injected signer headers. Re-attest exact
unsigned method, allowed origin, path/query, body and headers at prepare/apply.
No raw fallback, logging of material, provider-owned header injection, local
timer, retry, command terminal or outcome authority is introduced.

The intended consumer shape is the existing request_signed_managed API or the
SelectedRead queue closure, with the consumer's genuine Context and Pending.
The recipe declares unsigned Content-Type and supplies canonical provider work;
it grants no Read/Mutation authority. FetchLeverage parsing, revisions, financial
publication and terminal cleanup remain claude-55ao7t's separate slice.

## Original controls and source gates

Add a new test target only; keep every existing source/test/map byte unchanged.
Use genuine BloFin startup metadata and mode responses to reach FetchLeverage.
Original REDs separately expose eager private HTTP, missing Secondary/Primary
managed preparation, eager signing-entropy consumption and provider bypass.
Record full action batches so managed work cannot hide behind raw-only filters.
Native GREENs pin exact leverage path/query/body/header order, both GET choices,
absent Secondary passphrase fallback, explicit empty refusal, primary-required
and public-only gates, the 21-byte filtered nonce, independent HMAC literals,
and leverage result/settings with no synthetic positions.

These native REDs show the consumer boundary that needs adoption; a recipe alone
cannot make the native FetchLeverage consumer managed. Preserve the originals
when the later consumer moves construction to opaque setup/Prepare/Bind/Claim.
Do not claim a delayed action drain as physical Claim evidence.

Before recipe source freeze, add real managed probe controls for parked
provider, delayed route/preparation, result shape/receipt/intent mismatch,
entropy refusal before provider, exact Secondary/Primary preparation intent,
and clock re-preparation preserving the original deadline. Reuse the old
prototype only through an agreed source-exact handoff and review it against
current APIs. Local signer and genuinely remote provider positives are separate
from native caller observations.

No scanner, inventory or key-selection change is authorized here. Exporting a
canonical recipe is not a family custody cutover; no recipe-only provider arm
or temporary raw-material bridge is permitted. The later whole-family migration
retains its separate Plan/history/WS gates and direct-cutover rules. Architecture
gate: shared dispatcher/custody adoption, not a venue implementation of it.

Run one scoped Cargo target at a time with CARGO_BUILD_JOBS=4. CI owns broad
feature/workspace and performance checks. Commit the plan before the originals;
commit executable evidence separately and request both bounded reviews.
