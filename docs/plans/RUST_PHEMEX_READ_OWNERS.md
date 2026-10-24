# Phemex identity and recurring account read owners

## Scope and hold

Owner codex-5c57jl; branch `feat/rust-phemex-read-owners`, exact native base
`17e93c3038ef98e0f3d0d4580527cfd37660bcaf`. The original checkpoint was plan plus
assertion REDs. Production now integrates actual #1974 and the complete combo-5b
landing `219d4bc6725`; bounded source reviews and fresh CI remain pending.
No shared-core prerequisite is proposed.

Two separately reviewable slices: (1) FetchAccount identity, (2) recurring account
currency reads and uncertain-order reconciliation. ih35js cleared identity,
account and lib regions for this phase and retains history/income issuance,
Pending branches and retirement. Preserve actual unsigned PublicTrades #1969.
The original phase edited only this plan/index and new dedicated tests. The
implementation below adopts the three planned read owners. No history/income
adoption or checker change is included.

## Native contract and destination

| Site | Native contract | Shared owner |
| --- | --- | --- |
| identity.rs fetch_identity | One GET /g-accounts/accountPositions?currency=USDT; Secondary if configured, else Primary; no account Store publication | SelectedRead command_root from genuine FetchAccount entry, one physical ReadLeg bounded by caller root and 20s leg |
| account.rs account_cycle | Independent streamed USDT and USDC GETs to accountPositions; ordinary read selection; partial currency commits allowed | Two SelectedRead Background reads, each bounded by the same native cycle start +20s; no artificial command/join/Plan |
| account.rs orders_reconcile | GET /g-orders/activeList?symbol=encoded(native symbol), explicitly Primary; one reading per symbol, native generation/not_before/placing guards | SelectedRead Background with PrimaryRead reconciliation reason and 20s physical bound; no write authority |

Use #1974's trusted opaque credential slots and existing Phemex dispatch recipe;
never reconstruct an identity from raw options or hash keys. Preserve primary
routing for activeList and ordinary selected egress for identity/currency reads.
No new account-evidence assertion: identity comes only from the validated native
userID response, retaining exact large integer/text semantics. Public-only,
missing/invalid credential refusals stay correlated and pre-I/O. Preserve the
existing diagnostic then one typed terminal; numeric command completion retires.

## Admission, delivery and cleanup

Identity owns a non-cloneable caller ReadRoot and its exact leg. Validate root/leg
delivery before common 429 pause, decoding or terminal side effects. Use Context
terminals once, preserving parser/error semantics and unchanged account state.
A physical leg cannot extend the caller deadline through signing, routing,
Bind/reprepare, Claim or response. Refused admission creates no transport owner.

Background Pending state carries its own AdmittedRead plus native Request or
OrdersRead; it is not a borrowed command entry. Live DATA delivery (streamed
account_item, response parsing/publication and global rate-limit mutation) must
pass validate_background_read_http_delivery against that exact read. Failure
cleanup has a separate path: live delivery validation may return TimedOut for an
exact physical timeout, and preparation/provider/queue refusal may have no DATA
scope at all. Never return solely on that validation failure and strand custody.
Native cycle,
write/slice revisions, SnapshotBuilder coverage, per-row versions/holds and
currency validation remain additional predicates; shared Context does not replace
them. A successful currency can publish while its peer fails. Preserve aggregate
pending bits, 20s cycle cleanup, failure/backoff and 2s policy-scaled polling.
The native aggregate cleanup is not a renewable transport deadline: both read
bounds derive from the original cycle start, including partial admission failure.
No duplicate failure may clear a newer cycle's bit or schedule a second poll.
At equality, deny streamed/response effects even without TimerFired first.

Failure-only cleanup consumes the exact returned Pending/read handle, attempts
finish_http_context to retire that handle (expired current handles may retire),
and independently checks native cycle+currency or symbol+active-read custody.
It releases matching pending/reading ownership and schedules native backoff once;
no row/Store publication or global pause mutation is allowed. Retired/duplicate
old failure cannot clear a newer read, cycle bit or owed reconciliation generation.
A rejected finish call is not permission to clear arbitrary work; use only the
matching Pending's native custody. For queue refusal, the Result's returned
Pending is the sole cleanup input. For admission refusal before a Pending/read
exists, undo only the native slot reserved by that admission attempt. Do not
invent a live Context or call a DATA validator to obtain failure cleanup rights.
Identity timeout/preparation/provider failure likewise uses its owned root/leg
terminal path, not a blanket live-DATA early return.

Reconciliation preserves the native signed-write-expiry +5s not_before fence,
paused_until, placing guard, one-read flag, generation/lost-ID/cancel-late logic,
10002/null empty-list contract, incomplete-row limit 3 and native jittered retry.
A failed physical read settles/retires its own Context before native retry issues
a fresh Background read; never renew/retry a command or infer a write outcome.
Do not blanket-discard older-generation reads: native reads can recover an older
uncertain placement while a newer generation remains owed. Preserve native
financial/version and per-symbol cleanup rules, and test that an old completion
cannot settle the newer owed read. No local queue, new timer or parallel reducer.
Admission/sign/setup/expiry failures must release only matching native reading
custody so retry remains live, while retaining exact typed error evidence.

## Implementation and proof gates

Commit plan before original tests. Original REDs use the actual Engine and valid
native requests/payloads, not nonexistent APIs or source-string assertions:
managed Prepare required for each of the three issuance families; identity and
account equality expiry without timer delivery; stale streamed row/429 fencing.
Native positives pin exact UID/no Store changes, selection/routing, both currency
publication, native deadline retirement and reconciliation reveal/retry behavior.
Preserve original RED commits; when #1974 lands, adapt constructors to trusted
signer setup and transport to real Prepare/Bind/Claim without weakening financial
or wire assertions. Raw baseline Http handoff is not physical managed Claim proof.

Before production certification add parked-provider, delayed route/Claim,
partial-admission pressure, stream-before-expiry then response-at-expiry,
cycle/write/slice race, failure-first/both currency orders, stale 429, per-symbol
reconciliation generations, malformed rows/10002 and exact typed refusal controls.
The following ACTUAL managed recovery controls are mandatory source/certification
gates after actual #1974; native HttpFailed injection is not their substitute:

| Failure input | Required recovery proof |
| --- | --- |
| Exact physical Timeout at deadline, including no prior timer ingress | Failure callback receives the expired handle; both currency-leg orders release matching custody and poll again; OrdersRead releases reading and native backoff issues a fresh read; one cleanup, no data/pause effects |
| Real preparation refusal through the host handshake | Returned failure Pending is cleaned once even without DATA delivery; sibling currency can commit; native retry can admit after capacity/setup recovers |
| Parked provider returns a typed refusal or expires | Same exact ownership cleanup, zero physical send/Store mutation, late provider completion inert; recovered provider allows the native next attempt |
| Genuine shared queue capacity refusal returning Pending | Reserved native bit/reading slot is released once without a fake Context or callback; free capacity, fire native retry/poll and prove both currency and per-symbol reads issue |
| Old timeout/provider/queue completion after a replacement owns work | Old failure cannot clear newer pending/reading or pause/publish; newer response still commits and cleanup credits return once |

Keep these failure controls separate from live DATA expiry/429 REDs. Their final
receipt must include real Prepare/Bind/Claim/provider/timeout transitions, not
only assertions that a Prepare was expected on the original legacy source.
Run focused tests and strict Clippy locally; full native and CPU/memory/size gates
in CI. Request uufub4 API/custody and ih35js native/REST/financial reviews, plus
signer-owner composition review. Prune only actually retired own inventory sites
with an exact whole-map preservation check after actual integration; no bootstrap
or predicate edit. Architecture: adoption of shared SelectedRead/lifecycle, not
venue ownership of credentials, transport, deadlines or completion authority.

## Exact inventory boundary and original evidence

At native 17e93, identity.rs has three one-occurrence legacy fingerprints:
`0d4f7749bb0b24c084131b4f15d0e612c9b03378700f1ea465dfa856113e17e9`
(issuance), `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548`
(done), `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`
(fail). Account issuance is
`35203e2192556d58fa9706bd22bddc7b92206b18106905b5af0b39aefd365d1c`
(currency cycle) and
`db7b6dc88f90361d5d57c041003af076c94d051c673d00cec046198eb9441f98`
(order reconciliation), one occurrence each. The third account fingerprint
`ee694d9fd31f8bcf8dd1202488b242081189214dd143f90fc6bb10d6fd6fe886`
is a retained native diagnostic, not a transport cutover. No map is edited now.
Recompute against actual #1974; generic lib request/failure paths stay while
foreign pending variants still consume them, including ih35js history/income.

Plan-first commit: `bd6eb09149a`. New dedicated target:
`request_context_read_owners.rs`, using only native public Engine ingress.

| Original assertion REDs (8) | Working native controls (8) |
| --- | --- |
| Identity managed Prepare, expiry at 20s without timer | Exact large UID with Secondary/no Store mutation/duplicate inert; Primary fallback plus malformed identity terminal; missing/public-only pre-I/O refusal |
| Two Background currency Prepares; response at 20s; live streamed rows ending at 20s; retired 429 must not pause next cycle | Secondary selection and normal route; both currency arrival orders/partial publication; actual native deadline retires late currency and next poll issues both reads |
| Primary reconciliation Prepare; expired reply cannot reveal an order | Actual uncertain write -> native 65s fence -> Primary activeList -> live revealed order; 10002 empty and network retry with no invented orders |

Executed target result: 8 assertion failures / 8 passes / 0 ignored. Each case is
individually executable; compile/setup failures do not count. Initial fixture
compile lacked a SocketId import; fixed before this receipt. The stale-429 RED
has a working timer-retirement/next-poll control, and every expired financial
payload has an identical live financial positive. The native account 20s timer
is deliberately absent from equality REDs; expiry must be a delivery predicate.
No assertion claims an actual managed provider/Claim happened on legacy source.

Commands from sdk-rs (historical execution used jobs 4; subsequent shared-Mac runs use jobs 2):
`cargo test -p safe-cex-venue-phemex --test request_context_read_owners`
(expected assertion failure), and
`cargo clippy -p safe-cex-venue-phemex --test request_context_read_owners -- -D warnings`
(pass). Scoped rustfmt passes. Independent review and production/CI certification
remain separate; these original REDs are intentionally not a green PR.

Scope preservation check passes: only plan/index/new dedicated test differ from
17e93. Secret scan of the new fixture and check-signing.py pass. The regular
check-request-context.py exits 1 against the advanced shared remote base
59f50e4f37b (11 foreign Bitrue/Kraken/KuCoin/Polymarket provenance failures).
An independent scan of current production sites equals the exact requested
17e93 manifest, and all production/map/checker bytes are unchanged. This is a
base-drift limitation, not a regular ratchet PASS; no override was used. Recheck
normally after actual #1974/current-tip integration before any production PR.

## Failure-custody review correction

uufub4 and ih35js found that live DATA validation before error cleanup could
strand pending bits/OrdersRead.reading at expiry. The admission section now
separates DATA/stream/429 permission from exact failure-only retirement and native
cleanup. finish_http_context explicitly permits an expired current handle to
retire without granting sends; no copied issuer/attempt check is introduced.

Three appended original-source controls execute: identity leg expiry requires
one typed Timeout terminal (RED), expired reconciliation retains native retry
and a fresh Primary read (RED), and both currency failures in either order free
pending custody and produce exactly one poll (GREEN). The first two present a
real response at the expiry boundary; on native legacy source they do not claim
an actual managed Timeout callback. The both-failure control injects ordinary
HttpFailed and is likewise not a preparation/provider/queue refusal receipt.
Actual managed failure transitions remain the explicit matrix gate above.

Expanded result: 19 executed, 10 named assertion RED / 9 GREEN / 0 ignored.
All original 16 test/helper bodies are byte-exact to 8ffc9c0d73a; only three
controls were appended. Initial new typed-error assertion required Some(Timeout)
for the optional ErrorKind field; corrected before the recorded execution.
Scoped strict Clippy, rustfmt, diff whitespace and fixture secret scan pass.
Production, existing fixtures, Cargo and inventories/checkers remain base-exact.
Independent uuf original-8ffc archive receipt: 8 RED/8 GREEN, no setup failures;
that receipt does not claim execution of these three added controls.

## Scratch integration against frozen signer/socket dependency

Prepared locally on `feat/rust-phemex-read-preintegration` from requested current
landing `77f12408947`, with plain merges of original plan/control head
`71bca31bffa` and frozen #1974 `8ddc3880eff`. The dependency merge is
`d64ce6cb0e8`; neither frozen dependency nor original RED history was rewritten.
Only README, AUDITS, CALLERS and QUALITY conflicted. Their resolution preserves
current landing content followed by the dependency suffix once; all non-doc
blobs equal Git's automatic merge. No source ownership conflict was found.

The new target now uses #1974's existing trusted fixture setup for its three
constructors. All nineteen test bodies/assertions are retained except those
constructor expressions. Exact executed result remains ten intended assertion
REDs, nine working controls, zero ignored, with no compile/setup failure. Scoped
strict Clippy passes. This is dependency compatibility evidence, not managed
read adoption: production, inventories and the history/income/PublicTrades paths
are unchanged. Actual #1974 integration and the managed failure matrix above
remain required before production certification. This scratch branch is local;
no production PR, push or landing is claimed.


## Production on actual combo-5b

Plain integration `f2d0a6b37f5` has actual `219d4bc6725` as its second parent;
actual #1974 `c80cf9cecbd` is also an ancestor. Only shared docs required manual
resolution, with incoming content retained before the exact own suffix once.

Identity now retains a SelectedRead command root and one 20s ReadLeg. Currency
and reconciliation requests retain independently admitted Background reads plus
the native cycle/currency or symbol/read custody. Every item and response checks
live DATA authority before parsing, publication or common 429 effects. Failure
cleanup retires its exact handle before native cleanup and does not require a
live DATA scope. Native 20s cycle cleanup also retires both outstanding handles.

Immutable opaque slots are assembled once from the existing constructor inputs;
all three owners use the canonical SelectedRead selector. The existing secondary
field remains for held native history/income callers. No raw key is introduced,
no credential-selection site or checker predicate changes, and no fallback
sender is added. Existing unsigned PublicTrades, managed candles, private login,
accepted-login AOP/ping order, snapshot proof and financial reducers are retained.

Against actual 219d, the request map removes only the five fingerprints named
above: family **33F/36O -> 28F/31O**. The account diagnostic remains. History,
income and generic legacy requests remain genuinely used; their entries are not
pruned by this slice.

Immediate native fixture collectors now drive real Prepare/Bind/Claim at their
most recent engine ingress time. Boundary/parked-provider controls retain raw
Engine access. Two old Primary route assertions now inspect the actual managed
Prepare intent before releasing the request: managed routing intent lives in the
Prepare envelope, rather than the legacy HttpRequest.route field. Exact HMAC,
URL/body/header, financial and outcome assertions remain in place.

The original nineteen controls reach their final assertions on managed transport.
Additive controls exercise preparation refusal, parked-provider refusal/expiry,
real signing-queue pressure, partial Context pressure, stale host/provider events,
native retry/poll recovery and repeated credit reuse. Queue peers are real signed
requests under a trusted paused provider, not fabricated financial or read state.
Native source-stage results and independent reviews are recorded separately from
CI; no broad workspace or all-feature sweep is run locally.


Local source checkpoint: all **286 Phemex tests pass, zero ignored**, including
27 dedicated read-owner controls. Scoped all-target Clippy with `-D warnings`,
rustfmt and the regular bans/ratchets passed on actual219d with jobs4, one Cargo
process at a time. A subsequent checker rerun after the remote advanced reports
foreign GRVT account inventory growth against origin; own source/map matches
actual219d except the five declared Phemex removals. No override was used;
current-landing integration must recheck this before publication.
These are local source results; independent source reviews and fresh PR CI
remain pending. The retired generic-request Orders selection arm was removed;
History lookup selection and its native sender remain unchanged.
