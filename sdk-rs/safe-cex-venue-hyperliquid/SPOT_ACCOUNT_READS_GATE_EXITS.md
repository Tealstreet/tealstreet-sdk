# Spot account and read lifecycle gates

Fresh branch parent: `00cafd8f46790c3e283decca55b6618bf8ea4d97`.
Implementation: `3d45126701b5f564b318210abd29d740995baa91`.
Corrected native ordinal/control base: `2655efba4f9253b923cd5d3c842f29500c63b82e`.
Restored merge/gate SHA: `f3c7e0ff357769dd79a5c0b0cb14333818bb614a`, built on
actual integration merge parent `4d4b43c2bfc2ef7649b3a24cb658f9003525bec5`.
No unrelated unreviewed feature branch was used as an ancestor.

Own transition: nine pending→ported, zero→divergent. Python3.14 tomllib compared
per-symbol statuses independently with the fresh parent across all nine assigned
manifests: `(136,0,25)` → `(145,0,16)` in ported/divergent/pending order.
Including the already-resolved constants/errors files: `(151,0,16)`.
Whole Hyperliquid at the restored gate SHA: `(372,106,53)`.
Only our spot exchange manifest was edited. Perp-core source/manifests were
inherited via integration, not edited by this slice. Class/constructor, market
fetch, order placement/cancellation/update remain pending in the spot exchange.

## Ported bodies and finite proof

Nine source bodies: validateAccount, start, tick, fetchSpotClearinghouseState,
refreshSpotBalance, fetchOHLCV, fetchOrders, generateFakePositionsFromMarkets,
fetchPositions. Owned callbacks map candles and fake positions, filter original
position rows and invoke the live tick method through a captured exchange.

Async exports enter eager retained HelperTaskHost tasks; native async return
resolution/adoption remains mandatory. The refresh child allocates a native
promise, enters its prefix immediately and survives discarded observers. Forced
refresh time is written before in-flight lookup; in-flight is re-read on return.
The catch's logging errors propagate through finally clearing. A synchronous
throwing replacement fetch can finish finally before the source assigns the
already-settled child promise; the witness preserves that source behavior.

Start captures publicDataOnly after external initialization, keeps live store
reads, preserves ordered update fields/loaded spread and websocket ordering,
and primes balances before logging and tick. Existing positions bypass symbols
and copy the container while retaining original rows. Generated positions keep
the source Long-id/Short-side behavior and configuration overwrite order.
Candles preserve repeated option reads, date/limit request construction, wrapped
response selection, parameter destructuring, native parseFloat and own-field
order. Empty orders allocate a fresh native array on every call.

Oracle runs the unchanged complete spot exchange module with explicit imported
base/API/socket/validator/loop dependencies. It loads actual enum declarations,
logging enum, endpoint declaration, unchanged createPositionId and actual dayjs.
Six hashed inputs: five project source files plus the installed dayjs module.
No body rewriting. Native primitive/library implementations are bindings, not
an independently proved JS engine or general calendar implementation.

Independent source floor62, stopped at index61 (`ohlcv/nan-limit`):6 validation,
6 state reads,11 refresh,7 start,6 tick,12 candles,3 orders,5 fake-position,
6 position-query cases. Ten tests include these nine families and a discarded
refresh observer witness. Native variants, UTF-16, finite binary64 bits, NaN
class, own-field order, original handles, explicit rejection state, eager prefixes,
ordered effects and stored times/promise state are asserted. There is no encoding
of Rust results for comparison. Native stringify input is asserted before the
error log containing its string; no transport wire equality claim.

The floor covers concurrent shared inner/distinct outer promises, cooldowns,
post-await disposal, native throw-undefined, synchronous child throw, throwing
logger/emitter, wrapped/null/malformed candle responses, capped/fractional/NaN
limits, unpaired UTF-16 symbols, fresh order containers, changed store positions,
configuration spreads and exact generated-row ordinals. Firing the retained tick
callback after replacing exchange.tick proves live lookup, lexical owner and
exact returned native promise identity. Dropping a refresh observer retains
balance delivery/finally cleanup. Arbitrary getters/proxies/coercions, sparse or
custom iterators/IteratorClose, general thenables and microtask interleavings,
calendar/DST/month rules, unhandled rejection integration and live networking
are outside this finite witness. Virtual pending dependencies remain bindings.

## Isolated discriminating controls at the corrected base

Transient source/native-adapter controls were compiled once with:
`cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test spot_account_reads --no-run --jobs 2`:
exit0. Each selector ran alone using Node24.20.0 and the TypeScript path:
`SAFE_CEX_SPOT_ACCOUNT_MUTATION=NAME
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
sdk-rs/target/debug/deps/spot_account_reads-de74ed391ec9614e TEST
--exact --nocapture`. All ten exited101 at named native assertions:

| Broken behavior | Test | Named assertion |
| --- | --- | --- |
| Successful validation has wrong error text | account_validation | ACCOUNT native validate valid |
| Wrong state request type | spot_state_read | ACCOUNT native eager prefix state vault |
| Refresh skips finally clearing | balance_refresh | ACCOUNT native refresh ordinary |
| Startup loaded flags false | account_start | ACCOUNT native start internal |
| Retained tick callback does nothing | account_tick | ACCOUNT native tick fire-replacement |
| Candle low uses close | candle_reads | ACCOUNT native ohlcv direct |
| Native array binding reuses orders container | empty_orders | ACCOUNT native orders 0 |
| Fake position side Long | fake_positions | ACCOUNT native fake one |
| Filter binding duplicates first generated row | position_reads | ACCOUNT native positions two |
| Filter binding clones a generated row | position_reads | ACCOUNT native positions generated |

The last two controls isolate identical-field alias defects: ordinals `[0,1]`
versus `[0,0]` and `[-1]`. Native binding controls test allocation/identity at the
actual call sites; they are not claims that every host primitive was mutated.
No survivors. Both instrumented files were restored; `git diff --exit-code`
against the corrected base exited0. No selector/control remains in src/tests.
Both readonly sub-agent reviews found callback/alias coverage gaps; corrections
were reviewed at the immutable corrected base. Reviewers ran no gates.

## Positive gates at the restored merge SHA

`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test spot_account_reads --jobs 2`:
Node24.20.0, exit0,10/10 passed after restoration. Node26.7.0 ran the same rebuilt
executable with `PATH=/opt/homebrew/opt/node@26/bin:$PATH` and the same TypeScript
path: exit0,10/10 passed. NaN class was checked, not architecture-specific payload.

`cargo check --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --all-targets --jobs 2`: exit0, finished dev profile.
This is scoped host compilation, not a workspace or wasm compilation claim.
All cargo operations were sequential; no shared SDK type/trait was changed.

After manifest edits, pinned Yarn writer at the implementation HEAD:
`NODE_PATH="$PWD/sdk-rs/target/grvt-extract-deps/node_modules"
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`:
exit0, `Updated 0 manifests; 0 added, 0 orphaned symbols.` Writer retains optional
Rust arrays; none were hand-spelled. At the restored merge SHA, the corresponding
`parity:extract:check` command: exit0, `Would update 0 manifests; 0 added, 0
orphaned symbols.` Both output texts were inspected separately from shim status.
No Rust inventory/provenance claim.

Whole venue src/tests diff from corrected base to restored merge exited0,
byte-unchanged. Cargo.lock and safe-cex-exchange changed through integration;
positive tests/check were rebuilt at the restored merge rather than claiming
old dependency results covered the new tree. The evidence-only successor does
not change these measured inputs.

Not run: workspace/full-crate/monorepo suites, host clippy, wasm check/clippy,
Rust inventory/provenance gates, CI reruns, live exchange/browser tests.

## Final merge preservation

Final integration parent: `c54b80192e833f40b00bdebe2de289336cfb6b86`.
Final merge SHA: `116a2b47706a445304e359627a17d571a0676c8f`.
Whole venue src/tests, crate/workspace/lock files, compiled local package inputs,
five project oracle source inputs, the own manifest, package.json and yarn.lock
diff against the restored gate SHA exited0, byte-unchanged. Thus scoped cargo
and Node24/26 exits at the restored gate SHA still cover these final inputs.

A conservative Cargo.lock dependency walk visited481 package nodes and eight
local packages, ending when its worklist emptied after1390 pops. No feature
pruning claim. Package structures matched between the two SHAs; all eight local
src/tests/Cargo.toml/build.rs plus lock/workspace/.cargo inputs diffed unchanged,
exit0. Locals: safe-cex-candle-cache, safe-cex-crypto, safe-cex-exchange,
safe-cex-num, safe-cex-store, safe-cex-transport, safe-cex-types and this venue.

Other venues' manifests changed. The pinned Yarn extractor check reran at the
final merge: exit0, `Would update 0 manifests; 0 added, 0 orphaned symbols.`
Output text was inspected. Own transition/census remains nine pending→ported
and `(145,0,16)`; whole Hyperliquid remains `(372,106,53)`.
The evidence-only successor changes this Markdown file alone and preserves all
measured inputs. The not-run list remains unchanged.
