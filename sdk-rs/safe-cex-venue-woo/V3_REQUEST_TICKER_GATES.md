# V3 request / ticker slice

Moved **4 pending → ported and 8 pending → divergent** (12 rows).
Tomllib compares committed candidate `b828e59f51ad5d93479cdee5b6e28150244eedfa`
with integration `45ff4e3dd2`: WOO exchange manifests plus WOO journal, 22 files,
534 rows. Counts move from **92 / 39 / 403** to **96 / 47 / 391**.
The keyed census and actual Rust declarations confirm all twelve new targets
exist; `/tmp/woo-v3-census.json` records the exact transitions.

Actual branch base: `0e5021eab698a61809bdab56bd3eebe44c24cb0c`, created before the
later `45ff4e3dd2` instruction. Implementation candidate
`b561b483a8c6f533ac57a3aebe971ebac2083477` builds directly on that integration
base; annotation/test-strengthening candidate `b828e59f51` builds on it. No
unreviewed feature branch was introduced as an ancestor. The later publication
commit changes this evidence file only; measurements below retain their own SHAs.

## Native and source scope

Seven public records have shared numeric capture and private map-only Serde
companions. Native assertions cover every declared field, including nested
futures stats/BBO/Trade fields, optional absence, signed zero, literal 0/1 source,
and overflow. No decode/encode roundtrip is used as a native assertion.
The source/native operation corpus stops at seven query cases, 23 preparations,
ten normalizations, fifteen positive-price checks, and fourteen ticker cases:
69 authored operation cases. Source capture executes the actual TS declarations,
actual installed qs/HMAC and actual futures-stat mapper; it owns no transport.
One HEAD case witnesses an explicit typed output refusal, so the preparation
partition is 22 matched cases and one declared refusal, not 23 parity successes.
All migration gates remain untouched.

Frozen source descriptors/header maps and ticker/info objects contrast with
owned mutable native records. Source BBO/Trade info retains live referents;
native info copies them. Query input uses opaque prepared JSON instead of the
full scalar/array/undefined host union. Public Trade side/source construction is
wider than its validated decoder. The request entry uses asynchronous shared
crypto and message-string errors rather than JS synchronous throws/concrete
exception classes. These are declared CHOSEN boundaries, not forced limitations.
Native header keys/counts/order use independent 0/1/3/4 expectations, including
complete key/timestamp/signature/content-type values before wire projection.

## Three compiling family controls

Measured code SHA: `b561b483a8c6f533ac57a3aebe971ebac2083477`.
One control per family, iteration one, stopped after these three controls:

| Family | Production change | Exit | Named native failure |
| --- | --- | --- | --- |
| Request | Repeat array queries become indexed queries | 101 | QUERY native serialized value |
| Ticker | Bid reads BBO ask instead of BBO bid | 101 | COMPOSE native bid |
| Quotes | Symmetric askPrice/bidPrice Serde renames | 101 | BBO native ask price |

Each compiled and reached its assertion; no survivor in this three-control floor.
Source was restored between mutations. Restored focused target exits 0: **5
passed, 0 failed, 0 ignored**. Exact command:
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo
--test v3_request_ticker --jobs 2 --offline --locked`, with the prescribed
SAFE_CEX_TYPESCRIPT_PATH. `/tmp/woo-v3-controls.json` binds exits to the code SHA;
per-family logs and restored output live under `/tmp/woo-v3-control-*.log`.
The later header-proof strengthening does not relabel these mutation runs as
newly executed controls.

## Candidate gates

Measured code/manifest SHA: `b828e59f51ad5d93479cdee5b6e28150244eedfa`.

- Scoped WOO `cargo check --manifest-path sdk-rs/Cargo.toml
  -p safe-cex-venue-woo --all-targets --jobs 2`: **exit 0**, no errors.
- Real Yarn `parity:extract:check`: **exit 0**; output explicitly reports
  **0 updates, 0 additions, 0 orphans**. Writer ran after edits and reconciled
  three manifests. An earlier invalid `wire` layer was rejected (exit 1) and
  corrected to `adapted`, without changing the gate.
- Changed Rust file rustfmt check: **exit 0**.
- Clean committed WOO all-targets tests: **exit 0**, all named targets passed,
  including the five focused tests, with no failures or ignored tests. Exact
  command: `cargo test --manifest-path sdk-rs/Cargo.toml
  -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked`, with the
  prescribed SAFE_CEX_TYPESCRIPT_PATH. Measured at the candidate SHA above.
  `/tmp/woo-pages-v3-committed-tests.json`, `/tmp/woo-pages-v3-check.json`,
  and `/tmp/woo-pages-v3-extractor.json` retain SHA/command/exit/log provenance.

The initial full WOO development test run exited 0 but began with uncommitted
header-test/annotation edits; it is not claimed as a clean SHA-bound gate. The
clean committed rerun resolves that provenance gap. Sub-agent review found the
independent header-count/order gap; it was corrected before the clean gate.
No host/wasm Clippy, wasm build, full workspace check, full parity suite, CI,
WOO runtime registration or HOW runtime gate was run.
