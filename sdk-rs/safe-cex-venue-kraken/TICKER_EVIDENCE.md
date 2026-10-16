# Futures ticker read controls

Actual-source corpus: 49 cases, 63 native records, 693 direct numeric field
comparisons. One Node subprocess executes the whole read corpus. Capture header
records CPU architecture and Node version; NaN alone is canonicalized.

Eight controls compiled and exited 101 at named assertions, with eight restored
passes. Consuming controls drain every input element before returning None. The
bypassed configuration removes both native length checks, retaining independent
fixed-index coverage and explicit comparison totals.

```text
bid-ask-swap: compiled; exit 101; TICKER native bid
bid-ask-swap: restored pass
empty: compiled; exit 101; TICKER independent native length
empty: restored pass
subset: compiled; exit 101; TICKER independent native length
subset: restored pass
extra: compiled; exit 101; TICKER independent native length
extra: restored pass
empty-guards-bypassed: compiled; exit 101; TICKER native index coverage
empty-guards-bypassed: restored pass
trace-flatten: compiled; exit 101; TICKER trace failure allocation
trace-flatten: restored pass
consuming-empty: compiled; exit 101; TICKER independent native length
consuming-empty: restored pass
consuming-empty-guards-bypassed: compiled; exit 101; TICKER native index coverage
consuming-empty-guards-bypassed: restored pass
6 venue production + 2 consuming shared production controls; 8 named failures; 8 restored passes
```

Original reads oracle remains byte-identical (SHA256
70da2916f8efa6626a1f47f74e23d30432a648e6823b2593d8d16e946959c8fa).
No two-axis precision site exists in this read body: zero price-step and
amount-step adjustments. This is a census, not a future writer coverage claim.

Controls ran before integration merge-down; final SHA gates and review are
reported separately, without relabelling these controls as post-merge runs.
