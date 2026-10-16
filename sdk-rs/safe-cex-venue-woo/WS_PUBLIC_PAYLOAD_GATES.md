# WOO V3 public payload slice evidence

Branch: feat/woo-v3-public-payloads. Fresh base: 1273115ab915ad59ed9b478c48ca6356d8ac03f0 (1273115ab9).
Actual integration merge-down parent: 0650e658b488165c0a49081aecce3114a35e2861 (0650e658b4).
Measured source/test/manifest SHA: **b381ae3819bbc3fa80fa58e8152d07a5a45e5efb**, the integration merge commit.
This sheet/JSON are added by a later evidence-only commit; the following exits were measured at the
source SHA, not that later documentation SHA. No unrelated unreviewed lane branch was used as an
ancestor; our own source commit fd4f324632 remains subject to the user's PR review.

## Gates actually executed

| Gate | Exit at b381ae3819bbc3fa80fa58e8152d07a5a45e5efb |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with the same package/all-targets/jobs/offline/locked scope | 0; 36 observed targets / 92 passed / 0 failed / 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output says 7341 symbols across 512 files; Would update 0 manifests, 0 added, 0 orphaned |
| python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows and 257 indexed public Rust items |
| rustfmt --edition 2024 --check src/v3_ws_public.rs tests/ws_public.rs | 0 |
| python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-public-controls.py | 0; three compiling named failures and three restored passes |
| git diff --exit-code after controls | 0; exact source restoration before adding evidence |

Cargo commands ran serially. Tests/controls used
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
The yarn output text was inspected; success is not inferred solely from its process exit.
parity:extract was used as the manifest writer after mapping edits, with rust_files derived from the
actual rs targets by tomllib. The target audit indexes only WOO with 22 selected WOO/journal manifests;
sibling dependency crates are symlinked for resolution. It is not a full-workspace parity audit.

## Native floors and source evidence

The oracle executes the entire actual TypeScript contracts/WebSocket modules and exports the actual
private decodeLevels/assertPayloadSymbol/decodePublicPayload helpers. No helper is rewritten.
It independently pins required source declaration field names and types, the readonly tuple arity,
and the nine source union members. Declaration fixtures deliberately use distinct textual values
and numeric fields including -0 and infinity, retaining Number observations before JSON projection.
All Rust assertions inspect native fields, enum variants, tuple slots and numeric bits directly;
there are no new Rust re-encoded-byte comparisons.

Fixed walks stop at public topic kind 12, payload case 46, level case 30, symbol case 6 and record
fixture 7; all nine union variants are checked. The declared tuple's two slots are independently
asserted. Malformed declarations reject arrays/null/empty/missing fields; interval literals are strict.
These walks are finite floors, not exhaustive runtime enumeration. Operation outputs and declaration
union decoding are independently inspected against the source values. Multiply-invalid fixtures see
which error comes first, including symbol before branch fields, timestamps before levels, asks before
bids, trade source before price and side before RPI, ticker volume before count, and kline interval
before time and end-time comparison before OHLC. Every successful source payload and its nested book
arrays/tuples are observed frozen; native owned values are the disclosed adaptation.

## Discrimination proven once per new family

| Family | Compiling production mutation | Named failed assertion | Mutated exit | Exact-byte restored exit |
| --- | --- | --- | --- | --- |
| Declaration codecs | symmetrically swap ticker open/close Serde names | WS-PUBLIC native ticker open | 101 | 0 |
| Book levels | return quantity before price | WS-PUBLIC native level price | 101 | 0 |
| Public dispatch | BBO ask price reads bp instead of ap | WS-PUBLIC native quote ask price | 101 | 0 |

Controls ran as named overlays on b381ae3819bbc3fa80fa58e8152d07a5a45e5efb; each compiled, failed its named native assertion, restored
exact saved bytes, then passed all three ws_public tests. Stopped after mutation iteration 3, with
zero survivors in that floor. No further matrix or per-symbol mutation enumeration is claimed.
WS_PUBLIC_PAYLOAD_GATES.json preserves actual commands, call sites, patches, assertions and excerpts.

The native sub-agent review checked the twelve branch orders, shared numeric capture before untagged
Serde buffering, exhaustive nested output registrations, source AST assertions, direct native checks
and real targets. It caught generated helper-name mismatches, corrected before committing/compiling.
It ran the updated source oracle (exit 0); it did not run Rust gates, mutations or the target audit.

## Independent census and per-row justification

Commands: python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-public-census.py 0650e658b488165c0a49081aecce3114a35e2861,
and the same script against 1273115ab915ad59ed9b478c48ca6356d8ac03f0.
Python tomllib reads the 22 WOO/journal manifests, independently asserts the known 534-row inventory
and separately enumerated twelve-symbol expectation. Fresh base and merged integration parent have
the same WOO counts.
**12 pending rows moved: 0 ported / 12 divergent.**
Before: 102 ported / 84 divergent / 348 pending. After: 102 / 96 / 336.
Each new row has a distinct canonical CHOSEN note naming its own v3_ws_public.rs target and explaining
that symbol's actual adaptation; the scoped index confirms all twelve targets exist. The JSON preserves
all changed rows, targets and twelve distinct notes.

The eight declarations are the book tuple/update/snapshot, ticker, kline, price, funding-rate and
open-interest shapes. Public payload union plus the level/symbol/dispatcher operations complete the
slice. Native record/vector ownership replaces Readonly/frozen objects. Record declaration numbers
remain unrestricted f64 with retained ingress; dispatch operations separately enforce source integer,
decimal and cross-field constraints. The union chooses the first required structural shape in source
union order for ambiguous declaration inputs; dispatch selects its native variant by topic kind and
adds no wire discriminant. Retained JSON excludes host prototypes/accessors/executable/sparse values
and lone UTF-16 surrogates, and Result messages replace Error identity. The shared native descriptor
enum also carries private descriptors; public helpers refuse those outside the source public Extract
domain. These helpers do not register live sockets, mutate the store or activate subscriptions.
No HOW row movement is claimed.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW
tests, host clippy, wasm clippy, wasm build, doctests or CI reruns. No exits are claimed for those.
