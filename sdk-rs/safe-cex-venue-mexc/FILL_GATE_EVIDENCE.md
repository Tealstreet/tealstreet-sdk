# MEXC fill projection repair

Parent: ac0f5fccf24207b26c279e67da2a88f2c72a7aae (integration). This branch
does not inherit the unmerged webworker PR7864 or WOO PR7852.

Tomllib across all tracked MEXC manifests:185 rows,145 ported /40 divergent /
0 pending. Baseline22 leading FINDING rows becomes21 after the fill row is
reclassified MIXED with its observed text refusal and retained frame/error
contracts. Pending movement0→ported,0→divergent; divergent→ported0. This is
one method repair, not a closure claim over every inline finding or the venue.

At de82997e0324031f78976899f3dc9fcf7ec83138 the strengthened old missing-field
test failed101 at `FILL native retains empty orderId`: actualNone versus own
empty string. No production source was deliberately broken for that baseline.
The repair replaces the JSON event sink with the native eight-field record,
uses source number spelling for fallback IDs, passes only row.symbol to the
outer market lookup, permits unused asset values, and retains TypeError names
and null access ordering. The public Execution output refusal remains closed.

The actual Source body floor is12 cases and9 emitter calls. One materialized
U+D800 symbol is accepted and emitted by Source; native JSON refuses its whole
input. The native gate exercises the remaining11 cases with8 emit calls, measured at
7999bbe7a61bc893d0f8b4948a30c67b44d34b53. Assertions check native
fields, enums, number bits and errors before secondary JSON comparisons. The
Source constructor here is an extracted receiver for the maintained function,
not a claim of full venue/socket construction.

Classification cites [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary). These references
do not justify numeric loss. Production changes are confined to this crate.

At7999bbe7a61bc893d0f8b4948a30c67b44d34b53: the four selected fill tests, two
mapper tests, scoped all-targets cargo check, Source supplement, writer,
extractor check and restoration diff exited0. The compiled amount-collapse
control failed101 at `FILL native numeric bits amount-overflow.amount`, then
passed restored0; no survivor, stopping after one planned compiled control.
The existing Source fee control also passed inside the four-test target.

An earlier repair attempt at382dc8685b65aa65e2d829d8391c49e39267ead1 exited101
during test compilation (fixture used an inherent update instead of the Store
trait method); no runtime assertions executed in that attempt. It is retained
in FILL_GATE_RESULTS.json beside the red baseline and final measured exits.
Pre-gate sub-agent review found no production blocker and requested a fixed
four-field count before the numeric zip; that count is included. The sell
fixture distinguishes price, amount, fee and negative PnL. Reviewers ran no gates.
Not run: full venue suite, workspace checks/tests, host or WASM clippy, WASM
tests, TypeScript typecheck, live exchange transport or CI.

Final sub-agent review at866734a3ee414196fbd508c2270e6c5b1b990b65 was clean;
it ran no gates. Merge-down incorporated integration1a43f3a8f2. Own source/tests
remain byte-unchanged from7999, and integration added no SDK or maintained
safe-cex source changes relative to the built parent. Root package CI scripts
changed, so extractor check was measured fresh at866734a3ee and exited0 with
zero updates/additions/orphans. Other named gates retain their original SHA.
