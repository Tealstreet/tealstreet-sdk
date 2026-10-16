# Crypto.com private order mapping slice

Three new rows move **pending → ported**, with zero new divergent rows:
`CryptocomPrivateWebSocket#mapOrderType`, `#mapOrderStatus`, and `#mapTimeInForce`.
The independent TOML census at `b065fc63567595a78659c3fce51067e04047c248`
is 25 ported / 18 divergent / 94 pending. See `post-merge-census.json`.
Its enumeration floor is six manifests / 137 rows, ending at
`cryptocom.ws-public.ts.toml` / `CryptocomPublicWebSocket.dispose`.
The eight websocket policy rows that landed during this turn belong to the
previous slice and are excluded from this slice's three-row movement.

## Commit and parent boundaries

- Fresh branch: `feat/cryptocom-private-order-mappings`, created from reviewed
  integration `89ad07ac80f6efc169430a2f94bd59898660e9f9`.
- Implementation commit: `da0d88aeff3c5db25ab92e5510992071c4942627`.
- First measured merge: `e1bd9d4e0b44e932727758acdb1a140f04294fcd`, merging
  integration `9c151409d125381f67d08d2b644afa89c77a79c8`.
- Final measured merge: `b065fc63567595a78659c3fce51067e04047c248`, merging
  integration `c54039795784efc67fbfa0bb7d0109e47440ee11`.
- The final merge leaves this crate's source, tests, Cargo configuration, the
  source venue, SDK types, and logging source byte-unchanged from `e1bd9d4e0b`;
  the exact paths and diff exit 0 are in `post-merge-comparison.json`.
  Shared dependencies changed, so the clean gates were rerun after the merge.
- No separate unreviewed feature branch was carried into the fresh branch.
  The subsequent evidence-only commit adds this directory; it does not change
  source, tests, manifests, or build configuration.

## Parity and native witness

The three stateless switches retain their case groups and defaults. Strings
are represented as UTF-16 code units, including unpaired surrogates. Every
non-string JS value shares the strict-switch default equivalence class,
represented by `None`; no coercion or receiver access occurs. The free-function
renames are explicit in the manifest. Class construction and consumers remain
pending; these three rows do not claim their completion.

`private_order_mappings_match_actual_typescript` executes the actual three
TypeScript arrow initializers against enums loaded from the complete actual
SDK types module. Source results are decoded directly into native Rust enum
variants independently of the Rust enums' wire codecs. Every native result
assertion precedes shape, input and enum-inventory assertions; no Rust result
is serialized for comparison. Independent expected group counts are 30 / 30 /
28, totaling an **88-case floor**, ending at `mapTimeInForce` case 27,
`proxy-trap`. This is a case floor, not exhaustive runtime enumeration.

The initial uncommitted experiment exited 101 because its independent enum
inventory accidentally omitted Creating/Updating/Canceling. All 88 native
comparisons preceded that failure and matched. The expected inventory was
corrected against the original SDK enum declaration before the implementation
commit. `initial-moving-tree-inventory-failure.log` preserves that experiment;
it is neither a SHA-bound gate nor a mutation control.

## Runtime discrimination

All three controls were measured at **e1bd9d4e0b44e932727758acdb1a140f04294fcd**
plus the individual documented wrong-result patch. Each compiled and failed
at a named native runtime assertion, then passed after restoring the exact
tracked source. Patches are recorded as JSON, and raw output and commands are
retained in `gates.json` and the corresponding logs.

| Control | Named assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| Market → Limit | `CRYPTOCOM-PRIVATE-MAPPING-NATIVE mapOrderType case0` | 101 | 0 |
| Closed → Canceled | `CRYPTOCOM-PRIVATE-MAPPING-NATIVE mapOrderStatus case2` | 101 | 0 |
| IOC → FOK | `CRYPTOCOM-PRIVATE-MAPPING-NATIVE mapTimeInForce case1` | 101 | 0 |

Each restored run reaches all 88 cases. There are no survivors among these
three controls. This fixed enumeration stops at the third control, time in
force; it does not claim exhaustive mutation coverage. Controls were not
rerun at the final merge; their measured exits remain bound to `e1bd9d4e0b`.
The final merge's clean native run is separately recorded.

## Review and limits

Native sub-agent `review_auth_witness` found no static blockers at `e1bd9d4e0b`:
case groups/defaults, UTF-16 handling, non-string equivalence, native assertion
ordering, independent inventory, manifest targets and merge preservation
support the three helper-level ported claims. It also inspected all three
raw runtime failures and restored passes, confirming the named assertions,
88-case floor and absence of survivors among these controls. The reviewer
did not run Cargo. The final merge leaves the reviewed crate source and tests
byte-unchanged; no review of unrelated integration changes is claimed.

Not run: full Crypto.com test suite, other crate suites, workspace compilation,
workspace suites, canonical parity index validation, wasm runtime tests,
live exchange tests, old slices' mutation controls, or CI. Host clippy and wasm
clippy are separate measured compilation/lint gates, not wasm execution proof.

## Measured gates

Exact commands, elapsed times and raw logs are in the two JSON ledgers.
All Cargo runs were sequential and scoped to `safe-cex-venue-cryptocom`,
with `--jobs 2`. Native witnesses use the supplied TypeScript module path.

### First merge — `e1bd9d4e0b44e932727758acdb1a140f04294fcd`

| Gate | Measured exit | Raw output |
| --- | --- | --- |
| native-baseline | 0 | [native-baseline.log](native-baseline.log) |
| type-mutation | 101 | [type-mutation.log](type-mutation.log) |
| type-restored | 0 | [type-restored.log](type-restored.log) |
| status-mutation | 101 | [status-mutation.log](status-mutation.log) |
| status-restored | 0 | [status-restored.log](status-restored.log) |
| tif-mutation | 101 | [tif-mutation.log](tif-mutation.log) |
| tif-restored | 0 | [tif-restored.log](tif-restored.log) |
| scoped-check | 0 | [scoped-check.log](scoped-check.log) |
| host-clippy | 0 | [host-clippy.log](host-clippy.log) |
| wasm-clippy | 0 | [wasm-clippy.log](wasm-clippy.log) |
| fmt | 0 | [fmt.log](fmt.log) |
| extractor-writer | 0 | [extractor-writer.log](extractor-writer.log) |
| extractor-check | 0 | [extractor-check.log](extractor-check.log) |

### Final merge — `b065fc63567595a78659c3fce51067e04047c248`

| Gate | Measured exit | Raw output |
| --- | --- | --- |
| native | 0 | [post-merge-native.log](post-merge-native.log) |
| check | 0 | [post-merge-check.log](post-merge-check.log) |
| host-clippy | 0 | [post-merge-host-clippy.log](post-merge-host-clippy.log) |
| wasm-clippy | 0 | [post-merge-wasm-clippy.log](post-merge-wasm-clippy.log) |
| fmt | 0 | [post-merge-fmt.log](post-merge-fmt.log) |
| writer | 0 | [post-merge-writer.log](post-merge-writer.log) |
| extractor | 0 | [post-merge-extractor.log](post-merge-extractor.log) |

Both writer outputs say “Updated 0 manifests; 0 added, 0 orphaned symbols.”
Both extractor checks say “Would update 0 manifests; 0 added, 0 orphaned symbols.”
These output texts were inspected in addition to the measured process exits.
