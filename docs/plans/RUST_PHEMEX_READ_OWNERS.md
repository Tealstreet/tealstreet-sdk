# Phemex identity and recurring account read owners

## Scope and hold

Owner codex-5c57jl; branch `feat/rust-phemex-read-owners`, exact native base
`17e93c3038ef98e0f3d0d4580527cfd37660bcaf`. This checkpoint is plan plus
original-source assertion REDs only. Production waits for actual #1974 landing,
then genuine integration, reviewed composition and fresh CI. Frozen #1970/#1974
and the MEXC lane remain unchanged. No shared-core prerequisite is proposed.

Two separately reviewable slices: (1) FetchAccount identity, (2) recurring account
currency reads and uncertain-order reconciliation. ih35js cleared identity,
account and lib regions for this phase and retains history/income issuance,
Pending branches and retirement. Preserve actual unsigned PublicTrades #1969.
The phase edits only this plan/index and new dedicated tests. No source, Cargo,
existing fixture, checker or inventory changes. No history/income adoption.

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
OrdersRead; it is not a borrowed command entry. Before streamed account_item,
response parsing/publication, global rate-limit mutation or error cleanup,
validate_background_read_http_delivery against that exact read. Native cycle,
write/slice revisions, SnapshotBuilder coverage, per-row versions/holds and
currency validation remain additional predicates; shared Context does not replace
them. A successful currency can publish while its peer fails. Preserve aggregate
pending bits, 20s cycle cleanup, failure/backoff and 2s policy-scaled polling.
The native aggregate cleanup is not a renewable transport deadline: both read
bounds derive from the original cycle start, including partial admission failure.
No duplicate failure may clear a newer cycle's bit or schedule a second poll.
At equality, deny streamed/response effects even without TimerFired first.

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
Run focused tests and strict Clippy locally; full native and CPU/memory/size gates
in CI. Request uufub4 API/custody and ih35js native/REST/financial reviews, plus
signer-owner composition review. Prune only actually retired own inventory sites
with an exact whole-map preservation check after actual integration; no bootstrap
or predicate edit. Architecture: adoption of shared SelectedRead/lifecycle, not
venue ownership of credentials, transport, deadlines or completion authority.
