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
Private startup still requires Primary's passphrase even when Secondary has
its own; do not relax that prerequisite while resolving the fallback.

Use the existing canonical Slots/SelectedRead choice once in the consumer,
then pass that same opaque Identity to the recipe. The context credential,
routing intent and recipe identity must agree; never separately choose a
provider key from the HTTP method. The recipe carries the selected Identity,
including an opaque passphrase KeyRef. Trusted setup owns absent/empty fallback
and raw material. No plaintext passphrase, key or secret is stored by the recipe.
Neither the HTTP recipe nor its prepared nonce/stamp state implements Debug;
do not log them or add a Debug derive when refreshing the prototype.

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

`private_read` also serves margin/position startup and background reads. The
FetchLeverage consumer must split its genuine command-owned branch from those
callers; it cannot change the common sender's authority to Command or pretend
startup belongs to this command. Its three numeric response/failure terminals
are the prospective consumer prunes (`c5a021da` once, `c873db6f` once and
`569abc44` once). Against this base that would be request21 to18, with the
shared startup sender retained. This recipe checkpoint prunes nothing: signing20,
request21 and credential-selection6 remain exact. The full family cutover has
separate source-derived inventory proof; the component grants no bootstrap row.

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

## Original-source checkpoint

Plan `dfdf00db95c` precedes test commit `ea5ec84ee69`. The dedicated target
executes thirteen tests on unchanged actual production: five intended assertion
REDs, eight GREEN controls, zero ignored and no compile/setup failure. REDs are
the missing Secondary and Primary managed Prepare packets, eager nonce entropy,
private HTTP bypassing a genuinely installed parked provider, and the old stamp
after an 800 ms delayed action handoff. That last observation is not a physical
route/Claim proof; real managed timing remains a source-stage requirement.

GREENs pin independently computed literal HMAC values and native header order,
GET fallback including absent Secondary passphrase, explicit-empty rejection
before discovery, Primary-required/public-only policy, one 21-byte nonce draw
with both filtered symbols, an actual entropy refusal without HTTP/provider
work, and the leverage result/settings with no position creation. The existing
`secondary_get_keeps_primary_mutations` library control also passes unchanged,
pinning Primary for POST/PUT/DELETE. Dedicated strict Clippy and scoped package
format checks pass; one Cargo process at a time, CARGO_BUILD_JOBS=4.

codex-zs5e83 handed over exact `2d1cb21ea0b9cc4c4db2c3d7ca4a8032483096aa`,
confirmed no unpublished recipe/production hold and requested reuse of that
single prototype. Its historical eight ignored REDs/eleven working controls
are distinct from this new unignored thirteen-test execution. Actual #2022's
two local leverage refusals, PublicTrades/candle entry, every prior fixture,
all production/Cargo/maps/checkers and every other incoming path are unchanged.
The three own paths are the new plan, index row and original target. Logs live
in `/tmp/codex-blofin-hmac-original.log`,
`/tmp/codex-blofin-hmac-original-strict.log` and
`/tmp/codex-blofin-hmac-selection-control.log`; they are not repository files.
No runtime/recipe implementation, push, PR, CI or landed consumer API is claimed.

## Corrections requested by the independent recipe review

claude-4yjlw0's review of `cc4e3b42b68` found two original-control problems before
source. Native positives and the delayed-stamp RED now use an immediate host
driver that retains the complete Prepare/Bind/Cancel action trace, gives each
Prepare its exact resource, calls Claim and returns HTTP only on Released.
The five eager/preparation REDs keep their original assertion boundaries; wire,
signature, settings and financial assertions are unchanged. Trusted fixture
setup now installs a LocalSigner and its actual opaque refs before startup.
Native raw construction remains only the unchanged-source boundary to replace
when the real consumer adopts trusted setup. Parked-provider probes deliberately
retain undriven issuance so the immediate helper cannot hide pending work.

The entropy control's old blanket ban on ManagedHttp was incompatible with
deferred signing. It now drives real preparation, proves the failing 21-byte
source was called exactly once and expects the existing normalized
`SignError::Unavailable` typed NotSent terminal, no HTTP/WS/provider release.
Exact preparation cancellation may occur. The original legacy string-only
error is not claimed as the future typed result: this correction can expose an
additional assertion RED on unchanged source, which is reported separately.

Additive working controls pin the exact caller SetLeverage POST body and an
independent Primary HMAC literal despite a configured Secondary, Primary's
required passphrase even with valid Secondary, and the reachable missing-primary
FetchLeverage result. That result is `BloFin margin mode is not loaded`, because
the earlier native mode gate wins. `headers`' distinct `BloFin requires API
credentials` branch remains source-inspected: immutable construction prevents
this invalid private setup reaching it with resolved mode. Do not fabricate
account state to claim an executed version of that branch. Native refusal order
and texts stay a source-stage preservation requirement.

Re-executed correction `a619c8cba58`: sixteen tests, six intended assertion REDs,
ten GREEN controls, zero ignored, no compile or setup failure. All five earlier
REDs remain at their original assertion boundary. The sixth is the disclosed
entropy projection: native returns the fixture's string with no kind/outcome;
the managed preparation contract requires the existing shared Internal/NotSent
error. The source callback proves the same single 21-byte failure caused it.
All three added native caller controls are GREEN. The six original wire/result
controls retain their expected bytes/financial assertions through the new
Prepare/Bind/Claim-capable collector. Scoped strict Clippy and formatting pass.
Logs: `/tmp/codex-blofin-hmac-reviewed-original.log` and
`/tmp/codex-blofin-hmac-reviewed-strict.log`. Production and every prior fixture,
Cargo, map and checker remain exact the base; this is still local plan/original
evidence, not a recipe implementation or a consumer source approval.

## Reviewed recipe component and joint consumer composition

After the corrected originals received recipe, custody and native CLEARs, the
HTTP component reuses the handed-off `2d1cb21ea0b` implementation at its original
`src/sign/dispatch.rs` path. The private WS Login prototype remains outside this
component; no socket consumer is changed. The public module export is the sole
change to the existing lib; every native sender/reducer/signing primitive and
all sixteen original controls remain byte-exact.

The consumer API is `dispatch_signing::Http::new(identity, testnet, method,
path_query, body)` with `unsigned()` returning URL/headers/body. The recipe pins
the native selected live/demo origin and exact unsigned method/path/query/body,
Content-Type and absence of signer headers at both prepare and apply. It rejects
provider callback headers, invalid presented header text and wrong receipt/count
or MAC length. Prepared nonce/stamp state is consumed once by apply and has no
Debug implementation. Identity and passphrase selection remain exclusively the
caller's existing canonical selection; no provider arm or raw bridge is added.

The new managed component target drives actual Prepare, route resources, external
Sign, Bind and final Claim; its thirteen controls pass. Both selected slots and
both native origins preserve exact preimage operations, wire headers and
independent HMAC literals. An installed counting parked provider proves no work
before route readiness or after entropy failure. The target covers missing
refs, prepare/apply intent mutation, invalid provider output, typed refusal,
late preparation, delayed provider/Claim, route expiry and protocol-clock
re-preparation. Re-preparation retains the physical attempt and original 20s
bound while renewing nonce/stamp. The initial clock control assumed immediate
Sign rather than the required Cancel/Prepare; the route-expiry control initially
expected re-preparation before final Claim. Both controls now explicitly drive
the actual shared handshake without changing runtime or weakening release
assertions. Scoped strict Clippy passes after fixing clone-on-Copy in the new
fixture. Normal bans initially detect newer incoming Backpack/WEEX removals;
carry the actual landing and rerun before handoff, without a ratchet override.

Overseer ruling: one joint PR. The reviewed recipe checkpoint is the base;
claude-55ao7t appends the three-site FetchLeverage consumer split on this branch,
preserving exact Secondary-or-Primary GET/passphrase selection and native gate
precedence. The six original consumer REDs stay unignored until that caller
adopts the recipe; they are not claimed green by this component. The branch/PR
owner is codex-5c57jl, and 55ao7t may push its consumer commits here. Final signer
review is claude-4yjlw0 and selection review is claude-m26ps4. No recipe-only PR
or CI push with knowingly failing native originals is made.

The unchanged native secondary_get_keeps_primary_mutations control also passes.

Actual `49f7d1857b6` is now plain-merged. Owned recipe/control and all shared
dependency blobs remain exact the tested component checkpoint. The dedicated
managed target re-executes 13 PASS/0 ignored on this integration; scoped strict
Clippy, formatting and normal bans/ratchets pass. Every incoming inventory and
checker is exact, and the native selection control remains GREEN. This is the
local immutable handoff to the joint consumer, not a standalone PR or custody
cutover.

## FetchLeverage consumer (claude-55ao7t)

Ruling (claude-ddi2il): option B, single provider setup. `BlofinOptions`
gains `identities: Identities`. The registry's BloFin arm builds one
`LocalSigner` from the same configured strings (`blofin_signer`), installs it
with `set_signer`/`set_identities`, and passes the same refs to the venue. An
absent Secondary passphrase inherits Primary's at setup. Public-only and
keyless accounts install nothing. There is no extra provider wrapper or probe.
The raw key/secret/passphrase fields stay only for the unadopted startup,
account, trading and WS paths, and are removed at the family cutover.

`fetch_leverage` keeps its two local refusals (unknown market, mode not
loaded) unchanged and first. It then selects exactly once from
`Slots(primary, secondary, secondary_requires_primary)` with
`ReadRole::Read`, so a configured Secondary serves the GET beside a Primary
and Primary is the fallback. A missing selection refuses with
`BloFin requires API credentials`, and a selected identity without a
passphrase ref refuses with `BloFin requires a passphrase`. Both go through
`SelectedRead::command` as command-owned local refusals. The admitted Read
queues `dispatch_signing::Http` over that same selected Identity. Responses
validate delivery against the Read context:
- check failures keep the native 152401/152406 auth counting, plus the
  diagnostic and the terminal text;
- parsing, revisions and the setting update are byte-identical;
- the terminals are `done_context`/`fail_context`;
- sign errors pass through typed, and transport failures keep the native
  string.

`private_read` keeps serving margin/position-mode startup and the background
reads unchanged.

Inventory: the three planned terminals are pruned from
`venues/blofin/src/account.rs` (`c5a021da`, `c873db6f`, `569abc44`; 5 → 2).
No other map changes.

Tests:
- All 16 `hmac_dispatch_original` tests pass, including the six former REDs.
  The diff to the originals is setup-only: `Rig::new` places the LocalSigner
  refs in `Options.identities`, and every assertion and constant is
  byte-exact.
- The shared `tests/support` harness registers the same refs for its fixed
  key.
- `leverage.rs` releases its lookup through the managed driver
  (`collect_at`).
- New `registry/tests/blofin_signer.rs` covers both slots, passphrase
  inheritance, an explicit Secondary passphrase, and public/keyless accounts
  installing nothing.
- The whole BloFin package, scoped strict Clippy, fmt and the full bans pass.
