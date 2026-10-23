# WOO synchronous history refusal custody

Plan first, 2026-10-24. Base landing
`3ba3f57c9ef690d67e15f299dd0a36fb9864b7f4`; branch
`feat/rust-woo-local-history-context`. This slice removes exactly three legacy
terminal sites in `venues/woo/src/history.rs`. It does not adopt HTTP history.

## Inventory and ownership

The complete remaining WOO inventory at this base has 34 fingerprints and 37
occurrences: account 5/5, history 5/5, identity 2/2, lib 12/13, secondary tests
3/3 and trading 7/9. The broader operation map remains in
`RUST_WOO_REQUEST_CONTEXT.md`. This slice changes history
from 5/5 to 2/2 and WOO from 34/37 to 31/34. Other files and venues retain every
inventory entry.

| History fingerprint | Native branch | Destination |
| --- | --- | --- |
| `e10f2a544139be7b8a9e10a239fce00e30d8b13669f3f4b176743ab1bf2a34c5` | Unsupported remote client order lookup | Initial Read admission and `fail_context` |
| `aabc8213282a8979a24e0eecad23e21d2ec8e5bf00c6e0226e42b48cd609cb63` | Invalid symbol, explicit range or limit | Initial Read admission and `fail_context` |
| `4bba92ad9c2f3e562fd37ab07235ee0ca41b7fd4902d0e07de5b932da29d510e` | Derived end before start | Initial Read admission and `fail_context` |
| `c7840a8b417295613f5ba4cb19cd3ee6243e6810e609f4408e116d2f401f8668` | Unsigned streamed public trades | Retain; HTTP history adoption is separate |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | Signed page setup failure | Retain; it can follow earlier accepted pages |

The fresh open-PR audit found #1930 owns WOO socket lib/account/codec/tests
and #1788 owns account/trading snapshots. Neither owns history.rs. The socket
owner has been notified of this history-only slice. HMAC owner claude-4yjlw0
cleared local refusals and reconfirmed private paths wait for the shared managed
read helper and recipe. Aster is held by #1916. MEXC unsigned background delivery
waits for #1856; ownership of that shared prerequisite is separate from this
venue adoption. Crypto.com remains parked for actual #1923. These gates prevent
claiming a larger semantic shrink through renames or a venue lifecycle copy.

## Implementation contract

Only the three existing synchronous pre-I/O branches acquire custody. A small
history-only native error projection calls genuine `admit_http_command` with
Read, credential None, 20000 ms, no pacing and RetryPolicy::Never. On success it
calls `fail_context` with the exact native message. Rejected admission has
already published its single typed failure; Revoked has no terminal rights.
Neither branch falls back to numeric completion or creates another owner.

Preserve validation precedence: unsupported lookup first; Fills alias
normalization and grouped symbol/explicit-range/limit validation next; derived
`to.or(from.map(|_| cx.now))` validation last. Do not broaden the native
validation to other queries or valid continuation paths. A finite valid engine
clock keeps the native error and metadata. Invalid admission time is InvalidInput
and shared capacity refusal is Internal, with no financial outcome. The refusal
uses no credential and produces no HTTP, signing, timer, history notice or Store
change. Immediate settlement permits reuse of the command ID.

Valid public/private history keeps existing dispatch, credentials, query codec,
CursorHistory/Completion, page recovery, range windows, limits, pacing and
callbacks. A later page must not restart command entry. No lib/account/Cargo,
shared engine or socket changes belong in this PR.

## Original REDs and compatibility controls

Commit the new dedicated real Engine<Woo> test module before production edits.
Three independent assertion REDs exercise each selected branch at a non-finite
engine clock: unsupported lookup, grouped invalid input and the derived range
branch. The derived-range RED uses negative infinity, so the original native
`from > now` branch is actually reached; NaN or positive infinity would not
prove that branch. Expect typed engine admission refusal rather than the
existing untyped native terminal. Compilation errors are not REDs.

Finite-clock controls cover exact native messages and metadata, grouped symbol/
explicit-range/limit cases, lookup precedence, Fills alias behavior, repeated
settlement and same-ID reuse, and no I/O or Store effects. Preserve valid public
GET and valid private credential-refusal behavior as working controls; neither
continuation is converted. Shared capacity/revoked admission mechanics remain
covered by the genuine engine entry suite, rather than manufacturing raw legacy
requests as managed owners in the venue fixture.

After the narrow cutover, all original assertions must pass unchanged. Prune
only the three listed fingerprints and verify both request-context checks. Run
focused WOO controls, existing WOO and Binance suites, strict WOO Clippy and fmt;
CI owns WASM, replay and size gates. Add caller-visible admission semantics to
CALLERS and accurate audit/quality receipts. Independent source/adversarial,
caller and full-map reviews must be clean. Architecture gate: use the existing
engine custody choke point; add no venue retry/authority mechanism. Merge only
with a fresh guard and an explicit overseer window.

## Original checkpoint and narrow implementation

Plan `e18f8db92da` precedes original test commit `eec6e17ee9d`. The original
untouched WOO source produces exactly three InvalidInput-kind assertion failures
and four compatibility passes, zero ignored. The implementation changes only
the three selected terminals and a history-only native error projection onto
existing shared admission/completion. All seven original tests then pass
unchanged. No compilation/fixture failures are counted as REDs. Full focused
gates, independent implementation review and exact-head CI are still pending.

## Integrated verification

Integration `3dae752db98` merges actual landing `c473342d483`. Source merges
cleanly; shared documentation retains incoming content first and own sections
once. An inventory check before integration correctly refused the older branch
manifest because incoming Hyperliquid history entries had been pruned. After
integration, all incoming prunes remain and both checks pass with 15 controls.

WOO plus Binance native verification passes 496 tests across 39 targets, no
failures or ignored tests. Strict all-target WOO Clippy passes. The original
seven-control file remains byte-exact to the test-only checkpoint. Independent
source/adversarial, caller and full-map implementation reviews report no source
or boundary defect; the caller reviewer requested the updated README status.
CI, including replay and family size, remains pending. No merge is authorized.
