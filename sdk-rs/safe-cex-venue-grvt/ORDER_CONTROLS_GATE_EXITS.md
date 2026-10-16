# GRVT order controls gate exits

Measured implementation SHA: `776a9397c93af0ec4f73a6b702b6ad16fe5a44c5`.
Actual parent: `f436fc816390a8b4556ab7a2fe0ae85072337036`, integration.
Fresh branch: `feat/rust-grvt-order-controls`; no unreviewed feature ancestry.
The evidence commit is distinct from the implementation SHA above.

Independent Python tomllib comparison of all 164 committed GRVT family rows
between that parent and implementation measured **7 pending -> ported,
1 pending -> divergent**. Counts: 33/42/89 -> 40/43/81
(ported/divergent/pending). This census stops at this family's manifests.
The seven ported methods are cancelOrder, cancelAllOrders, cancelSymbolOrders,
cancelOrders, placeOrders, listenOHLCV and listenOrderBook. updateOrder is CHOSEN
divergent: shared UpdateOrderOpts ignores unmodeled own keys, so its diagnostic
fieldCount counts represented keys. A separate actual-source/native witness
measures `{amount:2,extra:7}` as source 2 versus native 1. Keeping the shared
update model avoids a second raw bag solely for diagnostic key accounting.

All methods use shared native models and configured bound SDK/socket hooks.
They do not claim the SDK constructor, authentication endpoint implementation,
single-order placement/signing, or websocket implementation. Native builder
errors preserve the existing concrete variant; a spoofed name remains ordinary.
Safe replacement validates before cancellation and retains TIF, trigger price,
side, amount, reduceOnly, close and distinct stop/take-profit trigger fields.
Absent/null info retains the source property-access error before fresh wrapping.

## Gates measured at the implementation SHA

All cargo invocations used `--manifest-path sdk-rs/Cargo.toml --jobs 2`,
one cargo at a time. Node oracle tests set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Exit / observed outcome |
| --- | --- |
| `cargo check -p safe-cex-venue-grvt --all-targets` | 0, Finished dev profile |
| `cargo test -p safe-cex-venue-grvt --test order_controls`, Node 24.20.0 | 0, 9 passed |
| Same target with PATH selecting Node 26.7.0 | 0, 9 passed |
| Pinned Yarn JS `parity:extract:check` | 0, output says 0 manifests updated, 0 added, 0 orphaned |

Native assertions precede comparisons of source effect sequences. SDK payload
Value fields are read directly; typed replacement and callback fields are
asserted without encoding/re-decoding. There are no wire roundtrip comparisons.
Independent fixed expectations cover nine cancellation cases per method (27),
three cancel-batch, six placement-batch, eighteen update and two listener cases
per method (4), plus one explicit typed-key boundary. The source/native floor
stops after those 58 cases and that boundary; no complete enumeration is claimed.

## One compiling control per new row

Each row below compiled, failed its named native assertion with exit 101, then
was restored byte-for-byte and its exact target passed with exit 0.
Targets are in tests/order_controls.rs; mutations are in src/order_controls.rs.

| Row | Deliberate mutation | Named failed assertion |
| --- | --- | --- |
| cancelOrder | Send real order ID under client_order_id | CANCEL single native payload key |
| cancelAllOrders | Corrupt sub-account ID | CANCEL all native account |
| cancelSymbolOrders | Corrupt normalized instrument ID | CANCEL symbol native instrument |
| cancelOrders | Treat successes as failures | CANCEL batch native warning |
| placeOrders | Continue after native builder rejection | BATCH native builder variant |
| updateOrder | Add one to replacement amount | UPDATE native amount |
| listenOHLCV | Corrupt delegated symbol | LISTEN candle native symbol |
| listenOrderBook | Corrupt delegated symbol | LISTEN book native symbol |

Two earlier mutation attempts were invalid compile-only controls (exit 101,
unused-variable warnings denied): account/symbol replacement with constants.
They were restored and replaced with compiling corruptions above. They are not
counted as behavioral failures. No executed mutation survived.

Initial development test builds failed exit 101 on fixture/assertion issues
(temporary Vec borrows, format argument, missing required lastUpdate) and a
method-chain typo during the info correction. These were fixed before the
measured implementation SHA; the gates above are the committed corrected code.
Native sub-agent review approved that exact implementation SHA, resolving the
missing/null info finding and naming the typed-key boundary. Reviewer source
oracle exit 0 belongs to byte-identical precommit content, not a fresh SHA gate.

Not run: workspace builds, other crate checks, full test suites, wasm checks or
tests, host clippy, wasm clippy, CI reruns, or inherited Rust manifest-validator
diagnostics. Claims above are scoped host checks and bounded native assertions.

## Merge-down preservation

Fetched `origin feat/safe-cex-rust` and merged integration
`df065d29b58fd12bdc103e2ef156c8de602663a0` down without conflicts.
Merge SHA measured: `bb888d1ad31a09f67fc164a7b62fc3f83f0a6767`.
`git diff --exit-code 776a9397c93af0ec4f73a6b702b6ad16fe5a44c5 HEAD --
sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests` exited 0:
all gated GRVT source and tests are byte-unchanged. The source GRVT TypeScript
and shared types.ts comparison also produced no changed paths. The behavioral
exits remain attached to the original gate SHA and stand under the explicit
preservation rule; neither Node target nor the eight controls was rerun here.
Shared dependencies changed, so the scoped GRVT all-targets check was rerun at
the merge SHA: exit 0, Finished dev profile. Extractor check at that merge SHA:
exit 0, output says 0 manifests updated, 0 added, 0 orphaned.
Independent tomllib comparison against the fetched integration SHA reconfirmed
7 pending -> ported, 1 pending -> divergent, with 81 GRVT rows still pending.
This evidence-only follow-up does not change any gate input.
