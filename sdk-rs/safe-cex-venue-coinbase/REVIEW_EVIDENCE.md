# Coinbase continuation evidence — 2026-10-13

Moved **3 pending → ported, 1 pending → divergent** in this continuation.
Measured final code SHA: `c12cfabe5ec6ee9bccdab5a3b30ef34ff4d8fa20`.
The following evidence commit changes documentation and recorded results only;
no gate exit is claimed as a new measurement at that evidence commit.
Exact commands, exits, mutation diff hashes and transcript hashes are in
[REVIEW_GATE_RESULTS.json](REVIEW_GATE_RESULTS.json).

## Parent and census

Actual checkout parent was `1857224642626ac25b1be7e5528736f0bc99bf64`,
whose parents are Coinbase `88c2f257cb52aa3a34753804d7e787cd4e76600b` and
integration `2d4daf6cec51b792b840f9c382ca6441eb48c3e5`.
**Warning: this branch contains unreviewed Coinbase ancestry**, including
`bdb248548c`, `66ba0e1eb6` and `88c2f257cb`. This continuation did not merge
integration, create a PR, or rewrite history.

Python 3.11 `tomllib` parsed every symbol in the 16 Coinbase manifests,
using `git show REV:path` for historical revisions. Stopped after census
iteration one, at the current 426-row manifest floor. This is a manifest
census, not proof of full Coinbase runtime coverage.

| Revision | Ported | Divergent | Pending |
| --- | ---: | ---: | ---: |
| integration `2bc83d78d4` | 258 | 13 | 155 |
| `bdb248548c` | 267 | 13 | 146 |
| checkout parent `1857224642` | 269 | 13 | 144 |
| final tested code | 272 | 14 | 140 |

Compared symbol keys `(file, ts, kind)`, not just totals: this continuation
moves 3 pending → ported and 1 pending → divergent. The entire branch versus
`2bc83d78d4` moves 14 pending → ported and 1 pending → divergent.

The new ported rows are `base64UrlEncode`, `base64UrlEncodeString`, and
`asn1Wrap`. `pemToPkcs8Der` moves to CHOSEN divergent: native DER bytes and
rejection match, while a unit Rust error discards platform DOMException
name/message metadata. The manifest explains the choice and alternative.
Key import, JWT signing, and the other 140 rows remain pending.
The extractor writer ran after the manifest edit and reported zero
reconciliation changes, additions or orphans.

## What bdb248548c closes

This is source inspection and TOML measurement of that historical commit,
**not a claim that this agent compiled or mutation-tested bdb248548c**.
Its diff adds an independent Store callable floor of 151, checks both Market
display symbols, and expands direct Ticker field assertions. It also corrects
the enum-projection diagnostic so it no longer calls serialization native proof.
It does not change the loop-side identity counter in either generic wire
consumer, pin credential/public-frame corpus content, or pin the generic wire
corpus content. Later `88c2f257cb` adds credential and public-frame input pins.

The original five review texts/IDs were not supplied in the brief or present
in the inspected repository evidence. Exact five-way finding attribution is
**unproven**; no 5/5 closure claim is made. Known code sites and the three
failure shapes are evaluated below rather than assigning invented finding IDs.

## Native checks and corpus pins

The inherited dirty diff was legitimate test-hardening work, with no deliberate
broken production source. Its counter still incremented if the passed closure
was omitted. The count now lives inside the passed closure wrapper, while the
shared invoking helper has no counter increment. Both consumers finish every
native field assertion and independent execution-count check before any
fingerprint/roundtrip check.

Recursive identity checks inspect native String/bool/f64-bit values, optional
values, arrays and exhaustive enum matches; no field or enum identity helper
serializes the decoded value. Static sub-agent review counted 42 enum mappings
and found nine omitted nested field links in four records. The links are fixed;
an independent Rust AST comparison checks all 116 record field sets. This is
an inventory floor over `types.rs` and `starbase_cutover.rs`, not the whole venue.

Twenty-nine generic wire entry points pin parsed per-type case arrays against
frozen SHA-256 content expectations from the generators unchanged since
`1857224642`. The hash includes case labels, values, validity and case order;
it uses `serde_json::to_vec` of the parsed Value. This is independent of the
current generator's own loop lengths. Unrelated credential secret/base64
corpora and the eight capture-metadata corpora do not gain content pins here.

## Measured mutation controls

Stopped after the listed controls. No broader mutation exhaustiveness is implied.
All changed files were restored byte for byte. Each restored consuming target
passed, and the final six-target run passed after every mutation was restored.

| Base SHA | Mutation / call site | Named assertion or result | Exit |
| --- | --- | --- | ---: |
| `acf6761b4b` | Reconstruct inherited helper: omit `fields(value, raw)` but mint a helper-side count; `check_wire` → TICKER_EVENT | **SURVIVOR / FINDING:** `wire_ticker_event` passes with no explicit field-closure call | 0 |
| `acf6761b4b` | Omit only `fields(value, raw)` in the fixed helper; legacy consumer | `TICKER_EVENT: native identity assertion count` | 101 |
| `acf6761b4b` | Same omission; cutover consumer | `CoinbaseStarbaseCutoverEvidence: native identity assertion count` | 101 |
| `acf6761b4b` | Restore closure invocation, legacy / cutover | Both targeted tests pass | 0 / 0 |
| `acf6761b4b` | Replace invalid `__not_a_literal__` with same-length `__not_b_literal__` in generator; legacy / cutover | Both named `wire corpus content fingerprint` assertions | 101 / 101 |
| `acf6761b4b` | Restore corpus generator, legacy / cutover | Both targeted tests pass | 0 / 0 |
| `acf6761b4b` | Wrong native nested `CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem` decoder mapping | `decoded native enum variant`, inside `cutover_plan` | 101 |
| `acf6761b4b` | Restore nested decoder | `cutover_plan` passes | 0 |
| `a2e22301be` | Use padded URL_SAFE encoding | `native base64url bytes` | 101 |
| `a2e22301be` | Short ASN.1 length + 1 | `native ASN.1 tag/length/payload` and `native PEM DER bytes` | 101 |
| `a2e22301be` | Omit `portfolio_im_notional` recursive link | `native field coverage matches production declarations` | 101 |
| `c12cfabe5e` | String result capture calls live TS helper with `input + "!"`, retaining the pinned input | `native base64url string` | 101 |
| `c12cfabe5e` | Restore all controls | Six targets, 58 tests pass | 0 |

The survivor is a disclosed defect of the inherited counter shape, not a
survivor of the fixed omission control. No measured fixed-control survivor
was dropped from this record.

## Final gates at c12cfabe5e

| Gate | Measured exit | Observed output |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2` | 0 | Finished, zero errors |
| scoped `cargo test`, `--jobs 2`, six relevant targets | 0 | 58 passed; zero failed/ignored |
| `node sdk-rs/safe-cex-venue-coinbase/scripts/check-store-notification.mjs` | 0 | JSON: 156 callable rows, 156 declarations, no reachable notification chains |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Output text: 7,341 symbols, 512 files, zero updates/additions/orphans |

The six test targets are `oracle` (34), `cutover` (13), `identity_inventory` (1),
`signing_helpers` (3), `credentials` (4), and `public_frames` (3). All venue
oracle commands set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Only one Cargo process was launched at a time by this agent.

The first Store floor update at `a2e22301be` measured exit 1:
`ported callable corpus total`, 156 versus 154. `bdb248548c` froze 151;
`66ba0e1eb6` added two credential callables and this port adds three, so the
independent expected floor is 151 + 2 + 3 = 156. Corrected in `c12cfabe5e`.
The subsequent auditor exited 0 and emitted valid JSON. The temporary wrapper
itself then exited 1 because it looked for literal text `156/156`; parsing the
same recorded JSON verified both 156 counts and the empty chain array, without
rerunning the audit. Both tool and wrapper outcomes are preserved in the record.

Not run: workspace compilation, full monorepo or full Coinbase test suite,
host Clippy, wasm Clippy, layer audit, parity Rust-symbol checker, repository-wide
formatting, CI reruns, or historical bdb compilation. Sub-agent reviews were
read-only; their inspections are not reported as executed gates.
