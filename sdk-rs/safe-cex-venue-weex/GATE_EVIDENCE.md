# WEEX market data port evidence

Measured code commit: `4cb62905e04b97eeed8258681e3f15156ad69316`.
Initial port parent actually built on: `2ed0f98cd844262cd68c0bec35e4aba15381989b`.
Latest measured code direct parent: `4a3a133e4cbd88dd343b82f7f99058a2408fbf8c`
(the initial evidence commit).
That predecessor commit is unreviewed/un-PR'd according to the assignment and
remains an ancestor. The branch is not rebased or merged onto the newer
integration head. Integration comparison is against
`2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`.

Counts measured with Python 3.12 tomllib over the six WEEX manifests:

| Revision | Ported | Divergent | Pending |
| --- | ---: | ---: | ---: |
| Integration 2bc83d78d4 | 26 | 18 | 112 |
| Direct parent 2ed0f98cd8 | 26 | 22 | 108 |
| Measured code 4cb62905e0 | 26 | 24 | 106 |

This slice moved **0 pending -> ported, 2 pending -> divergent**:
normalizeMarket and normalizeTicker. Total branch delta from the integration
comparison is 6 pending -> divergent; the other four are inherited balance,
candle, income-record and public-trade ports. No income pager was implemented.
Both new rows use CHOSEN notes describing retained-input and model boundaries.

All following exits were measured after committing the code SHA above, with
cargo commands executed sequentially. These measurements do not claim to cover
an evidence-only descendant commit, workspace compilation or other venues.
Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Exit | Observed result |
| --- | ---: | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 | Scoped crate and dependencies compiled |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/market-data-mutations.py | 0 | Both mutations compiled, named assertions failed, source restored, target passed |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test market_data (restored target inside mutation script) | 0 | 2 tests passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0 | WEEX status/invention gate: 156 rows, 55 public Rust items |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Output read: 7341 symbols; would update 0 manifests, 0 added, 0 orphaned |

The extractor writer was run after manifest edits before this commit and reported
0 changes, 0 added, 0 orphaned. Its earlier invocation is not asserted to have
executed at the measured commit; the check-mode result above did.

Discrimination at this code SHA, each with an intentional temporary source delta:

- Swap native market precision amount/price: cargo exit 101 at
  `market_data::market::distinct::route_0::amount`.
- Feed quoteVolume into ticker volume: cargo exit 101 at
  `market_data::ticker::distinct::route_0::volume`.
- Restore source: cargo target exit 0. No compiling mutation survived these two controls.

An earlier unilateral market mutation failed compilation (unused price_step,
warning denied) before any assertion. It was changed to the compiling symmetric
swap above; that initial rejection is not claimed as test discrimination.

Native checks read strings, flags, numeric bits and retained info before
any wire result assertion (there are no model wire assertions). The finite
source inventory stops at 12 market cases and 14 ticker cases: four ingress
routes, 80 successful native records, 24 errors and 816 numeric comparisons.
The three malformed Market string-field cases witness chosen projection
refusals; callable coercion and UTF16 source supplements witness input-domain
restrictions. This is a finite corpus, not an exhaustive runtime or precision
coverage claim.

Not run: full crate suite, monorepo suites/builds, workspace cargo check,
host clippy, wasm clippy, live venue/network tests, old-row mutation matrices,
or CI reruns. Read-only sub-agent review found the precision exponent-conversion
defect in the initial port. The follow-up guard and native test were reviewed at
this measured code SHA, with no further defect reported. The reviewer ran no
cargo/tests/mutations and performed no edits.

Read-only Store inspection: this crate has no Store instance, borrow guard or
notification call. Its sole Store-crate call is the pure orderbook total helper
in src/utils.rs. No borrow-guard patch was needed. The reported predecessor
tautology was not reproduced as a test result here; existing fake-position
counts compare a Vec length to independent literal 2, and this slice does not
claim to re-prove the inherited suite.


Review correction: parsed counts above 536,870,888 are refused before
Number-to-String can produce exponent notation and a second parser can read only
its prefix. The added native overflow test passed for two oversized string counts
in both precision fields (four refusal assertions), including eager quantity
fallback. The enormous TypeScript concatenation loop was NOT run; source failure
at huge counts is a control-flow inference. The reviewer measured only the small
Node parseInt/String diagnostic (exit 0), confirming 1e21 -> "1e+21" -> 1.

Initial scoped gates also passed at 888227edfd0c1f053b4a73cf8f56e7f8cf345493;
those earlier results are not being transferred to this correction. Every gate
in the table above was rerun and completed at the latest measured code SHA.
