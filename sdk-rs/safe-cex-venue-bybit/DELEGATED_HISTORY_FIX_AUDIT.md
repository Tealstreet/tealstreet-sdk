# Delegated Bybit history FIX-audit

Authority: original assignment `codex-0w5ohi.json` rows **6 and 16**, delegated to
codex-0efu4x in Aimux thread `thread-dm5145xa43r7`. These are the primary and
inverse `mapOrderHistoryRowsToExecutions` rows. The mapper is synchronous; the
old timing category does not justify adding an eager-start task to it.

Both rows are **FIX**. Through the constructed `Exchange`, real signed HTTP and
installed receiver, a truthy object `feeCurrency` previously caused typed
`Vec<Execution>` conversion to reject the entire successful recovery, losing
valid rows before and after it. The original compiled assertion failed at
`BYBIT-HISTORY-ROW: malformed output must not reject good neighbors` with
`TypeError: invalid type: map, expected a string`.

The live reader now applies `shared-type-decisions-final.md` decision 3 after
its full source projection and recovery logic have settled. A non-string
`symbol` or present non-string `commissionAsset` refuses only that execution,
with a warning identifying the output field and its original value. It does
not coerce either strict field. Good neighboring executions retain order and
values. An all-refused result is empty without starting a different recovery.
A later source mapping/page failure still takes precedence and discards the
recovery accumulator before output admission runs. A warning-listener error
propagates with its native error-code identity, outside the source fallback
catch.

The raw mapper and its **380** maintained-source cases, including **16** direct
strict typed refusals and both fee-detail surrogate cases, remain unchanged.
Native witnesses retain object/array field aliases and post-mapping mutations
before independently requiring the original typed guard to reject them. The
output admission reuses the original array when no row is refused and retains
each surviving native row rather than rebuilding or serializing it.

`tests/delegated_fix_audit.rs` drives all three live constructors (primary,
demo and dedicated inverse). Its fixed matrix has 48 malformed-field cases:
2 fields × 4 values × 3 constructors × 2 routes (primary execution and fallback). Additional cases cover all-refused,
missing fee asset, source numeric-order-ID conversion, later-page error
priority and refusal-listener propagation. The unchanged full reader oracle
continues checking requests, results and diagnostics against maintained TS.

`scripts/delegated-history-controls.py` independently breaks final admission,
good-neighbor retention, both field guards, visible diagnostics and the timing
of admission relative to a later source error. Every mutation must compile,
fail a named production-binding assertion, restore exact source bytes and pass
all ten integration tests. Its receipt records the source SHA256 and every
RED/GREEN log digest. No identity, alias, source-corpus or cancellation guard
is weakened.

This milestone closes **2 of the 12 delegated rows**. The other ten delegated
rows (1, 2, 3, 7, 8, 12, 13, 23, 25, 35) remain assigned and unaudited by this PR.
