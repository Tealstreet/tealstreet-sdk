Moved 1 row pending→ported and 4 pending→divergent in this slice.

All measured gate exits below cover code commit
`abfd8d1c034954a7c97ca124e2b5bbd11416bdd8`, including the independent Boolean
content guard committed at `9155817b15bbf6420a8af59ae0b7e9d470d2a60f`.
The evidence-only commit containing this file changes no tested code/manifests.
Exact commands, mutations, restore hashes and exits are retained in
[ACCOUNT_METHODS_GATE_EXITS.json](ACCOUNT_METHODS_GATE_EXITS.json).

Work began on inherited HEAD `5268ac300e189a55b90c84bcf026a360f63510e9`,
with unfinished account-info implementation/tests and no detected leftover
mutation. The immediate parent of the account implementation commit is the
Boolean guard commit `9155817b15`. This branch is not based on the specified
integration head `2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`.
Its common ancestor with that head is `115f2f1c82077057764e859d6980d6c955a32315`.
**Ancestry warning:** 11 pre-existing commits are absent from that integration
head; their review/PR status was not established. This slice does not certify
their gates. No history rewrite or integration merge was performed.

Python 3.14 tomllib parsed all six Backpack manifests at each immutable SHA:

| Reference | Ported | Divergent | Pending | Total |
| --- | ---: | ---: | ---: | ---: |
| Integration `2bc83d78d4` | 8 | 35 | 104 | 147 |
| Inherited parent `5268ac300e` | 9 | 39 | 99 | 147 |
| Tested code `abfd8d1c03` | 10 | 43 | 94 | 147 |

This slice maps getPositionMode (ported), getAccount, fetchAccountInfo,
changePositionMode and changeMarginType (four CHOSEN divergences). The latter
use configured host callbacks and shared error/future/prepared-value boundaries;
manifest notes name the exclusions. Relative to integration, including inherited
commits, the branch moves 2 pending→ported and 8 pending→divergent. These counts
are a census, not proof of the entire venue.

Measured gates:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: exit 0.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test account_reads --test types`: exit 0; four account tests and one type test passed after all controls/restorations.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit 0; inspected output reports 7,341 symbols / 512 source files and zero changed manifests, added symbols or orphaned symbols. The writer was run before the code commit: exit 0, two Backpack manifest formatting/reconciliation updates, zero added/orphaned rows. The check was measured at the code SHA above.
- Sub-agent review: read-only source/diff review found no concrete bugs within scope; it ran no Cargo or mutation controls.

One production control per newly mapped row, plus the two specifically owed
corpus controls, stopped after the following seven iterations. Every failure
was a real test panic with the named assertion, not a compile failure. Every
restoration was byte-identical to the code commit and reran the targeted test.

| Control | Observed assertion | Mutated exit | Restored exit |
| --- | --- | ---: | ---: |
| leverage-state | `ACCOUNT-READ native leverage bits case 0` | 101 | 0 |
| account-truthiness | `ACCOUNT-IDENTITY native user id 0` | 101 | 0 |
| position-mode | `POSITION-MODE native enum` | 101 | 0 |
| position-change-message | `UNSUPPORTED-MODE native event/log trace 0` | 101 | 0 |
| margin-change-message | `UNSUPPORTED-MODE native event/log trace 6` | 101 | 0 |
| one-hot-deletion | `TYPES-CARDINALITY source cases` | 101 | 0 |
| one-hot-homogeneous | `TYPES-CONTENT independent one-hot BackpackAsset depositEnabled field depositEnabled` | 101 | 0 |

No survivors in those seven controls. No exhaustive mutation coverage is claimed.
The one-hot deletion control reduced total cases from the independently pinned
1,545 to 1,528. The homogeneous control set every Boolean supplement field false
while preserving 1,545 cases and all labels/admissions; it failed the independent
one-hot content check. The guard independently pins 17 unique supplement identities
across ten records, compares raw and source-native values against the fixed
Boolean inventory, then checks decoded native fields on routes 0, 1 and 2 before
wire assertions. The previous symmetric-serde swap was not rerun in this slice.

Completed fixture enumeration stopped at leverage case 97, identity case 51,
unsupported-mode case 11, one native position-mode result, the one deferred
leverage read and string-boundary case 1. The type corpus reached case 1,544 on
all three ingress routes (318 accepted / 1,227 refused per route, 954 total wire
comparisons). These are fixture floors, not an enumeration of all host values.
Oracle metadata was Node v24.20.0, arm64, darwin. Account assertions probe native
enum variants, prepared-value variants/fields and IEEE bits; no re-encoded-byte
comparison substitutes for those assertions.

Not run: workspace compilation, full monorepo suites, host Clippy, wasm Clippy,
other Backpack test targets, live REST requests, browser/wasm execution, the full
venue constructor/createAPI, or WebSocket integration. No PR was opened and
nothing was merged to integration.
