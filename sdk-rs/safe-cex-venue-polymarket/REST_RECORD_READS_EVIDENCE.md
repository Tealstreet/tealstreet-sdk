Moved **0 pending → ported, 8 pending → divergent** relative to integration
`2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`. Four rows were inherited (three
utilities and server time); this implementation adds four REST rows. Against the
actual parent `7a1903524db82f4554a7b9cc075c991badd347e4`, it moves **0 → ported, 4 → divergent**.

The independently parsed venue census is **13 ported / 18 divergent / 110 pending**
(141 rows). Integration comparison: 13 / 10 / 118. Parent: 13 / 14 / 114.
Counts were measured with Python 3.14 `tomllib` over the seven venue manifests,
reading baseline files with `git show`; they are a census, not behavioral proof
of all rows. No manifest status changes were made after these measurements.

Implementation SHA: `6d7055561c40d3a427c3feb0a7dbe8308365b2ac`.
Mapping declaration SHA: `cc7e938106fccb8b0978ba1d1b4e1a8848aa1bf0` (five note changes only).
Actual implementation parent: `7a1903524db82f4554a7b9cc075c991badd347e4`.
Actual merge base with the specified integration revision:
`115f2f1c82077057764e859d6980d6c955a32315`.

**Ancestry warning:** the parent includes utility and server-time work absent from
the specified integration revision. Prior review of that inherited work is not
verified here. The new branch preserves that ancestry; it does not rewrite or
merge to integration.

| Gate | Measured SHA | Exit | Observed result |
| --- | --- | --- | --- |
| Four native controls | `6d7055561c40d3a427c3feb0a7dbe8308365b2ac` | 0 | Baseline 0, all four faults 101 at named native assertions, all four restorations 0 |
| Scoped Cargo check | `6d7055561c40d3a427c3feb0a7dbe8308365b2ac` | 0 | `-p safe-cex-venue-polymarket --all-targets --jobs 2 --locked` |
| Venue tests | `6d7055561c40d3a427c3feb0a7dbe8308365b2ac` | 0 | 12 passed; unit/doctest targets contained zero tests |
| Scoped formatting | `6d7055561c40d3a427c3feb0a7dbe8308365b2ac` | 0 | `cargo fmt -p safe-cex-venue-polymarket --check` |
| Scoped mapping, initial | `6d7055561c40d3a427c3feb0a7dbe8308365b2ac` | 1 | Five qualified-path correspondence errors: four new rows and inherited server time lacked explicit receiver projection declarations |
| Scoped mapping, repaired | `cc7e938106fccb8b0978ba1d1b4e1a8848aa1bf0` | 0 | Invention/status passed: 141 rows, 38 public Rust items, no exemptions |
| Extractor freshness | `cc7e938106fccb8b0978ba1d1b4e1a8848aa1bf0` | 0 | Output read: Would update 0 manifests; 0 added, 0 orphaned; 7341 symbols across 512 files |

The production and test trees are byte-identical between the two measured SHAs
(`git diff --exit-code` over the venue was exit 0). Cargo/test results are attached
to the implementation SHA above; mapping/extractor results are attached to their
later SHA. The subsequent evidence commit changes documentation only and is not
claimed as a separately built SHA.

| Fault (fixed floor, stopped after fourth) | Named native assertion | Mutation | Restored |
| --- | --- | --- | --- |
| markets-size-tick-swap | `record_native_minimum_order_size` | 101 | 0 |
| book-bid-ask-swap | `record_native_bid_count` | 101 | 0 |
| books-response-empty | `record_native_book_count` | 101 | 0 |
| prices-token-price-swap | `record_native_price_token_id` | 101 | 0 |

No survivor occurred within these four controls. No additional fault enumeration
is claimed. `finally` restores touched files even on failure; the runner refuses
dirty venue sources/manifests before binding its baseline to HEAD.

The oracle extracts and executes original `getMarkets`, `getOrderBook`,
`getOrderBooks`, `getPrices` bodies plus the original ENDPOINTS declaration,
with an injected recording HTTP receiver. Factory construction is not exercised.
One Node process captures the full 4-method × 9-case floor (iterations 0–8):
36 source cases, 69 accepted provenance comparisons, 37 refusals, 102 native row
checks, 135 native level checks and four full HTTP failure metadata comparisons.
Fields and timestamp bits are checked as decoded native values before request
body bytes; responses are never validated by decode/re-encode equality. Numeric
bits are captured before source JSON serialization. Capture: arm64, Node major 24.

Bodies cover Encoded, Untransformed and Transformed ingress; the lone-surrogate
market is encoded-only because Rust Value cannot hold that JS string. Fixed
expected row/level counts and fixed indexed traversal are independent of source
capture lengths. Empty arrays, duplicates, embedded commas, Unicode, unknown
fields, missing fields, null, transformed JSON-looking strings, negative zero,
overflow, rounded integer and negative underflow occur within this floor.

Each row uses CHOSEN for strict owned typed projections: unchecked shapes and
lone surrogates refuse; unknown properties and live JS aliases are not preserved.
Exact Rust targets and the this.xhr → injected HttpClient projection are declared
without fabricating a completed API class. No prediction-core normalization,
domain mapping, pagination, signing, retry, constructor or factory is claimed.

Sub-agent static review covered production, oracle/native assertions, controls,
and receiver mapping declarations. Its independent-floor finding was fixed before
commit; final review reported no actionable findings. The reviewer ran no gates.

Not run: host Clippy, wasm Clippy, workspace compilation, full monorepo suites,
live or credentialed API calls, CI reruns, or inherited mutation matrices. The
predecessor's guard-removal verdict was not re-proved by this slice.

The extractor writer initially failed because the shared smol-toml installation
lacked package metadata. The exact 1.4.2 yarn.lock tarball was SHA-1 verified and
installed in ignored worktree-local extractor node_modules, leaving shared
node_modules unchanged. Writer subsequently reported no stale manifests; final
check output, rather than the Yarn exit alone, established freshness.

Reproduce from repository root, one Cargo invocation at a time:

```sh
export SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
export POLYMARKET_CONTROL_LOG_DIR=/tmp/polymarket-record-controls
python3.14 sdk-rs/safe-cex-venue-polymarket/tests/record_reads_controls.py
/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --all-targets --jobs 2 --locked
/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --locked --jobs 2 -- --nocapture
/opt/homebrew/opt/rustup/bin/cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --check
python3.14 sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Complete local output and JSON summaries are retained under ignored
`sdk-rs/target/polymarket-rest-record-reads/<measured-sha>/`.
