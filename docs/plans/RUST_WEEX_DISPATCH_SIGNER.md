# WEEX dispatch signing and managed private HTTP

Plan first, fresh actual #1765 `a3a89719d4079c97d56297bb4738fdc13c659ceb`.
Read-only source audit against actual #1776 `35ba549959c` remains source-exact
for WEEX at this base. Owner reassigned WEEX immediately after BingX to unblock
the metadata core. No family source edits or production admission yet.

## Prerequisites and ownership

Consume the actual shared unsigned-header prerequisite (#1780), actual managed
RequestContext (#1741), trusted setup-field policy (#1738), dispatch provider
seam (#1731), signer/receipt/wire-stamp contracts and later actual shared inputs
under the normal integration rule. Do not stack implementation on an assumed
unmerged API or grow the legacy-request inventory. Coordinate metadata #1747
request fields/publication fences with its owner; preserve the native account
cycle and settings generation policy. Hosted admission remains separately gated.

## Native wire and credentials

- REST `sign.rs`: raw HMAC-SHA256, Base64 MAC over integer protocol milliseconds,
  uppercase method, native sorted path/query and exact optional body. Preserve
  native query sorting without decoding/re-encoding. Receipt stamp is the actual
  dispatch stamp; invent no nonce, recvWindow or absolute expiry field.
- `lib.rs::signed_request` is the common private builder, used by account,
  leverage, identity, history and trading. GET selects the secondary identity
  when provisioned; POST, including income reads, selects primary. Semantic
  Read/Mutation and credential selection are independent.
- Replace retained raw credentials with opaque primary/secondary identities.
  Declare WEEX's required primary/private passphrase and absent-only secondary
  primary-ref fallback in the existing trusted setup policy; explicit empty
  rejects and public mode does not waive supplied-secondary validation. Literal
  secret bytes remain literal; do not guess an encoding from their appearance.
  Preserve incomplete-primary CONFIG_INVALID/deactivation rather than treating
  an unusable supplied identity as absent and publishing an empty account. Test
  partial key/secret/passphrase inputs in native, raw and SDK setup, including
  the existing public-mode boundary.
- Declare static Content-Type `application/json` and User-Agent `Tealstreet`
  once. Reuse them in legacy references and managed intent. Final native order
  remains ACCESS-KEY, ACCESS-PASSPHRASE, ACCESS-TIMESTAMP, ACCESS-SIGN, then those
  two public fields. Credentials/signatures appear only in provider/apply output.
- Private WS upgrade signs `timestamp + /v3/ws/private`, primary only, through
  `connect_signed`. Preserve the private-header transport capability and existing
  REST safety-loop fallback, supervisor/open/subscription policy. There is no
  new login frame, session token or refresh service in this migration.

## Identity cutover: owner decision

Use trusted setup `primary.stableId`; absence yields typed identity-unavailable.
Never hash a public key, invent a fallback or restore raw-key custody in the
engine. Native accountConfig validates trading eligibility but supplies no
mapped venue UID. Preserve that validation where it remains required. Identity
GET may sign with secondary credentials, but its returned stableId always comes
from primary setup metadata; test distinct primary/secondary stableIds explicitly.

The same-PR caller section must spell out persisted migration: existing IDs are
`weex:` plus lowercase hex of the first 12 SHA256 bytes of the primary public
key. NX already holds that key and can compute the old ID once at cutover, then
map stored records to its intended stableId. The SDK/engine do not compute it.
Worker/hosted providers must receive that trusted metadata where they execute.
Any resulting replay identity/error differences must be explicitly reported;
do not normalize or relabel them as reference noise.

## Managed ownership and failure mapping

Build private HTTP through engine-issued semantic Context plus
`request_signed_managed`; attach existing streaming with `stream_response`.
Background account reads use native cycle/row ownership; command reads use their
command scope. Income POST is Read with primary credentials. Preserve the native
absence of write retries; never replay an uncertain mutation.

Map page/read lifetimes explicitly before wiring. Native account continuation
and shared-rate deferral renew a cycle timer, so silently applying the first
page's 20s bound to a whole walk would change behavior. Preserve per-page native
bounds with proper engine-issued read authority; never extend an already-issued
Context or manufacture a public Context for a private page. Bring a concrete
unrepresentable lifecycle to the owner before a workaround.
Record that page authority is minted after native rate deferral, when a
continuation starts a new bounded page/read, and how the previous Context is
finished or retired. Test a successful walk lasting over 20s with each page
within its native bound, a pause found between pages, and an old-cycle reply
after timeout and replacement. Any enclosing command deadline still applies.

Keep contexts with pending owners and settle all success/refusal/decode/timeout
paths once. Account refusal settles only its matching cycle, decrementing the
join and leaving previous snapshots; stale callbacks cannot mutate a newer
cycle. Provider refusal is not native account-auth evidence. History provider
refusal is strict typed completion even after earlier pages, separately from
native incomplete transport recovery. Leverage/settings revisions, tombstones,
normal/algo joins and optimistic rollback remain fenced.

Use RequestContext's per-leg/attempt write evidence and aggregate outcomes,
including accepted IDs and conservative prior-send uncertainty. Add no venue
ledger, new evidence flags or first-error outcome classifier.
Managed mutations must implement `capture_http_reconciliation` with bounded,
nonsecret native Call correlation and restricted `reconcile_http` ACK codecs;
their default refusal must never be bypassed. Timely ACKs report native effects
and refusals through `http_write_result` and the shared rejection/outcome path.
Retired or late ACKs cannot resurrect a command, replay a write or acquire Store
authority from advisory fields: use only the actual engine evidence/freshness
contract. Include real release tests, late ACKs at the original deadline,
cardinality/correlation mismatch and no-command-resurrection controls. No
mutation is considered adopted until that release/reconciliation path is proven.

## Verification and sequencing

1. Independent plan/API reviews, then original assertion-RED tests on actual
   native builders, provider refusal ownership and identity availability.
2. REST GET/POST/exact-query/body and WS vectors against an independent oracle;
   opaque primary/secondary selection, passphrase fallback/empty/public controls,
   malformed provider results, actual stamp/receipt and no pre-provider send.
3. Managed route/provider parking, expiry/calibration/reprepare/cancellation,
   streamed reads, page/cycle bounds and current/stale publication controls.
   No unauthenticated upgrade or second mutation after unknown effects.
4. Same-PR caller section and exact shrinking signing/request maps; every
   unrelated construction entry retained, only this family's declarations added,
   generated construction checks and independent full-map preservation review.
5. Focused engine/WEEX/Binance tests, strict Clippy/fmt/bans, independent source
   reviews, draft PR, full CI and authenticated replay/package proof. Hosted
   HTTP/WS and unchanged external-wire acceptance require their own explicit
   closure before enabling remote/public sign APIs.

Architecture gate: SignStep, provider setup, managed lifecycle and unsigned
headers remain shared choke points. WEEX supplies native codec bytes and maps its
existing semantic owners; it implements no signing custody or lifecycle service.

## Additive canonical-work checkpoint

Start implementation with opaque native recipe components on this actual-1765
branch while #1780 is unmerged. Add `dispatch_signing::{Http, Upgrade}` in
`src/sign/dispatch.rs`, exported separately from the existing raw legacy signer.
`Http::new(identity, method, path_query, body)` retains exact immutable intent;
`unsigned()` returns its native URL, two static public headers and optional body.
`Upgrade::new(identity)` attests the fixed private WS endpoint and public headers.
Both prepare from the injected integer protocol stamp, use the shared HMAC work
builder plus provider-owned passphrase Present, and apply exactly the six native
headers in credential-first order. Shared header validation supplies grammar and
bounds; recipes hold refs/bytes only, with no local crypto or material cache.

Original assertion-RED tests exercise this new contract through the actual engine
provider path, then independent existing REST/WS vectors and tamper/refusal tests
prove it. This additive phase does not rewire production builders, enable hosted
or managed construction, or claim raw-credential removal. Trusted setup and actual
builder/managed lifetime adoption follow, consuming actual #1780 before final
proof. The eventual HTTP recipe implements that shared declaration getter when
its managed builder lands; no empty-header workaround is used for admission.


The additive recipe accepts native `/capi/v3/` endpoint names (ASCII
alphanumerics, slash, underscore and hyphen) plus the shared RFC3986 query
builder's encoded spelling. It refuses arbitrary URLs, fragments, dot segments
and literal query characters requiring transport rewriting. This is bounded
native codec admission, not a URL normalizer. Existing builders' encoded query
order and bytes remain authoritative. Header validation uses one owned outbound
copy, erases it on validation failure, and moves it on success.

Checkpoint verification: the original two release assertions fail against the
unimplemented recipe, then the independent GET/POST/upgrade vectors pass through
the actual injected provider. Additional controls cover method/body/header/role
and fixed-origin tampering, absent passphrase refs with a provider that must never
be called, malformed result count/MAC/credential/header/receipt, explicit provider
refusal and absent-versus-empty body intent. This remains a component checkpoint;
production builders, trusted setup, identity cutover and managed lifecycle are
not yet migrated.

## Actual shared-header integration and trusted-policy checkpoint

Actual #1780 `02340e17712780fa526f4e07a029cd4e20fb00f5` is genuinely
integrated at `0b006f84055`. The only merge conflict was two independent README
index rows, resolved by the standing authorization: incoming shared-header row
first, then the unchanged WEEX row, each once. Deleting the WEEX row recovers the
complete incoming README. Both preservation reviews retain the four non-index
owned blobs and all 5,305 unowned incoming entries. All 475 WEEX/Binance native
targets, strict Clippy and fmt pass. Record this resolution as a PR comment when
the family draft is published.

Next bounded phase uses only landed APIs and does not turn on construction or
hosted admission:

1. Declare WEEX primary `requiredForPrivate` and secondary
   `requiredWithPrimaryFallback` passphrases in its own construction-rules entry.
   Preserve every incoming policy verbatim; no dispatch or remote-signing flag is
   added. Regenerate the existing construction table and run its check. The
   shared setup path remains the only resolver; do not trim/restore secrets in a
   venue or implement another fallback resolver.
2. Add original-RED behavior controls through the real trusted local provider
   setup using `signer_setup_policy("weex")`: a complete private primary without
   a passphrase rejects; absent secondary passphrase inherits the primary ref;
   explicit empty rejects; public mode bypasses only the primary requirement and
   still validates a supplied secondary. Pin literal secret bytes and independent
   primary/secondary stable IDs. Keep incomplete-primary validation separate
   from private family construction, whose builder adoption follows.
3. The existing opaque HTTP component already has one static UNSIGNED_HEADERS
   declaration. Implement actual #1780's `unsigned_http_headers()` getter from
   that same data. A real managed engine/probe fixture must prepare these exact
   two public headers before provider work, then sign/release the existing
   independent native vector through the provider with the exact six-header
   order. The original component currently lacks this getter, so record the
   preparation assertion RED first. This is an actual shared managed/component
   test, not production WEEX builder admission.
4. Update the relevant safe-cex trusted-policy spec and caller section in the
   same PR, describing resulting policy behavior with no merge/proof status.
   Run focused WEEX/Binance and isolated registry/setup tests, strict Clippy/fmt,
   construction generator/checks and both independent source/test reviews.

Production builder rewiring still needs the existing semantic-owner mapping,
identity cutover and complete managed read/write lifecycle proof. History keeps
its native Completion ownership until the shared bridge/read validator is
available; any required new legacy stage needs an explicit owner disposition.
No per-venue validator, group journal or timer lifecycle is introduced by this
checkpoint. The unmerged shared read-delivery validator and execution-group API
are not consumed or claimed as actual here.

### Trusted-policy and header checkpoint verification

Test-only checkpoint `3afbb29461a` reproduced five trusted-provider setup
assertion failures with two compatibility controls passing. The managed component
failed specifically because preparation received no headers instead of the two
declared public headers; all eight existing recipe controls still passed.

The correction adds only WEEX's shared setup-field declaration and returns the
existing `UNSIGNED_HEADERS` constant from its HTTP recipe getter. Removing that
one JSON line recovers the complete incoming construction-rules file byte for
byte, including every other family's policy and admission flags. Both independent
source reviews found no correctness or architecture issue. The registry's seven
WEEX setup tests and ten construction generator tests pass; 476 WEEX/Binance
native tests, 60 SDK setup tests, SDK typecheck and lint pass. Provider injection
and hosted construction remain disabled. Production request builders, identity
cutover and complete managed read/write ownership remain separate adoption work.
Strict venue/registry Clippy and workspace fmt pass after formatting the new
registry fixture and using its direct `expect_err` assertion. The full bans check
detects the newly landed Kraken history inventory reduction absent from this
older branch; this unrelated integration requirement is retained as a failing
check until actual #1768 is integrated, without changing or exempting the ratchet.

Actual #1768 `848924471635eee7a0c61c64407a861c3aea37f3` is integrated at
`1d92e296677`. Its sole conflict was independently appended caller sections,
resolved with the complete incoming file first and the unchanged WEEX section
once; the block/hash receipt is retained for the draft PR comment. Both
preservation reviews confirm all nine nonconflicting owned blobs unchanged and
all 5,301 unowned incoming entries exact. The unmodified full bans check passes
after integration, as do all 708 WEEX/Binance/Kraken native tests.

## Lane takeover status (claude-4yjlw0, 2026-10-23)

Checkpoint `794e9818c89` confirmed: 12 commits ahead and 172 behind landing
`73d04380dd9`, no PR. Recipes, trusted passphrase policy and unsigned headers
are in place. Production builders and provider admission are not adopted.

Next (priority with BingX, to unblock metadata #1747):
1. Merge pinned landing.
2. First production slice: an original-RED test first, then identity cutover to
   trusted `primary.stableId` (typed identity-unavailable, no key hash) and
   one-shot FetchLeverage through `request_signed_managed`. The CALLERS text
   covers the NX mapping from persisted `weex:<24hex>` to stableId.
3. Reverify the #1814 rows (WEEX 14/34: accepted batch IDs, typed write auth).
4. Open a draft PR and post the recorded README/CALLERS conflict resolutions
   as PR comments.
5. The joined six-read Balance cycle, paging, writes and history wait for their
   shared owners (Plan and execution groups, history Completion bridge).

## Owner-approved opaque cutover (claude-4yjlw0, 2026-10-23)

claude-ddi2il approved the same disposition as BingX. This cuts over in one PR, with no dual path:

1. **Options.** `Options` and the venue hold only opaque primary/secondary `Identity`
   refs, with no raw key, secret or passphrase custody. The registry builds WEEX from
   trusted signer setup (primary passphrase `requiredForPrivate`, secondary
   `requiredWithPrimaryFallback`), refuses raw credentials in engine options, and
   requires an injected provider when identities are present. An incomplete primary
   (a failed `validate()` or a missing passphrase ref) keeps CONFIG_INVALID deactivation.
2. **Managed reads.** Identity and FetchLeverage run on `request_signed_managed` under
   engine-issued Read Contexts, and their completions use the Context.
   - **Identity:** returns trusted `primary.stableId`. If absent, it fails typed
     (Unsupported, NotSent) before any I/O; there is no key-hash fallback.
   - **GET credentials:** GET selects the secondary identity when provisioned; POST
     selects primary.
3. **Private WS.** The private upgrade uses `connect_signed` with the existing opaque
   `Upgrade` recipe.
4. **Staging site.** Every other private request (the joined account cycle, trading,
   history) keeps its native owner. These sign through exactly ONE temporary
   `sign_http` staging site in `signed_request`, admitted by an exact checker entry.
   Remove it once the shared Plan/execution-group API and cxxyqr's history
   Completion bridge (#1848) own those requests.
5. **Ratchets.** The request inventory and the signing ratchet both shrink. The raw
   HMAC header code leaves `sign.rs`, and its fixed vectors become an independent
   test oracle.
6. **Callers.** The CALLERS section covers the NX mapping from persisted `weex:<24hex>`
   key-derived IDs to `stableId`, and provider provisioning for workers and custom
   hosts. Hosted admission stays disabled.
