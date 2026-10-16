# CORE/P4a gate evidence

Measured code SHA: `38ccee6f47838aa808c1d5022a40a4a19699cd5c`. This file is a later documentation commit and does
not rebind these exits to a different SHA. Final measurements used the committed
versions of the three inherited WIP files; their original uncommitted bytes
were preserved outside the checkout and restored after the gates finished.

## Rows moved

Against integration `2bc83d78d4`, Python 3.11 `tomllib` over the fixed eleven
assigned manifests measures **17 pending→ported, 8 pending→divergent: 25 total**.
Before: 127 rows = 119 pending, 7 ported, 0 divergent, 1 not applicable.
At the measured SHA: 127 rows = 94 pending, 24 ported, 8 divergent, 1 not applicable.
All 24 formerly pending order rows are implemented; the additional native `load`
wrapper moves one row. The 94 venue entry functions remain pending with empty
Rust targets; descriptor data is not executable construction.

Census enumeration stopped at iteration 1 over the eleven assigned manifests.
The initial per-symbol snapshot is CORE_P4A_INVENTORY.md; the reproducible reader
is scripts/core-p4a-inventory.py. No full-project status census is claimed here.

## Parent and limits

The slice starts on `c5325263cc`; its integration merge-base is `115f2f1c82`.
Unreviewed query ancestors are `fe9530f6f2`, `a821d586a0`, `e4a29eab4a`,
`7fcaebd598`, and `c5325263cc`. **This branch is not merge-ready:** query consumer
migration/source boundary controls/accounting remain WIP. No query Value
conversion shim or query projection was added by this slice. The inherited
query exemption, tryParseJson manifest, and transport evidence drafts remain
uncommitted and were restored byte-for-byte, rather than bundled into this port.

## Measured exits

All Cargo commands ran serially. Native-source commands used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-num --all-targets --jobs 2`: exit **0**.
- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --all-targets --jobs 2`: exit **0**.
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-num --test order_policy --test order_policy_market --jobs 2`: exit **0**.
- `python3.11 sdk-rs/safe-cex-num/scripts/order-policy-controls.py`: exit **0**.
- `python3.11 sdk-rs/safe-cex-registry/scripts/native-load-control.py`: exit **0**.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit **0**.

The two scoped all-targets checks reported no compiler errors. They are not
workspace compilation. The order native gate passed four Rust tests: one
arithmetic comparison (9,779 source cases), one Market policy comparison
(7,231 source cases), the declared-boundary witnesses, and caller preservation
on success/later refusal. The native load gate passed one Rust test after its
compiled mutation was restored.

The extractor output was inspected, not inferred from the Yarn exit alone:
`Symbols: 7341 across 512 source files` and
`Would update 0 manifests; 0 added, 0 orphaned symbols.`
The writer was run after manifest edits; its final run reported zero updates,
added symbols and orphaned symbols.

## Discrimination

Each mutation below compiled, exited 101 at its named assertion, was restored,
and passed with exit 0. Both control scripts themselves exited 0. No survivor
was observed in this seven-mutation enumeration; iteration stopped at 1, with
no exhaustive mutation or floating-point-domain completeness claim.

```text
reduce-only-fallback: exit 101 at ORDER native roundMarketOrderSize bits
reduce-only-fallback: restored exit 0
breakeven-side: exit 101 at ORDER native breakevenPrice bits
breakeven-side: restored exit 0
inverse-notional: exit 101 at ORDER native notionalToNativeSize
inverse-notional: restored exit 0
spot-venue: exit 101 at ORDER native shouldRoundSizeDown
spot-venue: restored exit 0
price-identity-swap: exit 101 at ORDER market native price bits
price-identity-swap: restored exit 0
infinity-stop-gate: exit 101 at ORDER prepare unexpected refusal
infinity-stop-gate: restored exit 0
```

```text
native-eager-load: exit 101 at REGISTRY native lazy construction
native-eager-load: restored exit 0
```

Native numeric bits, field identities, optional field presence, constructor
identity and invocation order are asserted directly before any wire view.
The independent corpus dimensions are pinned in Rust and the observed comparison
counters must equal them. NaN-capable inputs are reconstructed explicitly;
these tests prove policy/native outputs, not the input Serde field mapping.
Price, stopLoss, takeProfit and triggerPrice have distinct source values, and
the compiled price/stopLoss value swap fails the native price-bit assertion.

## Review and declared boundaries

Native sub-agent review found borrowed input mutation on prepare failure and
missing clamp ticker-fallback coverage. Both were fixed before the measured
SHA: prepare borrows and clones its input, and eight discriminating fallback
cases cover price/mark/last, NaN/zero and missing ticker. A follow-up sub-agent
review confirmed those fixes and the actual native loader initializer witness.
The stale native require note was also corrected.

The eight divergent rows declare canonical CHOSEN/MIXED buckets: frozen stock
metadata (not a full ExchangeMetadata implementation), Rust Error identity/stack,
typed options requiring side where the source accepts a sparse generic input,
owned clamp input/injected console logging, and native future polling instead
of JavaScript Promise microtask scheduling. The source sparse acceptance and
metadata mutation differences have live witnesses. Full venue constructors,
JS object identity, executable getters and arbitrary mutated prototypes are not
claimed by the policy corpus.

Not run: workspace builds/tests, complete venue suites, host Clippy, wasm Clippy,
wasm checks, CI, Rust parity:check invention/status validation, and query cutover
acceptance. No PR was opened and no integration merge was performed.
