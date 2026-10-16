# Historical positions delegation

One Kraken row calls the shared fold_closed_position_records operation; no local
folder or pager is introduced. Constructors/fills wiring remain pending. Measured
with Python tomllib: 24 ported / 19 divergent / 90 pending (133 total).

Ten actual-source cases execute the unchanged Kraken and BaseExchange arrows in
the existing single Node process. Source and native counts are independently
pinned: ten cases, nine folder calls, sixteen returned records across eight success
cases, two error cases, ten completed comparisons. Inputs distinguish from/to and
include negative zero. Native record and error allocations are retained. A separate
source-acceptance/Rust-refusal pair records the typed account boundary.

The original reads-oracle.cjs remains byte-identical, SHA256
70da2916f8efa6626a1f47f74e23d30432a648e6823b2593d8d16e946959c8fa.
No original fixture values were repaired. Captures retain the existing architecture
and Node-major header; numeric comparison canonicalizes NaN only.

Seven production controls compiled, failed at named runtime assertions with exit
101, restored original source/test bytes and passed. Removing the length guard
while keeping empty native records fails direct native record equality. This is
not a consuming-deserializer control: the new options decoder has scalar fields,
and the output Vec is returned directly from the shared folder without decoding.

```text
symbol-scope: compiled; exit 101; HISTORY native fold arguments
symbol-scope: restored pass
bounds-swap: compiled; exit 101; HISTORY native fold arguments
bounds-swap: restored pass
empty: compiled; exit 101; HISTORY native record count
empty: restored pass
subset: compiled; exit 101; HISTORY native record count
subset: restored pass
extra: compiled; exit 101; HISTORY native record count
extra: restored pass
empty-length-bypassed: compiled; exit 101; HISTORY native records
empty-length-bypassed: restored pass
flatten-error: compiled; exit 101; HISTORY error allocation retained
flatten-error: restored pass
7 production mutations failed named assertions; 7 restored passes
```

Read-port compile iteration 4 passed after three compile fixes (numeric-output
registration, Store trait dispatch, explicit notification argument in the witness).
The final scoped check iteration and SHA-specific gates are reported separately;
no workspace compilation or unrelated-crate review is claimed here.
