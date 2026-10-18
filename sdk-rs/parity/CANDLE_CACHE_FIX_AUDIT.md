# Candle-cache: eight assigned non-UTF-8 FIX-audit rows

Owner: codex-4ri6pb. Baseline: official master `be6906fe30`.
Exact assignment: `gap-census-evidence/assignments/codex-4ri6pb.json` (eight
`safe-cex-candle-cache` rows, not the superseded crate-wide list).

The audit retains all eight `divergent` labels. ACCEPT below closes only the
named boundary, not arbitrary behavior in that function. No production behavior,
typed Candle representation, storage API, or numeric-ingress policy changes.

| Assigned source row | Verdict | Real production probe and reason |
|---|---|---|
| `KeyValueCandleCacheBackend.loadAllChunks` | ACCEPT | `fix_audit::listed_extreme_keys_report_refusals_without_losing_healthy_chunks` invokes the real backend with healthy, BOM/decimal-prefix, >2^53, outside-i64 and wrong-prefix listed keys. It checks actual GET paths, usable candles and the exact refusal list. The maintained class separately demonstrates binary64 rounding and trusting a broken prefix contract. One-second slots need over 285 million years to reach 2^53; real candle dates cannot. Prefix-respecting filesystem/Redis stores cannot return another namespace. Allocation >16384 refuses before GET; default real cache chunks have 100 slots. Overseer approved these boundaries in thread-dm4ltqa1kuqc. |
| `KeyValueCandleCacheBackend.loadChunks` | ACCEPT | `fix_audit::all_calendar_adapters_and_both_real_kv_cache_paths_match_source` runs actual CandleCache/KV write/reopen/fetch for every one of 27 intervals under both read modes, asserting no second venue request. Native fields are checked before JSON. Existing filesystem NaN and malformed-row probes retain positive numeric hits and structural misses (decision 5). The direct pre-epoch round trip pins Euclidean slot 99; no exchange has pre-1970 candles. Invalid widths/indices remain outside values produced by typed intervals. |
| `calendarPeriodForInterval` | ACCEPT | The same 54 constructed cache profiles compare calendar classification, key-space use, real request boundaries and cached lookups. A bool replaces CalendarPeriod only inside the cache; monthly selection is separately retained by the actual cache's interval. The external union rejects `1W`, `W`, `M`, `bad`; the shipped cache receives typed authored intervals. |
| `chartCandleStartTime` | ACCEPT | Source and native start values, full cache request boundaries and persisted cache hits agree for all 27 intervals. Cache call sites normalize epoch milliseconds before the seconds-only helper. Aliases/automatic unit selection are not silently dropped user options. Existing monthly Date.UTC/TimeClip source tests cover ancient-year quirks and invalid dates independently. |
| `nextChartCandleStartTime` | ACCEPT | Three adjacent cached candles plus future-clipped real cache lookup exercise successive boundaries, including Monday weeks and February 2024's leap month. Same typed/seconds-only adapter boundary as start; real cache advancement matches source. |
| `chartCandleCloseTime` | ACCEPT | The source/native boundary comparison checks close separately; the first actual fetch persists closed candles, and a fresh cache serves them without HTTP. Monthly close and next have distinct source assertions. The typed cache never passes millisecond input directly to this seconds helper. |
| `getNumChartCandlesInRange` | ACCEPT | Native inclusive count must be exactly three before source comparison; actual future-clipped lookup checks remaining from/to/limit against the maintained class for every interval. Typed seconds are normalized at the cache boundary. No lost live range or diagnostic was reproduced. |
| `IndexedDBCandleCacheBackend.persistChunks` | ACCEPT | Compiled Wasm against real Chrome IndexedDB: existing 26 NaN/null/structural profiles plus `fix_audit_probe` issue a real put, drop its pending waiter, check callback-owner release and reload committed native fields. Maintained TS separately loses negative array properties; Rust retains pre-epoch fields, unobservable for exchange data. Normal live cache spawns and owns the persistence task until completion; direct waiter cancellation releases callbacks without undoing an issued browser transaction. Existing abort-only pending behavior remains unchanged. |

Pre-epoch acceptance is **unobservable exchange data**, not authorization to
correct source bugs generally. The overseer explicitly approved that rationale.
The source/null numeric codec defect was already fixed by #841; this audit
replays its real persistent-backend witnesses rather than reclassifying NaN as
malformed. Structurally malformed rows still miss/refetch under decision 5.

## Reproduce

Run Cargo serially from `sdk-rs`, with `CARGO_INCREMENTAL=0` and `--jobs 1`.
Source-dependent tests use `SAFE_CEX_TYPESCRIPT_PATH="$PWD/../node_modules/typescript"`.

```
cargo test --locked -p safe-cex-candle-cache --test fix_audit --jobs 1
cargo test --locked -p safe-cex-candle-cache --test runtime_backends filesystem_ --jobs 1
cargo test --locked -p safe-cex-candle-cache --test backend_api --test monthly_behavior --jobs 1
cargo build --locked -p safe-cex-candle-cache --target wasm32-unknown-unknown --example indexed_db_probe --jobs 1
```

From root, use matching wasm-bindgen 0.2.129 to create browser glue under
`sdk-rs/target/cache-fix-audit-browser`, then run
`sdk-rs/safe-cex-candle-cache/scripts/nan-browser-probe.mjs` with that directory. See `GATE_EVIDENCE.md` for
its `CANDLE_CACHE_PLAYWRIGHT` / `CANDLE_CACHE_BROWSER` settings. Dependencies and
browser glue stay in ignored target trees. This audit requires actual browser
execution; compilation alone does not close the IndexedDB row.

## Executed evidence

- Actual maintained source + native constructed CandleCache/KV: **54 profiles**
  (27 intervals x both modes), reopened cache makes **zero** additional venue
  requests. Three audit tests PASS, including exact refusal diagnostics and
  preserved pre-epoch native fields.
- Existing backend API **2**, monthly source/native **5**, filesystem persistence
  **3** PASS (13 tests including the audit). Three Redis tests remain explicitly
  ignored because they require isolated external services; no Redis execution is
  claimed by this PR.
- Actual Chrome IndexedDB, freshly compiled Wasm: **26** native profiles plus
  maintained TS backend writes, pre-epoch no-record source control, issued-put
  waiter cancellation/reload, native callback-owner release and existing
  cancelled open/read release all PASS. Browser log:
  `/tmp/port-txflow-cache-audit-browser.log`.
- Full parity index PASS **7563 rows / 16669 public Rust items**.
- Extractor check: inherited **one Backpack manifest drift**, zero added/orphaned
  symbols, assigned to wdtab1 by overseer in thread-dm4ltqa1kuqc. None of this
  PR's three manifests drift. No Backpack edit or broad regeneration here.
- Production `src/` bytes unchanged from the baseline; no cancellation pin
  changes or classifier exemptions.

Remaining required gates are recorded below when complete.
