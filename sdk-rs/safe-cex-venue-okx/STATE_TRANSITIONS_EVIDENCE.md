# OKX account projection and warning parity

Four rows moved pending -> ported; zero divergent rows added. The independent
Python 3.11 tomllib census visited all 12 OKX manifests, stopping at
`okxeudemo.exchange.ts.toml`: parent 51 ported / 19 divergent / 93 pending;
measured merge head 55 / 19 / 89. This census is separate from behavioral proof.

Fresh branch: `feat/okx-kucoin-state-transitions`, based on integration
`e9b4202fc66f3f4ae6bc6ec7f78f09f27795c5d5`. No earlier feature branch was inherited.
Implementation commits: `6609f69582`, `bb423a3c6e`; mapping/gate wiring correction
`eeceb6dd3512263daf2ec693cf2e4ccd9ec8ff6d`.
Merged integration `9c151409d125381f67d08d2b644afa89c77a79c8` at measured code SHA
`7010ee4f4889314323cc631bb63afc5925d8c555`.

The mappings are `fetchAccountMarketOverlay` -> `fetch_account_market_overlay`,
`getOrFetchPositionMode` -> `get_or_fetch_position_mode`,
`getOrFetchAccountLevel` -> `get_or_fetch_account_level`, and
`changeMarginType` -> `change_margin_type`, all in `src/state_transitions.rs`.
The async configuration dependency and warning callback are injected. The
configuration fetch/cache method and owning exchange class remain pending.
The warning method omits parameters that its maintained body never reads.

## Gates actually measured

All commands below ran sequentially at
`7010ee4f4889314323cc631bb63afc5925d8c555`, with oracle environment
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test state_transitions` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 |

The extractor output said `Would update 0 manifests; 0 added, 0 orphaned symbols`,
with 7341 symbols across 512 source files. The writer was also run after each
manifest edit; both runs reported zero updates/additions/orphans. Mappings and
provenance were serialized programmatically, not spelled as arrays by hand.

`state_transitions_native_before_source` asserts native position modes, f64 bits
(including signed zero, NaN, infinities), forwarded force, pending-await state,
complete errors and Rc identity, explicit null, and warning text/identity BEFORE
calling the live TypeScript oracle. The declared floor is 144 projection cases:
2 methods x 2 position modes x 6 numeric values x 3 force inputs x 2 outcomes.
Enumeration stopped at account level / Hedged / negative infinity / true /
rejection. Two warning cases and one null result are additional witnesses.
These finite inputs do not claim exhaustive runtime input coverage.

## Compiling controls

Each control was measured against source SHA
`bb423a3c6e996f7394d684d24996bbe8b555ac6f`, restored byte-for-byte in a finally
block, and rerun using the same exact focused test command above. None failed
compilation. Every mutated exit was 101 and every restored exit was 0.

| Row / deliberate production mutation | Native assertion that failed |
| --- | --- |
| Overlay: null -> false | `OKX-STATE native overlay explicit null` |
| Position mode: invert forwarded force | `OKX-STATE native force mode/None` |
| Account level: add 1 to returned level | `OKX-STATE native level bits` |
| Margin warning: cross -> isolated | `OKX-STATE native warning text` |

Zero survivors among these four controls. No claim about untried mutations.
After controls, scoped `git diff --exit-code bb423a3c6e --` over both venue
src/tests and KuCoin scripts measured 0. After merge, a second scoped diff
against `eeceb6dd35` over both venue src/tests/scripts, all their manifests, and
package.json measured 0. Fresh gates above were measured at the merge SHA;
the older control exits were not relabeled as merge-head measurements.

Sub-agent static review found and verified fixes for workspace-relative mapping
paths and KuCoin's explicit oracle target list. It reported no remaining static
blocker; it did not run gates. No workspace compilation, complete venue suite,
host clippy, wasm clippy, CI run, or venue network test was run in this slice.

## Subsequent integration merge

The pre-push fetch found integration
`ccc6b553a45033e903488df180dd9ac1523b4edb`. It merged at
`d8b11d57fa20cf4496a79e3509ff00ba1403e412`, whose other parent is receipt commit
`d990857e24527c7883e426788f4e57766fabf48c`. This brought shared exchange/number/
transport changes, KuCoin API changes, and existing test updates. No foreign
unmerged feature branch was used as a merge parent.

The broad whole-venue src/tests/scripts/manifests preservation check against
`eeceb6dd35` now exits 1 due to incoming integration changes. Narrow scoped
`git diff --exit-code bb423a3c6e --` over the six new source/test/oracle files
exits 0. The analogous check against `eeceb6dd35` over the three edited manifests
and package.json exits 0. No broad byte-unchanged claim covers this second merge.
The independent 12-manifest census at this SHA remains OKX 55 / 19 / 89 and
KuCoin 73 / 39 / 79. The earlier mutation results retain their original SHA.

Fresh measurements at `d8b11d57fa20cf4496a79e3509ff00ba1403e412`: the exact
OKX cargo check, focused state_transitions cargo test, and extractor commands
listed above each exited 0. The same fresh KuCoin check/test exits are recorded
in `../safe-cex-venue-kucoin/PRIVATE_DISPOSE_EVIDENCE.md`. Extractor text again
said zero updates/additions/orphans. No control was rerun at this second merge.
The delivery commit after this SHA changes only these two manual evidence files;
all other tracked bytes must compare equal using a diff excluding those files.
