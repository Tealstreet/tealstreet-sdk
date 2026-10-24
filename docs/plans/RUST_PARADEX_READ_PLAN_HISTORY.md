# Paradex private history on command Read Plans

Owner codex-uufub4 for plan and original compiling assertions; production lane
owner codex-zs5e83 may take over implementation. Base actual `77f12408947`;
fresh branch `feat/rust-paradex-read-plan-history` stacks exact #2011
`9486269b6361a4c06ee1059b90b4df65a5cfe070`. This phase changes only this plan,
its index row and a new test target. No production, Cargo feature, checker,
inventory, signing, credential, host or existing-test change is authorized here.

## Scope and existing evidence

This is the history nine-occurrence subset of the 36 engine-gated Paradex
private sites in `RUST_PARADEX_ENGINE_GAPS.md` at #2032 `69d3bebfe91`.
Remote Fills, OrderHistory, Income and OrderByClientId are included. PublicTrades,
synchronous local Read terminals, account cycles, identity, trading and public
candle fallback/polling remain outside this slice. #2032's config/local-entry
source and fixture changes are not copied into this branch.

The native history owner is `history::Request` with numeric `Completion`,
`CursorHistory` and one `account.request` funnel. The exact conservative lexical
inventory in history.rs is five hashes / nine occurrences:

| Fingerprint | Count | Actual frozen expression |
| --- | --- | --- |
| 156d12f0af1aa73d997695a9ee5ae2f0b10efdaf05fbf3c5a44b49c0b8c67eb3 | 4 | matches!(request.query, OrderByClientId) |
| 4f41451eb1e1a039f8b647eaee00abfdc9027246d230dff2fad9129e9919c3e7 | 2 | matches!(request.query, PublicTrades) |
| 552d45a1b59d0f22ccc9a74886bba18ab8ed7ed63abbbaf21329272a874b4ace | 1 | format! referencing request.fallback |
| 60ad9d1f92b48483572276a3a75cf6e7c6579a0c8d0118a9f9f51688b851c9aa | 1 | matches!(request.query, Fills) |
| a81744f9afdaf13c609cee0a6b18b04c80c9fa876308896b9d619fd75c8c7b9c | 1 | account.request for the physical page |

Eight records are conservative macro references to a variable named request,
not eight additional HTTP calls. Adoption must prove any eventual removal from
actual source. Retained native geometry may retain inventory references; no
rename-only prune, checker/bootstrap rewrite or nine-independent-call claim.

Historical test-only `08c99cd236c` on zs5e83's parked private-history branch is
review input, not current execution evidence. Refresh its real delegated-auth
setup on this branch and split failing variants into independently executed
tests. Preserve all existing history financial/geometry assertions.

## Required actual prerequisites before production

1. #2011 must actually land. Its private one-way `PlanReadCompletion` and
   `PlanHistoryRows` supply exact Plan/Slot/Context/Attempt authority, atomic page
   acceptance/Slot settlement and the common prepared terminal. The local stack
   is a reviewed dependency, never an actual landing claim.
2. The complete retained-Preflight/command-Plan Session wait composition must
   actually land (fe7lgm's 9e23/a115 proposal is not an installed capability).
   Capture the original caller receipt in the genuine command callback before
   auth/queueing. Park that same owner without a fresh command, guessed 20s root
   or early physical leg. Resume only the exact Session generation, current
   owner and retained cap. Waiting before activation spends no physical window.
   Phase-1 #2037 alone does not supply retained Preflight wait or retry. Its
   a2c54 readiness/outer-flush findings are closed by bounded source review of
   b44c9cebc10 (42 scoped controls PASS); that pin is not an actual landing.
3. m26ps4's Session-fenced JIT Bearer hook must actually land. JWT remains owned
   by Account and is read at actual prepare and every reprepare/new attempt,
   behind exact target/attempt/Session validation. Do not bake Authorization into
   queued work, manufacture a SignStep/credential or put JWT in engine Identity.
   Current native private history has no automatic 401 resend. Preserve that
   strict failure; any future refresh/resend would require a separately reviewed
   declared attempt and current JWT, never reuse of an old packet.

#2011 alone does not unlock private adoption. Exact wait/JIT signatures,
credential selection and context issuance need joint Plan/Session/header-owner
review. Public methods cannot simulate those missing seams. Native, local and
hosted signing remain supported; hosted setup supplies only minimum required
Stark material through its existing provider path. Legacy non-Rust AWS is
unchanged. No Sam-only B/C hold is assumed.

## Native walk and wire contract

| Family | Existing geometry and page budget | Native output policy |
| --- | --- | --- |
| Fills | Up to 5 /fills pages, then one /orders-history aggregate fallback after an empty discovery DATA stage | Requested limit defaults 100 and clamps 1..500; raw vs kept rows remain distinct |
| OrderHistory | Up to 50 cursor pages | limit None/0 is unbounded; finite limit permits early bounded completion |
| Income | Up to 200 /fills cursor pages; default time range is native preceding 30 days | Preserve realized PnL, absolute commission/rebate, funding IDs, asset and timestamp units |
| OrderByClientId | Up to 50 /orders pages, then 50 /orders-history pages | Found result stops early; definitive miss requires both stages complete |

Declare one serial command Read/Never Plan for the entire native walk, including
fallback or lookup stage transition. The maximum declarations are 6, 50, 200
and 100 physical calls respectively, each with the existing 20,000ms physical
bound and no new retry grant. The Plan deadline is the minimum of its checked
native bound and the exact original caller cap; each activated leg is capped
by it. Native maximum-page declarations reserve existing shared context credits;
capacity refusal is atomic, before JWT/private dispatch. No second Plan at a
stage restart or cap/deadline renewal. If future auth retry needs an additional
attempt, its exact native rule must be declared/reviewed before adoption.

Preserve mainnet/testnet base URLs, GET query parameter and header order,
Authorization/Content-Type/Accept values, market encoding, cursor encoding,
page_size vs fallback limit, bounds and defaults. Keep decimal validation,
side/order mapping, execution quantity vs aggregate filled quantity, commission
sign handling, stable IDs, sort order, de-duplication and final limit shaping.
History never publishes balance, orders, positions, subscription books or frames.

The fallback is successful empty DATA, not a refusal or retry. Lookup open and
history stages use the same owner and completion. Restart clears current cursor,
staged rows and page identity, while retaining accepted rows and the native seen
set. Preserve the native cumulative raw-row counter: an empty later page after
accepted fills finishes that prefix without entering aggregate fallback. No
provider/native failure is reclassified as successful empty discovery.
Unbounded page exhaustion, repeated cursors and malformed rows remain strict;
finite bounded exhaustion retains its native INCOMPLETE policy through one
prepared DATA terminal. An incomplete lookup never becomes a definitive miss.

## Exact authority and terminal cutover

Begin a receipt-bound ReadOnly Preflight at genuine entry. Missing/expired
receipt uses shared rejection without another numeric terminal. Retain it through
initial Session wait, admit the final Plan only in exact resumed custody, then
bind `PlanReadCompletion::from_plan` before selecting any Slot. Preserve native
unknown-market local results before this binding, using genuine Read or retained
ReadOnly-Preflight completion with no fabricated DATA. Keep credentials and
validation precedence; unknown-symbol lookup remains nondefinitive. After binding,
no cloned Plan done/fail or standalone Slot settlement is an escape hatch.

Use `PlanHistoryRows` as the sole accepted-output/terminal owner and native
CursorWalk as geometry only; do not retain numeric CursorHistory completion.
Validate exact Plan/Slot/Context/current Attempt before row parsing, raw-count,
dedup/cursor changes, status handling or staging. Stage only under the exclusive
admission guard, which has no Cx accessor. Prepare native progress/seen storage
before effects, then atomically accept the page and settle its Slot. Commit native
cursor/coverage state consistently with that prepared acceptance; exact ordering
and allocation refusal are source-review gates, not permission for an unguarded
second row authority. Retry/reset discards failed-page stage and identity.

A final DATA page uses guarded `finish_with` before giving up its release anchor;
retire only the unissued suffix and refuse unresolved issued peers. Preserve
lookup JSON and sorted/truncated rows inside the pure encoder. No fallible Store
projection, external finalizer or venue-emitted marker/terminal is added.

Only the latest exact engine-released Network or physical Timeout may recover a
nonempty accepted prefix while the original Plan cap is live. Use the callback-
scoped engine proof and private accepted-Vec witness. Failed-page staged data is
discarded; accepted-but-filtered-empty encoding may retain its witness. Empty or
entirely filtered-before-acceptance prefixes fail strictly. Lookup transport
failure remains strict and cannot claim a definitive absence.

Preparation/provider failure, HTTP/native status, malformed envelope/row/stream,
root expiry and logical deadline never authorize prefix recovery. This tightens
legacy CursorHistory, which currently recovers accepted rows even after several
nontransport failures. Pin each cause independently, document the deliberate
caller delta in CALLERS, and preserve native diagnostic content where eligible.
Released Network/physical Timeout retain canonical typed Read error semantics;
exact provider/pre-release fields are not erased by string reclassification.

## Original compiling assertions and source certification

A new test target drives genuine Engine command receipt, real delegated auth and
native public/account setup. No private state injection, fake identity/Context,
missing method or setup panic is counted as a RED. Existing APIs expose the old
legacy request path, unclaimed DATA acceptance, ignored caller cap, auth-wait
expiry and overbroad prefix recovery. Record each failing family/cause as its own
test; mainnet/testnet negative variants must not hide behind a first failing loop.
Positive controls retain exact wire/financial geometry, accepted-only transport
recovery, empty-prefix strictness, fallback and lookup stage behavior.

The protocol driver may learn Prepare/Bind/Claim for later adoption while running
unchanged legacy source today. A successful Claim-shaped fixture is not proof
of JIT Session authority, a physical timer or a refusal path. After production
cutover certify exact held-provider/Bind/claim expiry, actual physical timeout,
late/stale/foreign/reused Slot/Attempt and same-id generations, auth retirement
and late ACK, original caller cap across stage restart, reservation failure and
zero partial geometry commit, pool capacity recovery, and no Store/frame effects.
Prove Session wait does not mint a child early and that rotated JWT is chosen at
prepare; retain borrowed/owned native/hosted setup and exact failure metadata.

Plan and original REDs are the only work in this phase. Production implementation
and its Cargo http-plans opt-in wait on all actual prerequisites and renewed
native/Plan/history source review. Draft PR and lane handoff follow evidence;
full adoption CI includes family WASM/CPU with unchanged budgets/profiles.


## Original evidence checkpoint

Plan `7aa780e0cc2` preceded the new
`venues/paradex/tests/request_context_private_history.rs` target. The current
production and all existing tests remain byte-exact to actual `77f12408947`;
only the reviewed #2011 dependency is stacked underneath this phase.

Each negative case runs independently. The expected original result is 17
assertion REDs and nine GREEN controls, zero ignored tests:

| Original assertion | Cases | Observed legacy mismatch |
| --- | --- | --- |
| Prepare before private dispatch, four families on each network | 8 | Legacy `Action::Http` emitted |
| Unclaimed DATA cannot finish an empty result | 1 | One numeric success accepted |
| Full shared Read pool denies private walk before dispatch | 1 | No terminal; private HTTP bypasses pool; 256 held genuine peers plus a canonical refusal control establish saturation |
| Configured 1234ms caller cap bounds first page | 1 | HTTP timeout is None |
| Original root equality after accepted DATA is strict | 1 | Accepted prefix succeeds without a marker instead of one Timeout failure |
| JWT wait expires at original receipt equality | 1 | No terminal |
| Late JWT cannot release work for the expired root | 1 | One private history HTTP emitted |
| HTTP 503, native error envelope and malformed stream cannot recover a prefix | 3 | Each emits partial success plus marker instead of one strict failure |

The nine GREEN controls cover exact authenticated native wire and financial
rows on both networks/all four families; accepted-prefix transport recovery;
empty-prefix transport failure; finite-limit early DATA; successful empty
fills discovery and single aggregate page; complete two-stage lookup miss;
strict lookup transport failure; unknown-market local results; and accepted
fills followed by an empty page without aggregate fallback. The existing native
`history` target also passes all 30 tests without changes.

These are compiling native product assertions. No missing method, fake Context,
private auth injection or fixture panic is counted as a RED. The initial draft
of the extra accepted-fills/empty-page control incorrectly expected aggregate
fallback and panicked looking for that route. Source inspection confirmed the
native cumulative counter, and the corrected GREEN now preserves that behavior;
that fixture mistake is not counted as product evidence.

The first failing assertion bounds each RED's evidence: later typed metadata,
no-marker or no-I/O assertions are source-stage gates until the primary mismatch
is fixed. Native wire positives exercise actual delegated-auth startup and
headers; they do not prove future JIT provider custody. Legacy transport-prefix
recovery is a working behavior control, not an engine-released proof witness.
Physical Timeout, held Prepare/Bind/Claim, generation and exact Session/JWT
retirement remain postcutover certification gates. No private production or
inventory adoption is claimed by this checkpoint.
