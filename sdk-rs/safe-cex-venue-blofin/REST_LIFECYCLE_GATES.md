# Blofin REST factory, remaining readers and lifecycle

Eight pending rows moved: **3 ported / 5 divergent**. Constructor, disposal and
tick are ported within their stated base/socket/reader/Store host contracts.
API, validation, startup, markets and OHLCV have specific CHOSEN notes and
actual-source witnesses. The full class and websocket/listener rows remain pending.

`tomllib` stopped after all seven Blofin manifests / 125 rows: parent
**16 ported / 66 divergent / 43 pending**, then **19 ported / 71 divergent /
35 pending**. Inventory is separate from behavioral proof.

## SHA bindings

- Actual integration parent: `a739087a0791b49fb0d6d34a5ac0d631f3ce6130`.
- Implementation: `2978e3939ae12ca26bbdc8c34ec3d25dd0241447`.
- Corrected production/source gates: `ef2e732a983d141e756876e1204e9ca16456115f`.
- **Pre-merge Cargo gate and static review SHA:
  `170600eed19b0133e20044a99dcc99f3362daa93`.** The last commit changes one
  test-only Box assignment, preserving production, source oracle, manifests and
  controls (`test-allocation-preservation.json`, scoped diff exit 0).
- Reviewer `/root/ticker_review` ran static reviews only, no Cargo, source oracle,
  mutation or network gates. Accepted findings and resolved deltas are recorded
  in `rest-lifecycle-evidence/review.json`.

## Measured gates

All Cargo commands ran serially, scoped to `safe-cex-venue-blofin`, jobs 2.
Exact commands, exits, timing and measured SHAs are in `cargo.json` and `source.json`.

| Gate | SHA | Exit / result |
| --- | --- | --- |
| Cargo check, all targets | 170600eed1 | 0 |
| Crate-only host Clippy, all targets, `--no-deps -D warnings` | 170600eed1 | 0 |
| `rest_lifecycle` test binary | 170600eed1 | 0; 9 tests pass |
| Rustfmt check for new source/tests | 170600eed1 | 0 |
| Extractor check | ef2e732a98 | 0; zero updates/additions/orphans in output |
| Existing Blofin source declaration coverage | ef2e732a98 | 0; independent 20/3/3/66 floors, stopped after 4 files / 92 declarations |
| Blofin-only Rust parity audit | ef2e732a98 | 1; only 3 inherited `position_adapter.rs` findings |

The Rust parity audit uses the existing binary (not rebuilt this slice), production
symlinks and empty exemptions. The inherited targets are `BlofinPositionAdapterDeps`,
`map_blofin_position_history`, and `num`. Parent-preservation diff exits 0; these
findings were not changed or exempted. Declared files and qualified trait/impl
targets were generated from real source symbols; extractor owns canonical TOML
serialization. No class target was fabricated.

## Completed post-merge gates

All changed-input checks completed at **`245c107385cf55b1040d7f1c6749599cb44868ea`**:
scoped Cargo check **0**, crate-only host Clippy **0**, `rest_lifecycle` tests
**0 / 9 passed**, Rustfmt **0**, extractor **0** with zero updates/additions/orphans,
and source declaration coverage **0**. The Blofin-only Rust parity audit remains
**1**, exactly the same three inherited findings. Post-merge tomllib census is
**19 ported / 71 divergent / 35 pending**, eight moves relative to the actual parent.
Exact commands, exits and raw output are under `rest-lifecycle-evidence/post-merge/`.
The static preservation review is bound to this same merge SHA; mutation controls
remain bound to `170600eed19b0133e20044a99dcc99f3362daa93`, with byte preservation
proven rather than relabeling their measurement SHA.

## Native proof and limits

Actual maintained TypeScript bodies/constructor/field initializers execute with
genuine numeric helpers, DefaultStore, dayjs, AxiosHeaders, qs, HMAC and axios-retry.
Common HTTP/base/socket capabilities are injected boundaries, not a claim that
pending socket bodies or the full facade are implemented.

Enumeration stopped at **61 REST/lifecycle, 30 API and 5 constructor cases**,
plus **6 boundary witnesses**. Native REST readers use 6 body/suspension variants;
lifecycle uses 2 suspension variants. Independent floors yield **297 case
executions + 6 boundary executions**. Decoded numeric bits, strings, enum values,
optionals, intermediate/final native Store snapshots, request headers, auth counters
and retry deadlines precede trace comparisons. No decode/re-encode roundtrip gate.

Specific source witnesses pin owned-vs-live API credentials, returned validation
error hints, market fallbacks and unsupported market info/string values; invalid
date refusal; New York spring DST (source 23h vs native fixed 24h); native NaN-barrier
insertion vs V8 sorting; and deferred market continuation after peer rejection.
These are declared choices, not hidden successes. The native interfaces require
compatible configured hosts; live HTTP, proxy/remote signer integration and socket
operation are not proven by injected fixtures.

Review corrected two implementation issues: source filtering must finish before
mapping, and the factory's 20-second timeout must enter retry budgeting before
classification. Actual axios-retry clocks advance 7s per attempt: default/zero/5s
budgets produce 3/4/1 attempts with native deadline assertions. Each retry re-signs;
successful nested retry unwinds run the auth handler four times, including final
credential errors. Date clipping canonicalizes truncated negative fractional zero.
The non-total NaN comparator gap is explicitly CHOSEN rather than called parity.

## Mutation proof and final merge

Ten deliberate runtime controls at `170600eed19b0133e20044a99dcc99f3362daa93`
were caught at their named tests, exit **101**, and every restored test passed,
exit **0**. Enumeration stopped after control **10**; no runtime survivors or
compiler-only controls. Call sites, commands and raw logs are retained under
`rest-lifecycle-evidence/controls/`. Source/test restoration diff exits **0**.

Integration `71ab6dd78100ff547dbfb64e2ded8d84196adcc8` was merged down at
`245c107385cf55b1040d7f1c6749599cb44868ea`. Actual scoped owned-source/test/config/
manifest diff against gate SHA1706 exits **0** (`merge-preservation.json`). Shared
Rust/source-oracle inputs are unchanged. Cargo.lock adds an unrelated Bitget
crypto dependency and package.json adds an unrelated Kucoin script; changed-input
scoped gates are measured again at the merge SHA under `post-merge/`, without
relabeling the earlier measurements. Integration ancestors are inherited and
not independently reviewed by this lane; no private lane branch was merged.
Static preservation review is clean at the merge SHA; reviewer did not audit
untracked documentation/evidence or independently run the mutation controls.

## Retained failures and unrun gates

The initial 2978 check exited 0 and crate Clippy exited 101 (two nested conditionals).
Its extractor/coverage and census completed, then source aggregation incorrectly
checked Rust findings on stdout; the checker reports them on stderr. The corrected
driver records both channels. At ef2, check exited 0 and crate Clippy exited 101
for one test-only replace_box allocation; 1706 fixes it. Both attempts are retained.
A dirty development run crossed a corpus edit and failed the 29/30 floor, exit 101;
it is retained as development evidence, not a SHA-bound gate.

Not run this slice: dependency-inclusive host Clippy, wasm compilation/Clippy,
workspace compilation/suites, full crate suites, live venue/network/proxy/remote
signer/socket tests or CI reruns. Earlier dependency-Clippy findings are inherited
context, not newly measured gate exits. Raw Cargo logs retain their stdout bytes.
