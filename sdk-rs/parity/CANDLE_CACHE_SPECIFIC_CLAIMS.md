# Candle-cache specific claims

Assignment: codex-sup640 rows 69 and 74–80 in the exact 80-row ledger.
Baseline: master `26621790a0`. The separate `CANDLE_CACHE_FIX_AUDIT.md` (#1129)
records related 4ri6pb rows; this cohort does not count that work a second time.

| Assigned row | Disposition | Bounded evidence / authority |
|---|---|---|
| 69: IndexedDB `open` | OPEN, Wasm | No browser-open equivalence claim is made here. #1129's actual IndexedDB persistence observations do not prove this open-error guard. Browser-worker Rust adoption remains a separate product decision. |
| 74: `cleanCandles` | FIX | The former NaN-last correction changes returned rows/order. Remove the old round-3 authorization and use the existing canonical V8 TimSort with subtraction/NaN-as-equality. First-timestamp deduplication remains unchanged. |
| 75: `slotSecondsFor` | ACCEPT, typed interval boundary | #1129's actual cache/KV source/native paths cover all 27 authored intervals under both modes. The external union rejects `1W`, `W`, `M`, `bad`; no source-helper alias equivalence is claimed. The existing source `1W` missing-duration bug remains declared. |
| 76–77: cached lookup / `fetch` | ACCEPT, explicit storage bounds | Preserve the existing 16,384-slot bound and zero-size uncached behavior. #1129 measures zero/oversized refusal before backend GET and normal 100-slot real cache reuse; this is an explicit typed storage boundary, not a silent fallback or claim that arbitrary TS chunk sizes work. The sort result defect in `fetch` is fixed separately in row 74. |
| 78: `getChunkInfo` | ACCEPT, unobservable pre-epoch exchange data | Reuse #1129's approved pre-1970 ruling: no exchange has such candle data. The typed Euclidean offset deliberately differs from JS negative array properties. Keep finite/index guards and the existing source negative-property and native slot-99 persistence observations. This authorizes no general source-bug correction. |
| 79: `queuedHydrate` | ACCEPT, **diagnostic log only** | Maintained TS and real native cache callers resolve first/queued requests with unchanged fallback candles and measured settlement order after early or late backend failure. Capacity is released. Native reports the exact typed backend error cause; TS swallows it. No caller rejection or result loss is accepted. |
| 80: KV `persistChunks` | ACCEPT, same pre-epoch boundary | #1129's real KV persist/reopen proof retains slot-99 native fields, while actual TS drops a negative array property. The same previously approved pre-1970 exchange-data ruling applies. |

## Sorting proof

Committed baseline RED: `6191178407`, with the public cache fetch assertion
failing `CLEAN-CANDLES real fetch source permutation 1`. The TypeScript oracle
loads the actual maintained class and `cleanCandles`; only terminal fetch data
is scripted. There is no reconstructed source sorting algorithm.

133 profiles exercise empty/singleton data, lengths around minrun boundaries,
longer/galloping-sized inputs, numeric duplicates, NaN, both infinities and
signed zero. Every timestamp/OHLC/volume field is compared as binary64 bits,
through both the public helper and constructed cache `fetch`. Two existing
regressions retain 2,000 seeded mixed permutations plus two explicit duplicate
cases, replacing obsolete total-order assertions with exact source field/order
observations. NaN deduplication keeps the first row and zero deduplication keeps
the original signed-zero timestamp.

The canonical sorter pins inconsistent-comparator permutations to the measured
Node/V8 version. Stable consistent-comparator behavior is portable; no universal
engine-independent NaN permutation is claimed.

## Hydration proof

`hydration-error-oracle.mjs` executes the actual TS class with terminal backend
failures before and after a controlled timeout. The native
`hydration_errors_are_diagnosed_before_and_after_timeout_without_losing_fallback`
uses the actual cache and typed backend with the same two caller requests.
Both return their unchanged network candles and settle first then queued.
Native diagnostics retain the exact key and backend error string for both
requests; no broader raw-JS Error identity is claimed by this typed backend API.
The shared known-residuals ledger records the overseer's diagnostic-log-only
ruling. Existing timeout retry-suppression behavior remains unchanged.

Run the scoped `fix_audit` target, named cleaning/hydration unit filters, and
`node --import tsx sdk-rs/safe-cex-candle-cache/scripts/hydration-error-oracle.mjs`.
Required integration gates are full parity index, all-features cancellation and
override checks, fmt, strict touched-crate native/Wasm all-target Clippy,
CI wiring and extractor check. Executed results belong in the PR description.
