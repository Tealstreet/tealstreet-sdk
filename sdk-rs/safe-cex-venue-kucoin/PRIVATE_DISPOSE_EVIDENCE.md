# KuCoin private websocket disposal parity

Two rows moved pending -> ported; zero divergent rows added. Independent
Python 3.11 tomllib census visited all 12 KuCoin manifests, stopping at
`kucoinspot.ws-public.ts.toml`: parent 71 ported / 39 divergent / 81 pending;
measured merge head 73 / 39 / 79. Counts are census, not behavioral proof.

Branch `feat/okx-kucoin-state-transitions` was created directly from integration
`e9b4202fc66f3f4ae6bc6ec7f78f09f27795c5d5`, with no older feature branch ancestor.
Source/control SHA: `bb423a3c6e996f7394d684d24996bbe8b555ac6f`.
Mapping/oracle wiring correction: `eeceb6dd3512263daf2ec693cf2e4ccd9ec8ff6d`.
Integration `9c151409d125381f67d08d2b644afa89c77a79c8` was merged at measured SHA
`7010ee4f4889314323cc631bb63afc5925d8c555`.

`KucoinPrivateWebsocket.dispose` maps to `dispose_futures_private` and
`KucoinSpotPrivateWebsocket.dispose` to `dispose_spot_private`, both in
`src/private_dispose.rs`. These are the local maintained method bodies with
injected truthiness, clearTimeout, and base-disposal callbacks. Handle Clone
must preserve primitive values or opaque reference identity. Truthy slots are
cleared only after successful cancellation; falsey slots remain. Spot reads the
balance slot after the orders callback. Cancellation failures propagate the
original error and skip later work. Owning classes and timer creation remain
pending; this slice does not claim their wiring or core query ABI parity.

## Gates actually measured

Commands ran sequentially at `7010ee4f4889314323cc631bb63afc5925d8c555`, with
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test private_dispose` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 |

Extractor text: `Would update 0 manifests; 0 added, 0 orphaned symbols`, 7341
symbols / 512 source files. Writer runs after manifest edits also reported zero
updates/additions/orphans. The existing explicit `venue-oracle:kucoin` script
now includes `--test private_dispose`; that whole venue command was NOT run.

`private_dispose_native_slots_before_source` asserts native call order, live
timer slots during cancellation, final slots, opaque handle Rc identity,
rejection and complete error/Rc identity before any TypeScript comparison.
Independent floor: 12 futures profiles plus 64 spot profiles = 76; stopping
iteration spot / object orders / object balance / super failure. The four slot
descriptors are undefined, zero, number, and object. Extra paired native/source
witnesses cover replacement on success, replacement retained on cancellation
throw for both venues, a callback replacing the later balance timer, and falsey
NaN retention. These are finite floors, not complete timer-domain enumeration.

## Compiling controls

At `bb423a3c6e996f7394d684d24996bbe8b555ac6f`, separately remove futures timer
clearing and spot orders timer clearing. Each exact focused test invocation
exited 101 at `KUCOIN-DISPOSE native orders slot`. Each mutation compiled;
each byte-restored rerun of the identical command exited 0. Zero survivors
among these two controls; untried controls carry no proof claim.

Post-control scoped diff against `bb423a3c6e` over both venues' src/tests and
KuCoin scripts exited 0. Post-merge scoped diff against `eeceb6dd35` over both
venues' src/tests/scripts and manifests exited 0; package.json also compared
byte-identically with exit 0. Merge-head gates above are fresh measurements;
the earlier control results stay attached to their original SHA.

Sub-agent static review found the path-prefix and missing oracle-list issues,
then verified both fixes and reported no remaining static blockers. It did not
run gates. No workspace build, full venue suite, host/wasm clippy, CI rerun,
network cancellation, or concrete websocket-class lifecycle was tested.

## Subsequent integration merge

Pre-push integration fetch was `ccc6b553a45033e903488df180dd9ac1523b4edb`, merged
at `d8b11d57fa20cf4496a79e3509ff00ba1403e412`. The other parent was
`d990857e24527c7883e426788f4e57766fabf48c`. Shared query/number changes and
existing venue API/test updates arrived from integration; no unmerged feature
branch was adopted. The whole-venue preservation check against `eeceb6dd35`
exited 1 because these existing files changed. A narrow exact diff of the six
new Rust/test/oracle files against `bb423a3c6e` exited 0; a separate exact diff
of the three edited manifests and package.json against `eeceb6dd35` exited 0.
The 12-manifest census at this SHA remains 73 ported / 39 divergent / 79 pending.
Older control exits are not measurements of the second merge head.

Fresh measurements at `d8b11d57fa20cf4496a79e3509ff00ba1403e412`: the exact
KuCoin cargo check, focused private_dispose cargo test, and extractor commands
listed above each exited 0. Extractor output again reported zero updates,
additions, and orphans. OKX check and state_transitions test also freshly exited
0 at this SHA, recorded in `../safe-cex-venue-okx/STATE_TRANSITIONS_EVIDENCE.md`.
The delivery commit only updates these two manual receipts. Gates belong to
the measured SHA; delivery byte identity excludes only those two receipts.
