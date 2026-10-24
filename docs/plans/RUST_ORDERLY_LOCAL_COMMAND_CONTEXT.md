# Orderly initial history refusal custody

## Scope and ownership

Original plan/RED checkpoint based on actual `9d344d541d5068f6d34f43948d6c65773ab10029`.
Branch: `feat/rust-orderly-local-command-context`; owner: codex-5c57jl.
Hcmq3f released Orderly source/fixture ownership. This slice excludes pending
combo-5b Orderly lib.rs, sign.rs and Cargo.toml, all credential selection/nonce
work, physical requests, callbacks, pagination, reducers, trading and sockets.
No new core API is required. After hcmq3f's bounded plan/native review CLEAR,
the overseer authorized production plus a draft PR and CI.

## Exact next slice

Replace only three initial numeric failure calls in `src/history.rs`:

| Native initial branch | Fingerprint prefix | Occurrences |
| --- | --- | --- |
| Unsupported OrderByClientId | 65c654c353026 | 1 |
| Unsupported income kind | 69026daad855 | 1 |
| Initial argument/credential/symbol validation | 0451c2999329 | 1 |

The current canonical request inventory is 28 fingerprints / 29 occurrences for
Orderly. This slice removes 3 / 3, yielding 25 / 26; history.rs alone goes 9 / 9
to 6 / 6. Counts are lexical, not claims of physical lifecycle adoption. Maps
and checker source remain unchanged during this plan/RED checkpoint.

The validation site retains its native precedence: range/limit, timestamp,
canonical positive uint64 order ID, private credentials, then symbol lookup.
Income accepts FUNDING and FUNDING_FEE case-insensitively. Unsupported client
lookup and income-kind errors retain their exact native messages and metadata.

## Implementation contract for later approval

Use genuine current command-entry custody and Operation::Read. A small helper
local to history.rs may reject an unstamped/expired genuine entry via the shared
reject_unstamped_command/reject_expired_command API, otherwise admit one Read
Context and fail_context with the native error. Admission failure already emits
the shared terminal: never fail again. No copied command IDs/issuer checks,
borrowed entry, synthetic callback, request, signing work or new timer.
The helper must never run on an accepted request, delayed funding timer,
continuation, page callback or a queued trading command. It may use the existing
20-second local leg policy capped by the original caller receipt. A local read
refusal carries no write outcome.

At a finite valid receipt, retain one exact native failure, no transport actions
and no Store changes. At a nonfinite receipt the shared typed InvalidInput
rejection takes precedence over the native local string, once and without IO.
This is the intended custody correction exposed by the original REDs.
No selection or signing setup change is part of this slice, even after 5b.

## Original controls

Add a dedicated tests/local_history_context.rs on unchanged production.
Three separate nonfinite genuine Engine command-entry controls target the three
sites. Finite working controls pin exact native strings for those sites and the
validation variants; they inspect the complete action batch and Store state.
A valid unsigned PublicTrades request proves the accepted history path and wire
are still reachable without private credentials. Do not claim deeper shared
Context retirement/duplicate rejection coverage from native legacy execution.

Before a later cutover, preserve these original assertions, add actual shared
admission/capacity and duplicate-current-command controls, verify exact source
and inventory preservation, and obtain custody/native-caller reviews. Any 5b
composition must preserve its incoming signer source; do not resolve selection
or nonce conflicts through this slice.

## Verification

Only scoped local Cargo is allowed, CARGO_BUILD_JOBS=4, one build at a time.
Run the new target on unchanged source, record assertion REDs separately from
working controls, and keep logs outside Git. CI owns broad workspace gates.
No PR, ready, merge or production claim follows from an original RED run.

## Original execution receipt

Plan committed at 35eda6420f7, original test source at 0d59abe4901, and a fixture
String-vs-Option compile correction at 7963ccf5d9b. The first attempt did not
execute tests; it is not a RED receipt. Corrected unchanged-production run:

`cargo test -p safe-cex-venue-orderly --test local_history_context`

12 executed: **3 intended assertion RED / 9 GREEN / 0 ignored**. Each
unstamped lookup/income/validation control reaches the typed-kind assertion:
native kind None, expected Some(InvalidInput). Every full-batch/Store/Read-outcome
assertion before it passed. All nine native controls passed, including exact
messages and validation precedence, and the accepted unsigned public-history
GET `/v1/public/market_trades?symbol=PERP_BTC_USDC&limit=2`.
Log: `/tmp/codex-orderly-local-history-original.log` (not committed).
Only plan, index and new test differ from actual9d; production, prior tests,
Cargo, all inventories and checker algorithms remain unchanged.

## Production adoption

The three initial failure calls now use reject_local_history, which consumes
only a genuine Read entry and finalizes its admitted Context. None/Expired
receipts use the shared initial rejection helpers; refusal emits no second
terminal. Existing 12 original controls are unchanged and now pass.
New local_history_admission controls allocate 256 genuine Background peer roots
in a preceding no-ID callback. Each branch proves one shared capacity rejection
with Read outcome None, releases the peers, then proves native-message recovery.
An actually retained command root rejects all three duplicate queries without
finishing it; its own terminal then permits 810 fixed-clock local queries,
followed by allocation of all 256 peer slots to prove credits were released.
The fixture's generic Capacity error expectation was corrected to the native
Read projection (no NotSent); runtime error projection was not changed.

Physical history send/parse/page/retirement, native validation and all incoming
5b source remain outside this delta. The inventory removes exactly the three
listed fingerprints; no new entries, bootstrap rows or checker changes.

## Integrated local receipt

Production 0d36c12cd42 was plain-merged with actual 10d01de5279 at 818a59570f4.
The only manual resolution was CALLERS, authorized by the train owner: entire
incoming bytes plus the exact own suffix once. All Orderly/engine/host/crypto
source and tests are byte-exact to the locally tested production pin. All other
incoming tracked paths and all foreign manifest entries are preserved.

Executed: original local_history_context 12 PASS, new local_history_admission
4 PASS, native history 25 PASS; zero ignored. Scoped strict Clippy and fmt PASS.
The initial admission fixture compile attempt omitted mandatory Venue forwarding
methods; it ran no tests and was corrected. Three capacity expectations then
exposed the fixture's generic NotSent projection; they now pin the correct Read
outcome None without changing production. The initial 9d ban run failed solely
on the foreign landed BingX shrink; after the actual merge full bans PASS,
including 1 signing + 15 request + 8 socket negative controls. No exemptions or
predicate changes. Draft publication and full Rust CI proceed in parallel with
production reviews; no READY/landing or broad CI result is claimed here.
