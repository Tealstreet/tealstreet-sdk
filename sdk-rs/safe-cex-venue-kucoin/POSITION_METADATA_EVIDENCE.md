# Futures position metadata getters

Moved **0 pending to ported, 2 pending to divergent** on this continuation.
Python 3.11 `tomllib` parsed all twelve KuCoin manifests: parent
`71607b4e6a4cbbecb83de926fd015f3c4c53b3bb` has 63 ported / 35 divergent /
93 pending; tested code `1a7a0551d86d280183688cf69837d9765bc8b4aa` has
63 / 37 / 91. The moved methods are getOrFetchPositionMode and getOrFetchMarginType.

Actual branch parent is the PR 7397 repair head `71607b4e6a`; its tested merge
parent is `94375aeaf7`, containing integration `eb0041f3fe`. **PR 7397 remained
open when checked during this slice: this branch inherits its unmerged work.**
The new implementation is committed separately on feat/kucoin-position-metadata-reads.
No integration merge or PR was created by this continuation.

Both wrappers await the actual shared BaseExchange position operation, select
the first Position and return its native enum. Empty results and absent
metadata reject with the source messages; dependency errors retain all native
ExchangeError metadata. Fetch tasks, caching and Store notifications belong
to core. No new venue task, write or transport/query API was introduced.
The CHOSEN manifest notes cite the shared UTF-8 declaration; a source
lone-surrogate error symbol and Rust ingress refusal are explicitly witnessed.

Fresh completed gates at the tested code SHA:

| Gate | Exit / result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0; zero errors |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test position_metadata` | 0; three tests passed, zero failed/ignored |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; inspected output: 7341 symbols / 512 source files, zero changed manifests, added or orphaned symbols |
| `rustfmt --edition 2024 --check` on lib.rs, position_metadata.rs and its test | 0 |

The extractor writer ran after manifest edits, exit 0 with zero updates.
The provenance array was appended through extractor parseManifest and
serializeManifest, preserving existing entries and annotations, then the
full writer ran again. No array was hand-spelled. The venue oracle package
script includes the new integration test target.

One separate compiling production control per newly implemented row changed
only that wrapper's positions.first() to positions.last(). Both ran the exact
live_metadata_selects_first_native_enum_and_preserves_missing_errors test:

- Position mode control: exit 101 at `KUCOIN-METADATA native position mode 0`,
  Some(OneWay) versus independently expected Some(Hedged).
- Margin type control: exit 101 at `KUCOIN-METADATA native margin type 0`,
  Some(Isolated) versus independently expected Some(Cross).
- Source restored byte-for-byte in finally after each control; the same exact
  test passed after each restoration, exit 0. Neither mutation survived.

All native enum/error assertions in the main corpus precede all wire
comparisons. Independent expectations cover eight cases, stopping at index 7;
one source process captures the complete corpus. A separate actual-core test
covers one shared successful fetch and one cached rejection, asserting one
fetch in each path and intact native dependency errors for both getters.
One lone-surrogate source case stops at index 0. These are floors, not exhaustive
acceptance claims. The live source oracle injects only the shared position
dependency; it does not claim a TypeScript core lifecycle oracle.

Initial check at `9f0aaa9289` exited 101 because the fixture accidentally called
DefaultStore's inherent update instead of Store::update. `68e0cce0a6` fixes it.
Static sub-agent review also found an unbound ordinary-method oracle export;
it was fixed before running the oracle gate. The reviewer confirmed that fix,
then verified the new rust_files provenance at `1a7a0551d8`, with no remaining
findings. Its review was static only, with no cargo or edits.

Not run for this continuation: complete venue oracle, workspace compilation,
host Clippy, wasm compilation/Clippy, Rust parity tests, or CI. The cancellation
repair's fresh gates are separately bound in MERGE_DOWN_EVIDENCE.md; they are
not relabeled as metadata-head results. The following evidence-only commit
changes documentation, not the named tested code.
