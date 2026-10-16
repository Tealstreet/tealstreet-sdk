PR 7430 merge-down into feat/polymarket-spot-contracts, measured at
`7337f92335a5ef90dd61490fe1b680f06e976389`. Merge parents: `36ddbf6052cdab0b5c35aa560fc74ff0074bf8dc`
and requested integration `45ff4e3dd2941ae338dd783a6177e94d00eccce0`.

This requested merge moves **0 additional pending rows**; it preserves the six
previous pending→ported spot-contract rows plus incoming utility/REST mappings.
Venue census by Python 3.14 tomllib: 19 ported / 18 divergent /
104 pending (141 rows). No unreviewed branch was used as a merge parent.

Three conflicts were resolved: lib.rs includes both utility/public-read and spot
modules; exchange manifest retains all incoming rows plus the six spot rows and
uses the extractor serializer for the rust_files union; documentation retains
incoming guidance plus the authored spot-contract paragraphs. No method body was
changed in this merge. The extractor writer reported zero stale manifests.

| Gate at the merge SHA | Exit | Scope |
| --- | --- | --- |
| Cargo check | 0 | safe-cex-venue-polymarket, all targets, jobs 2, locked |
| Venue tests | 0 | 14 passed; unit/doctest targets contain zero tests |
| Scoped mapping | 0 | 141 rows, 44 public Rust items; no exemptions |
| Extractor check | 0 | Output read: Would update 0 manifests, 0 added, 0 orphaned |

All Cargo commands ran serially. Historical seven-control fault proof is bound
only to its original implementation SHA in SPOT_CONTRACTS_EVIDENCE.md; it was not
repeated during this merge-down. No host/wasm Clippy, workspace compilation,
monorepo suite, live API, CI rerun or human review was performed.
This following documentation-only commit is not a separately tested SHA.
Raw commands, output and exits: sdk-rs/target/polymarket-merge-down/7337f92335a5ef90dd61490fe1b680f06e976389/.
