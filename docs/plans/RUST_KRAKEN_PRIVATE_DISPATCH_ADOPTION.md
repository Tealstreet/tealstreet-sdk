# Kraken Futures and Spot private dispatch adoption

Plan-only checkpoint, local and unpublished. Inventory/native reference:
`feat/rust-rewrite` at `49f7d1857b66fc14f5164c8ca82ce42d48a53c1d`
(fetched before writing). The implementation branch retains the reviewed local
recipe `cbc6c9b963483bb5754c3c91337ad6ee6ef22379`; this document does not merge
prerequisites, change source, activate a private caller, or authorize publication.
Owner: codex-9czdjh. HMAC review: claude-4yjlw0; credential/selection review:
claude-m26ps4; nonce core: codex-uufub4; socket fit: codex-5c57jl; Session fit:
claude-55ao7t. Preserve #1876's managed-write originals and #1798's Spot book.

## Size and exact request inventory

There are **three physical legacy HTTP sends**, not six independently removable
account-read sites. The reference request checker inventories Kraken at 22
occurrences. Replacing the three sends removes exactly three singleton hashes:
**22 -> 19**, before any separately demonstrated terminal/test-reference removal.
No site is counted merely because its caller now selects a recipe.

| Physical funnel | Native expression and source | Guaranteed removal |
| --- | --- | --- |
| Futures `signed_request` | account.rs:375, `cx.request(method, url, headers, body, pending)` | 1 |
| Futures `signed_streamed` | account.rs:388, `cx.request_streamed_body(method, url, headers, body, at, pending)` | 1 |
| Spot `send_private` | spot/account.rs:241, the private `cx.request(Method::Post, ...)` | 1 |

Exact own request-manifest removal map, each value currently one:

```json
{
  "venues/kraken/src/account.rs": {
    "34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc": 1,
    "fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6": 1
  },
  "venues/kraken/src/spot/account.rs": {
    "29a69618a4d60ba5b2b3e3872f6ab2e1c20013d981a27fb056e9a0d633b3ceb6": 1
  }
}
```

After this send-only accounting, remaining inventory is history.rs 5,
secondary_tests.rs 4, spot/account.rs 3, spot/trading.rs 2, trading.rs 5: 19.
The terminals and conservative references in those files need their own genuine
owner/completion conversion and scanner proof; the three funnel replacements
alone do not remove them. If coherent command adoption necessarily changes a
terminal, re-derive and review its exact additional delta before claiming it.
Preserve foreign inventory bytes/order. Do not rename a raw sender/reference to
manufacture a prune or add a staging/bootstrap exception without an exact grant.

## Funnel consumers and immutable selection

The production unit must move every private signer consumer coherently. The
funnel boundaries are an inventory map, not permission to run selected account
reads beside a legacy private nonce allocator.

### Futures ordinary signed requests

`account_refresh` issues GET `/derivatives/api/v3/accounts` through
`signed_request`. The trading dequeue uses the same funnel for:

- POST `/derivatives/api/v3/sendorder`, `/cancelorder`, `/batchorder`, `/editorder`;
- PUT `/derivatives/api/v3/leveragepreferences`;
- GET `/derivatives/api/v3/leveragepreferences` for the command read fallback.

Declare roles at actual call sites, never infer authority from HTTP method.
Account and leverage GETs are Read; actual writes are Mutation. Futures Slots
use `secondary_requires_primary: true`: read-only configuration performs no
private startup/provider/socket work. Preserve `Slots::has_primary()` lifecycle
checks and native silent Missing handling; do not reintroduce raw signer gates.
A Secondary claim is refused by the recipe for non-GET or canonical
`/api/history/` even if a call site supplies the wrong role.

Background account snapshots need fresh Background Read contexts, current
account/revision guards and exact scope cleanup. The command leverage read must
retain its genuine original command authority, capped caller lifetime and
original Selection; queuing and waiting cannot re-admit it as a new command.
Mutations need their coherent original owner/queue/terminal conversion, effect
journal and existing refusal/resend taxonomy. Preserve waiting/step ordering,
prior effects, progress identities and native reducers. No Read cast, fabricated
attempt, broad retry grant or local no-I/O success substitute is allowed.

### Futures streamed signed requests

`account_refresh` streams GET openorders/openpositions/leveragepreferences with
native `openOrders`/`openPositions`/`leveragePreferences` fields. These are Read
and use Secondary when configured with Primary, otherwise Primary.

Private history shares this physical funnel:

| Native method/path | Declared selection | Native streamed field |
| --- | --- | --- |
| GET `/derivatives/api/v3/fills` | Read, Secondary if configured with Primary | `fills` |
| GET `/api/history/v3/account-log` | PrimaryRead, reason History | `logs` |
| GET `/api/history/v3/orders` | PrimaryRead, reason History | `elements` |

The fills path must not be accidentally promoted merely because its caller is
history. Only canonical `/api/history/` is native Primary-only. Keep native
forms, cursor, caps, row identity/economics, accepted-row recovery and errors.
Caller-owned private history retains one original root and Selection across
pages; each physical page/retry gets a fresh child/claim from the retained
snapshot. Keep public PublicTrades and landed candle callbacks byte-exact.
Validate live root/current child before item staging, DATA, metadata or Store
publication. Retire each physical leg before a successor, and complete only its
original owner. Released failure binding and accepted-row recovery require the
history/core owner's exact source fit; do not reuse a DATA validator on an
expired physical child or invent a new partial-recovery policy.

### Spot common private POST

The one `send_private` funnel serves POST `/0/private/Balance`, `/OpenOrders`,
`/GetWebSocketsToken`, `/AddOrder`, `/AmendOrder`, `/CancelOrder`. Slots are
Primary-only, including the two account Reads. Token bootstrap is SessionAuth
selection plus a signed session Read; it is not a Mutation merely because it
uses POST. Trade operations remain genuine Mutations under their queue owners.

Preserve native form insertion order and append nonce LAST; preserve the
API-Key/API-Sign/Content-Type/Accept header order, endpoint and 20 s physical
bound. Balance/order generation, revision and snapshot/provenance reducers stay
native. Preserve one invalid-signature resend and the independent three
non-trade transport retries with native zero-delay scheduling; each eligible
reissue has a fresh dispatcher reservation/receipt/MAC and retained selection.
A Mutation resend requires the genuine classifier-issued grant; a recipe nonce
or signature error never grants resend authority. Preserve all native auth
rejection/deactivation and permission-on-write distinctions.

GetWebSocketsToken uses the Session owner's reviewed `open_session` /
`session_context` pattern, no REST Bearer. Keep the token venue-owned for native
WS frames. Explicitly retire the session on socket close/epoch change, then
reopen for the next generation; late token responses must not reach the venue
or subscribe. No fake Session for Futures socket challenge signing.

## Hard gates before production adoption

1. **Actual nonce and recipe landings.** Combo-6b must land before the nonce
   draft is published; then the reviewed nonce and recipe PRs must land in
   order, and this adoption must integrate their actual landing tips. Local
   nonce `ed5120dbeb11e41e0e2d8059b32aeaace3e98c58` and recipe `cbc6c9b9634`
   are review references, not landed dependencies. Check current immutable
   successor pins before source. The isolated proof harness and its Cargo.lock
   hunk never land in core/consumer production.
2. **Retained SelectedRead nonce accessor.** Consume the landed equivalent of
   reviewed `641b5ab50be8316d52cb7d02745e838a9f25e6b8`:
   `AdmittedRead`/`ReadLeg::queue_with_nonce(..., &KRAKEN, |identity, claim| ...)`.
   Use the exact retained `&Identity` plus non-Clone `NonceClaim` in
   `Http::futures`/`Http::spot`; no slot lookup, copied helper, getter exposing
   authority or reselect after key replacement. `NonceClaim::slot()` is metadata
   only. Plain queue cannot smuggle a claim. Reserve only at dispatcher prepare;
   global released-floor fencing and bounded same-attempt repair stay core-owned.
3. **Spot ValidateOnly, #2078.** Consume m26ps4's actual reviewed landing and
   activate `krakenspot.bridge.readCredentials = validateOnly` at cutover. The
   ignored read key is validated once with native error precedence, wiped and
   never installed/transmitted. Invalid read material must outrank malformed
   primary Base64; the reviewed correction is `825d203d5d3`. Futures installs
   its read identity normally. Do not wire Spot setup while this is held.
4. **Nonce namespace continuity.** Native counters use sha256(decoded secret);
   selected dispatch uses the trusted stable subject. Their namespaces differ.
   Equal API keys with different secrets must receive the same trusted subject;
   Futures/Spot and cross-engine uses must share the intended sequence. No
   key-hash/secret-digest fallback, parsed response seed or venue allocator.
   Obtain nonce/credential-owner review of either a continuity mechanism or an
   explicitly coordinated all-callers transition that proves old/new ordering,
   including live legacy instances. Moving all three funnels alone is not proof
   against a higher outstanding old nonce. This remains an explicit OPEN gate;
   no small account-read-only activation can bypass it.
5. **#2080 Futures socket auth progress and whole-family fit.** Consume the
   actual socket-progress landing and obtain 5c57jl's final Kraken source fit.
   OPEN starts native 15 s; one accepted nonempty challenge renews it for 15 s
   while authentication remains pending. Progress is not Accepted/Ready and
   creates no CONNECT-origin auth clock; preserve the separate 180 s payload
   watchdog. Capture challenge once per exact FeedId/socket epoch and request
   shared epoch-bound `send_signed`; provider completion and final send must
   refuse retired/equality-expired sockets. Never use a venue auth timer,
   synthetic issuer/Session or snapshot opt-in as a substitute. Resolve the
   known mixed managed/legacy socket dispatch hazard for public and private
   sockets as a whole-family fit, preserving #1798's book region and all native
   subscription/ACK behavior. #2080 alone is not Kraken adoption proof.
6. **Setup, declared policy and host admission.** Base64/untrimmed trusted
   setup preserves both native CONFIG_INVALID notices and validation order;
   metadata-only venue options and declared HmacSha512 feature/provider admission
   must compose with registry and WASM dispatch-signing wiring. Record approved
   CALLERS changes: Secondary read-key egress/explicit Primary, missing or
   malformed trusted stable_id -> InvalidRequest/InvalidInput/NotSent before
   IDs/provider/I/O with no auth-health counters, and raw clock above MAX_SAFE
   ms -> InvalidTime refusal (no nonce-to-f64 cast). Do not silently impose a
   new 20 s lifetime on an existing account/session/paginated logical root;
   size/cap any new physical policy explicitly with native originals and review.
7. **Proof scope and hosted boundary.** The CLEAR real WASM engine+actual recipe
   proof (core d3f78f17074, recipe cbc, harness 0768b56a) is not actual-family
   adoption: its fixture has no-op NonceUnscheduled release. Real native family
   construction, account/read/write/session/socket release, stale repair and
   HMAC wire behavior still need proof on the integrated consumer. V5 proves
   only canonical u64-max codec+recipe MAC, never a reservation. Hosted canonical
   reserved-nonce forwarding is a tealstreet-next owner gate, outside this SDK
   slice: forward the engine's exact decimal nonce, never reserve again on the
   server. Do not claim hosted/private readiness without that consumer proof;
   unsupported adapters refuse before private I/O. Namespace continuity and
   hosted forwarding remain separate gates.

## Plan-first implementation and evidence order

After gates are concrete, commit compiling originals before production. Preserve
all existing originals, including #1876 and the recipe/adoption checkpoint;
label missing-API specs honestly rather than counting compile errors as runtime
REDs. Cover each of the three funnels and every listed role/wire case, Primary
and Secondary, public-only/keyless/read-only, malformed Base64/read validation,
missing/malformed/valid stable subjects, clock edge and exact header/form MAC.

Exercise selected snapshots after identity replacement; reversed dispatch,
provider park, stale final Claim and repaired fresh MAC; equal-key/different-
secret and cross-product/cross-engine monotonicity; all native retry budgets;
command receipt expiry, Background cleanup, genuine capacity and recovery;
strict preparation refusal versus native released-failure recovery; no duplicate
terminal/Store mutation after retirement. Preserve existing native account and
financial assertions and disclose suffixes gated by earlier REDs.

Production must compose caller owners, all private HMAC callers and required
socket/session transitions coherently. Request the exact primitive/credential
checker flips and maps from resulting source before any bootstrap grant or
prune. The guaranteed request projection remains 3, not all 22. Recipe-only
preparation may not activate a private caller to make a smaller PR look green.

HMAC, credential, nonce, socket/session and native preservation reviews precede
freeze. Scoped changed-crate tests only, CARGO_BUILD_JOBS=4, one local build at a
time; full suites in CI. Re-run bans/ratchets/selection against the actual tip,
freeze/push only when authorized, and use one PR-triggered Rust run. This
checkpoint changes only this document and makes no source, test execution,
READY, draft publication, CI or adoption claim.
