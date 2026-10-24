# Bitrue local one-way position-mode success

Owner: codex-9czdjh. Base actual `5f86ae51717`; #1989 shared LocalSetting
is actual `ce278e0b4a9`. New venue-only slice, separate from signer work.

## Native contract and direct adoption

In trading.rs::trade_command, SetPositionMode(false) calls Store::set_hedged(false)
and returns `[]` before credential validation or remote work. This holds for
keyless/public-only/keyed setup and any optional symbol, including unknown.
SetPositionMode(true) keeps its existing Unsupported/NotSent refusal; no remote
request exists for either variant. Keep the initial acknowledged-retention cleanup
and every other trade/refusal/signer/queue helper unchanged.

Replace only the false branch with
`cx.done_local_setting("[]".into(), |store| store.set_hedged(false))`, then
return true on Ok, Rejected or Revoked. No numeric fallback, local admission helper,
Context, HTTP, provider, timer, journal, Plan or new owner registry. Shared #1989
validates the genuine Setting receipt and command capacity before projection and
seals one terminal. Expired/sub-ms waits and capacity now refuse before changing
hedged; existing duplicate diagnostics/owners are retained.

## Exact remaining inventory and file map

| Source | Fingerprints | Occurrences |
| --- | ---: | ---: |
| `venues/bitrue/src/account.rs` | 8 | 8 |
| `venues/bitrue/src/lib.rs` | 2 | 11 |
| `venues/bitrue/src/secondary_tests.rs` | 2 | 2 |
| `venues/bitrue/src/trading.rs` | 6 | 9 |

Remove only trading.rs fingerprint
`09d284b47513158ba424efe6b85ef618c3f2649072f4211be5a3ec043ecc0e88: 1`.
Venue 30 -> 29 occurrences, 18 -> 17 fingerprints; trading 9 -> 8 occurrences,
6 -> 5 fingerprints. The full original map stays RUST_BITRUE_REQUEST_CONTEXT.md;
all remaining signed/private sites stay blocked on the signer and shared stamp-floor
migration. No scanner match is renamed or exempted.

Fresh open-PR file map checked before edits: no trading.rs/test collision after
actual #1993. No signer source exists on our separate plan/RED-only branch.

## Plan, originals and verification

1. Commit/review this plan against actual native source and shared API.
2. Commit compiling original assertion REDs for genuine command-owner capacity
   (256 retained command roots issued in separate real callbacks) and expired/
   sub-ms receipts. Working controls: exact pre-credential []/hedged false,
   unchanged true Unsupported branch, original duplicate owner/diagnostic and
   256 Background Contexts allowing Context-free success. Native positive matrix
   covers public-only/keyless/keyed and unknown symbol. Current Engine::handle
   supplies the default receipt; missing-receipt defense is shared-engine coverage.
3. Direct adoption and exact one-row prune; identical tests turn GREEN. Append
   CALLERS/AUDITS/QUALITY and record executed evidence. Run scoped Bitrue tests,
   optional-feature-off control, strict Clippy/fmt and both ratchets; Binance
   control already executed on the same shared engine source for paired #2017.
4. Independent source/adversarial, caller and full-map reviews; draft PR/full CI.
   Merge only in an explicit window. Architecture gate: No venue-local algorithm;
   only native Store projection/result is passed to the shared LocalSetting seam.

## Executed originals

Seven compiling native tests against unchanged source produced two assertion
REDs (command-owner capacity and sub-ms receipt), plus five working controls.
The duplicate is an existing Engine diagnostic/owner-preservation control,
not a migration RED. No shared API compilation failure is counted as evidence.
The same assertions are retained after the single direct source substitution.
