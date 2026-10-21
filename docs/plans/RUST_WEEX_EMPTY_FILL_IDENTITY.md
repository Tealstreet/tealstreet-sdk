# WEEX empty fill identity visibility

Fresh landing at `501fb664edb6769bde4a2337bde56fc44d6b5724`.
Final port/wire sweep reverified that an in-scope parsed fill with empty native
id/tradeId/fillId is silently skipped and valid peers return as complete. Missing
native identity must never be invented. The overseer explicitly approved this
routine visibility repair: keep valid fills, keep skipping the unidentified row,
and mark the owning query incomplete rather than fail the entire fill history.

1. Reproduce a valid peer plus empty native fill identity returning silently
   without the existing #1627 owned completeness marker or coded warning.
2. Retain one per-query missing-fill-ID flag across windows/pages. For an
   otherwise admitted in-scope empty identity, emit one coded Warn row.skipped
   naming the missing field and history.incomplete with the actual command.
   Use existing completion/serialization ownership so the result is flagged.
3. Preserve row filters, valid financial values/IDs/order/deduplication, genuine
   empty completion, and strict failure for otherwise malformed financial rows.
   Never fabricate an execution ID or broadcast another query's marker.
4. Regress id/tradeId/fillId aliases, repeated rows and later windows, independent
   overlapping queries, retained valid fills, all-skipped versus genuine empty,
   out-of-scope rows and owner/result-flag association, including Error-only logs.
5. Independent reviews, WEEX+Binance native tests, strict Clippy/format/bans with
   four Cargo jobs, draft/full CI and a separate exact guard before merge.

Index triggers, missing income assets and other owner/live/protocol exclusions
remain unchanged. This is separate from pending Bitunix balance/settings PRs;
no unlanded Bitunix code is carried. No budgets or local release/replay/load work.

Verification:
- Three original-source regressions failed before the fix; restored full
  WEEX+Binance native suite passes all 407 tests, including all 23 history tests.
- Native-verified event fixture passes the host result-flag test and worker
  translated-owner isolation test. Error-only logging preserves the semantic
  marker; unrelated callers and genuine empty/filtered rows stay complete.
- Strict WEEX all-target Clippy, format and banned-pattern gates pass. Two
  independent source/test reviews are clear. Additional filtered order/time
  controls pass the final history suite.
- Kraken #1663 owns the current shared-source/harness merge window; this PR
  includes shared host/worker tests and remains held until that actual merge.
  No production host/worker serialization changes are needed for this repair.
