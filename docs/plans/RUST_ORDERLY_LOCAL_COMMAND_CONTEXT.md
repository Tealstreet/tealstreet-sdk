# Orderly initial history refusal custody

## Scope and ownership

Plan and original REDs only, based on actual `9d344d541d5068f6d34f43948d6c65773ab10029`.
Branch: `feat/rust-orderly-local-command-context`; owner: codex-5c57jl.
Hcmq3f released Orderly source/fixture ownership. This slice excludes pending
combo-5b Orderly lib.rs, sign.rs and Cargo.toml, all credential selection/nonce
work, physical requests, callbacks, pagination, reducers, trading and sockets.
No new core API is required. Production is not authorized by this checkpoint.

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
