Current carrier cutover: model info now retains native Serialize output directly.
The historical NaN refusal below is superseded for the typed ticker/market/order
mappers; their native metadata observations now require successful NaN retention.
The pure position helper still takes Value, so its explicit nonfinite input
projection refusal remains separate. No output-info refusal is inferred from it.

# Aster native metadata and signer evidence

Parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae` (integration; no unreviewed feature ancestor).
Reviewed implementation: `a5153e03d1684cc13d58d4630df9c040237c888f`.
Reviewed note correction: `c99034cb1983d0fdafe2a50a63ac8bd19864cc87`; production,
source fixtures and native tests are byte-identical to a515 (git diff exit 0).
This manually authored record follows those commits.

Tomllib: 130 ported / 92 divergent / 9 pending, unchanged statuses; five FINDING
rows closed. FINDING-containing notes 30 -> 25 on this independent branch.
The other 25 are addressed separately by the stream and numeric ingress PRs.

At a515, scoped `cargo check -p safe-cex-venue-asterdex --all-targets --jobs 2`
exit 0. Final source/test inputs are unchanged from the measured gates below:

- At `164fc9c2aec9b6f834d3969ac3a21b8618e1de4b`, native
  `mapper_metadata_retains_native_numbers`: exit 0, one test; restored at a515:
  exit 0. Independent floor 18 numeric metadata cells, five values minus two
  nonterminating source cases = 88; stopped at iteration 87. Native metadata
  binary64 assertions precede any serialization. All NaN cases assert whole-call
  Err/no model; this differs from the usable source model retaining native NaN.
- At 164fc, `api_message_signature*`: exit 0, three tests (including source
  wrong-chain child failing 101 and missing real ethers child failing 1).
  Restored at a515: exit 0, three tests. Independent expectation 12 valid
  signatures plus nine invalid keys. Actual error class is asserted natively.
- At 164fc, `exchange_helpers_match_typescript`: exit 0, one test.
  At a515, `order_mapper_matches_typescript`: exit 0, one test.
- Compiled production abs-value mutation at a515: exit 101 in
  `mapper_metadata_retains_native_numbers`, named assertion
  `native metadata bits ticker/openTime iteration 0` (positive versus negative
  zero). Compiled signer-class mutation: exit 101 in
  `api_message_signatures_match_typescript`, `native signing error class`
  (Error versus TypeError). Both files restored byte-for-byte; restored gates
  above passed. No surviving mutation is hidden.
- Extractor check at a515: exit 0, output `Would update 0 manifests; 0 added,
  0 orphaned symbols`. Writer after note correction c990: exit 0, updated 0.
- rustfmt on touched Rust tests: exit 0. Integration fetch/merge-down: exit 0,
  already up to date at ac0.

The first expanded source fixture at
`2e9010a6b9be6f202934f825f56e67e3b3a317e7` exhausted Node's heap (exit 134).
Inspection located the source `precisionToString` unbounded concatenation loop
for positive-infinite price/quantity precision; those two cases were removed
from this terminating floor, not counted as passes. Native NaN metadata cannot
be represented by the fixed shared JSON owner and now refuses visibly. See
[canonical numeric requirement](../parity/MANIFEST-SPEC.md#numeric-ingress-defect)
and [divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
Shared retained numeric representation handles infinities and signed zero.

Live official documentation checked: [Aster API repository](https://github.com/asterdex/api-docs)
and its [V3 documentation](https://github.com/asterdex/api-docs/blob/master/V3%28Recommended%29/EN/aster-finance-futures-api-v3.md),
including numeric ticker timestamps/counts and position updateTime. TypeScript
is the maintained behavior reference; docs do not replace it.

Not run: workspace compilation, full crate/monorepo suites, host clippy, wasm
compilation/clippy, Node 26 for this slice, credentialed/live exchange requests,
CI reruns or networking integration. No such coverage is claimed.
