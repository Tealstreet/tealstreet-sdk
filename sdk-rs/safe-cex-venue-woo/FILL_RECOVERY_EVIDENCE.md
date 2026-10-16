# Legacy WOO fill recovery and execution reports

Base: `c4caf76e6cdb22b858492ca47796fcb46a212d9e`.
Code: `e31ea4548d8e8d7d041c808245b6fccd5233e5dc`.
Branch: `feat/rust-woo-legacy-fill-events`.

Eight pending-to-ported rows, six in `woo.exchange.ts` and two in
`woo.ws-private.ts`: position signature, retry scheduling, refresh, optimistic
one-way fill application, fill synchronization, disposal, execution reports and
algo execution reports. No v3-owned file was edited by this slice.

The native bodies use compatible JS-value, captured-call, store/Map, timer,
imported-helper and locale-sort primitives. Full class/runtime wiring remains
separately pending. `ported` describes these method bodies, not a completed
venue adapter or every possible host implementation.

Python 3.11 `tomllib` independently compares the three owned manifests against
the base, asserts the fixed 101 row identities and exactly eight changes, all
pending-to-ported. Base: **54 ported / 20 divergent / 27 pending**.
Slice: **62 / 20 / 19**. Census stops at row 100.

## Native and actual-source floor

The Node oracle extracts the six actual exchange method ASTs, their actual two
static constants, the two actual private report handlers and their timestamp
helper. It loads the actual WOO constants/enums/utils, `v` helper and position-ID
helper. It does not replace those method algorithms with copied mocks.

Rust native assertions precede each family's wire comparisons and check values,
primitive types, floating-point results, UTF16 text, raw price/amount values,
original errors, mapper inputs/outputs, response arrays, retained entry-price
references, captured Map setter targets, callback identity, getter rereads,
live Map iteration, sparse inputs, omitted timestamps, independent counts and
side-effect order. These are not decode/re-encode comparisons.

The independently fixed corpus contains 53 cases: signature 3 (stopped local 2),
schedule 5 (4), refresh 8 (7), optimistic 10 (9), sync 5 (4), dispose 5 (4),
execution 9 (8), algo 8 (7). Global stop: 52. Size, family/index content and
independent native fixture inputs/results are asserted. Complete JS prototype,
array-method, locale, network and runtime coverage is not claimed.

## Review and development failures

Read-only sub-agent review found three mismatches: disposal must read its funding
timer again after resolving clearInterval; fill synchronization must capture the
Expected Map setter target before signature evaluation; retry refresh must ignore
the scheduler's return. All three were corrected and pinned with independent
actual-source/native fixtures. The scheduler fixture returns a non-undefined
sentinel. Reviewer source witnesses exited 0; reviewer ran no Cargo.

Development Cargo failures: exit 101 for a temporary borrowed Rc reference;
exit 101 for an unused fixture helper; exit 101 with 7/8 tests passing because an
independent execution-case-7 expectation omitted the source's closed-status update
before its invalid-fill guard. These were fixed, then the uncommitted scoped
target exited 0 with eight passed. They are not credited mutation controls.
A controller initially rejected a nonunique textual injection before writing or
running its target (exit 1); no runtime control is claimed for that refusal.

## Gates and discriminating controls

Measured results are appended below with their exact commit SHA. One Cargo
invocation at a time, jobs 2. All venue oracle runs use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The extractor writer ran after manifest edits, exit 0; output said Updated 0
manifests, 0 added, 0 orphaned. `rust_files` arrays were not manually spelled.

## Non-claims

Not run: workspace compilation, full crate or monorepo suite, host clippy, wasm
clippy, credentialed HTTP, live sockets, full adapter integration, v3 runtime
tests or CI reruns. No shared type or runtime trait was edited by this slice.

At code `e31ea4548d8e8d7d041c808245b6fccd5233e5dc`, command:
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_fill_recovery --jobs 2`.
Eight batched method mutations exited **101**, **0 passed / 8 failed**. First
rejections were native assertions, before wire comparisons:

| Mutation | Test | Named assertion | Local stop |
| --- | --- | --- | --- |
| Signature entryPrice replaced by contracts | signature_native | FILL native signature order and fields | 0 |
| Attempt-zero delay changed to 150 | schedule_native | FILL native retry delay | 0 |
| Reverse unchanged-signature retry decision | refresh_native | FILL native refresh retry and finally sequence | 0 |
| Reverse fill buy/sell sign | optimistic_native | FILL native optimistic side | 0 |
| Initial recovery attempt changed to one | sync_native | FILL native initial recovery attempt | 0 |
| Omit Flight Map clear | dispose_native | FILL native disposal sequence | 0 |
| Raw execution price replaced by quantity | execution_native | FILL native raw fill price | 1 |
| Raw algo price replaced by total quantity | algo_native | FILL native raw algo price | 2 |

Restore, same command: **0**, eight passed. Separate filtered control using
`refresh_native -- --exact`: forwarding Schedule's return exited **101** at
`FILL native refresh ignores schedule return`, local 1; restore **0**, one passed,
seven filtered.

Two corpus controls used the restored Cargo-built
`legacy_fill_recovery-0f8f653dd2b01557` binary with `signature_native --exact`.
Return 52/53 rows: **101**, `FILL corpus SIZE`; restore **0**, one passed,
seven filtered. Return 53 copies of row zero: **101**,
`FILL corpus CONTENT index iteration 1`; restore **0**, one passed,
seven filtered. The second corpus retains the independent expected length.

Credited runtime control floor: **11**, stopped control 10. No credited control
survived; no compiler-only failure is counted. All restores were byte-identical.
Measured records: `/tmp/woo-fill-controls.json` and
`/tmp/woo-fill-corpus-controls.json`, bound to the code SHA above.

## Merge and input preservation

Merged integration down at `a553ce92e78a0559194e953f569bf4fe42e6db8c`, producing
`63d05deccb171bf0214353eba4b42dc1d37ce69b`. No unreviewed feature branch was
added as an ancestor; the fresh base and subsequently merged integration are
reported separately. Own source/test inputs were byte-unchanged from the code
SHA: `git diff --exit-code` **0**. The expanded reachable-input comparison also
exited **0** for the root Cargo manifest, eight internal dependency packages,
owned WOO crate source/tests/Cargo manifest, WOO manifests, actual TS source and
conservative helper/logging/transport inputs listed in
`/tmp/woo-fill-preservation.json`.

All **481** reachable Cargo.lock package records compare equal, stopped record
480. The eight internal packages are candle-cache, crypto, exchange, num, store,
transport, types and venue-woo. Actual Node loading observed **66** external
module files, stopped 65; resolved paths and SHA256 contents compare equal after
merge. Node producers exited **0** and corpus bytes also compare equal. This is
input preservation, not a native assertion gate or proof of every module/OS/
toolchain input.

Restored source/test SHA256:

- `src/legacy_fill_recovery.rs`:
  `ccd0097108fc00aae9a75a835adb1cefb6eac237027dfdc7d2d7fcc44ec5a50a`
- `tests/legacy_fill_recovery_oracle.cjs`:
  `8c2ae91508b551d5e5887f9d674e95e0394554d0ac7d6517a0df533607d74dd9`

At merge `63d05deccb171bf0214353eba4b42dc1d37ce69b`:

| Named gate | Exit | Measured output |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2` | 0 | Finished dev profile, zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_fill_recovery --jobs 2` | 0 | Eight passed, zero failed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests, 0 added, 0 orphaned |
| Python 3.11 tomllib owned census | 0 | 62 / 20 / 19, eight pending-to-ported, fixed 101 row identities |
| `git diff --check` | 0 | No whitespace errors |

Logs: `/tmp/woo-fill-merge-check.log`, `/tmp/woo-fill-merge-test.log`,
`/tmp/woo-fill-merge-extract.log`, `/tmp/woo-fill-census.json`.
The delivery commit only adds this manually authored evidence sheet. The Cargo
and extractor gates above were executed at the merge SHA; they are not claimed
as fresh executions at the documentation commit.
