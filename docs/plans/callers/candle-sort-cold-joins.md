# One concrete Candle sorter and concrete string joins (wasm size)

**Change.**
- `safe_cex_engine::model::sort_candles_by_time(&mut [Candle])` replaces every
  venue's `rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp))` on candles
  (19 sites). Same comparator, same stable sort: oldest first, equal timestamps in
  input order, `total_cmp` placement of NaN and signed zero.
- `safe_cex_engine::text::JoinWith::join_with` replaces `.join(sep)` on `[String]`
  and `[&str]` in venue crates (89 sites). Its body is the std `join`, so output is
  identical.

**Why.** Each sort closure, and each crate's `join`, monomorphized its own copy at
that crate's opt-level. Combined wasm (local, Binaryen 132): 16,766,081 ->
16,709,127 B (-56,954).

**Callers.** A new venue's candle sort calls `sort_candles_by_time`, and its string
joins use `join_with` (`use safe_cex_engine::text::JoinWith;`). Std joins remain in
the engine (one copy in the wasm crate), `safe-cex-types`, `Cow` slices and the
blofin/okx socket codecs.

**Obligation.** None. **Deletion prerequisites.** None added.
