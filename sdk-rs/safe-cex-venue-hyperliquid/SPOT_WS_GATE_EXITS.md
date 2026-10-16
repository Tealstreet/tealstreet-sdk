# Spot websocket dispatch and private socket gates

Fresh branch parent: `372972fbfaa126366d6309f4ba15fa84d07bc7a5`.
Implementation/control base: `16f8e48c787c6430448228b2d6347c2bb3f90ad5`.
Restored gate SHA: `dec8eee8407cb985bae9963cbb21d8c56737bbc7`, whose actual
integration merge parent is `51a76599506b631dbe122bcc00a259e4a8aeb259`.
No unrelated unreviewed feature branch was used as an ancestor.

Nine pending→ported, zero→divergent. Python3.14 tomllib independently compared
symbol statuses with the fresh parent across the nine assigned manifests:
`(127,0,34)` → `(136,0,25)` in ported/divergent/pending order. Including spot
constants/errors adds six ported rows: `(142,0,25)`. Whole Hyperliquid at the
restored gate SHA: `(353,106,72)`. The seven private pending rows are resolved;
public routing and mids account for the other two. Public full class/constructor
and listeners remain pending. Perp-core files/manifests were not edited.

## Implementation and finite witness

The private class allocates a native named derived constructor/prototype and
composes all eleven mapped lexical callback bodies in source initialization
order. Constructor forwards original parent/newTarget; native base construction
honors target prototype resolution and errors. Existing connect/open/subscribe/
ping/system/coin Rust leaves execute directly through these captured callbacks.
Class/prototype allocation, native function metadata, imported BaseWebSocket,
property/callee/coercion/JSON/iterator operations remain explicit host bindings.

Frame bodies retain `{data}` parameter binding before disposal, tracking outside
the catch, fresh channel/stats reads, active-data original references and open/
closed position updates. Fills preserve snapshot early return, live indexed
arrays, original market symbol handles, own-field order, parseFloat values and
immediate refresh. Funding intentionally does nothing; ledger refresh has zero
arguments. Candle callback failure aborts the batch; each book callback failure
is caught separately. Mids retains inherited enumeration, skips unknown data
reads, shares one buffer, and reads the current store/callee each iteration.

Oracle runs unchanged complete private/public source modules. Actual OrderSide,
WSS_URLS and subscription declarations plus unchanged jsonParse are loaded from
source: six hashed files. Independent floor73, stopped at index72
(`private/changing-channel`):4 class,4 constructor,20 private frames,5 balances,
7 fills,3 funding,3 ledger,21 public frames,6 mids. Nine Rust tests assert native
variants, explicit rejection state, UTF-16, ordered own fields, finite binary64
bits, NaN class, effects and original handles. Payload field assertions precede
serialization; no Rust result is encoded for comparison. No wire equality claim.

Witness includes a custom target/prototype throw, detached lexical callback,
complete connect/open chain, live appended fills/handlers, malformed/primitive
JSON, disposed data getter, a present poison object, actual throw-undefined,
changing channel getters, known/inherited/unknown/NaN mids and a replacement
store/callee mutating the shared buffer. Native fixture primitives are bounded,
not an independently proved JS engine. Arbitrary proxies/coercions/symbols,
nonenumerable shadowing, sparse/custom iterators, general custom target fallback,
full browser construction, public listener composition and live networking are
outside this finite proof. Mapping census and bounded proof are separate claims.

## Isolated mutation controls

At the implementation base, twelve transient controls were compiled once
(`cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test spot_ws_dispatch --no-run --jobs 2`, exit0).
Each ran alone through `sdk-rs/target/debug/deps/spot_ws_dispatch-d13016c937c29eb3
TEST --exact --nocapture`, with Node24.20.0 and
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Transient selector: `SAFE_CEX_SPOT_WS_MUTATION`. No selector/control remains in
production or fixture files. Every control exited101 at the named assertion:

| Control | Test | Named native assertion |
| --- | --- | --- |
| Prototype constructor points at prototype | private_class_composition | WS native class ordinary |
| Omit final constructor arrow | private_constructor | WS native constructor ordinary |
| Wrong private stats channel | private_frames | WS native private 0 |
| Drop original balance array | balance_forwarding | WS native balances balances |
| Use price as fill amount | fills_native | WS native fills buy-sell |
| Funding requests refresh | funding_noop | WS native fundings ordinary |
| Ledger passes true argument | ledger_refresh | WS native ledger ordinary |
| Book failure aborts remaining handlers | public_frames | WS native public l2Book-handlerThrow |
| MidPrice assigned zero | mids_native | WS native mids known |
| Reuse old ticker callee/store | mids_native | WS native mids mutate |
| Native allocator has wrong class name | private_class_composition | WS native class ordinary |
| Swallow outside-catch tracking throw | private_frames | WS native private track-undefined: explicit rejected discriminator |

No survivors. Restoring both instrumented files produced `git diff --exit-code`
exit0 against the implementation base. Whole venue src/tests also diffed
byte-unchanged between implementation base and restored merge SHA, exit0.
Both readonly sub-agent reviews found fixture gaps (ticker call label, class
metadata and present poison object); all were corrected and re-reviewed before
these controls. Reviewers ran no gates.

## Measured positive gates at the restored gate SHA

All cargo operations were sequential. Node24.20.0 scoped restored command:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test spot_ws_dispatch --jobs 2`: exit0,9/9 passed.
Node26.7.0: same restored test executable with
`PATH=/opt/homebrew/opt/node@26/bin:$PATH` and the same TypeScript path:
exit0,9/9 passed. Thus every primary/control assertion passes after restoration.
Finite bits were asserted; NaN was asserted as a native NaN class, not payload.

Required scoped compile command:
`cargo check --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --all-targets --jobs 2`: exit0, finished dev profile.
This is a scoped host compilation claim, not workspace or wasm compilation.

After manifest edits, the pinned Yarn writer ran at the implementation base:
`NODE_PATH="$PWD/sdk-rs/target/grvt-extract-deps/node_modules"
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`:
exit0, `Updated 0 manifests; 0 added, 0 orphaned symbols.` The writer preserves
optional Rust arrays; no hand-written arrays or inventory/provenance claim.
At the restored gate SHA, the corresponding `parity:extract:check` command:
exit0, `Would update 0 manifests; 0 added, 0 orphaned symbols.` Output text was
inspected separately from shim status.

Not run: workspace/full-crate/monorepo suites, host clippy, wasm check/clippy,
Rust inventory/provenance gates, CI reruns, live exchange/browser tests.

## Final integration merge and repeated gates

Final integration parent: `51e9d74873a1c863a42569e7dfeb95cbc2da40bd`.
Merge/gate SHA: `262cf08660a421484fd7057553c609933dda7c57`.
The other lane's ten-row position/order slice added
`src/core_position_order_state.rs` and `tests/core_position_order_state.rs`.
Only shared `src/lib.rs` conflicted; both module exports were retained.
Thus the whole venue src/tests changed, and earlier whole-crate compile results
were not reported as covering this merge.

At this SHA, the same required scoped all-targets cargo check reran: exit0,
finished dev profile. The same scoped Node24 cargo test reran: exit0,9/9 passed.
The freshly rebuilt executable reran under Node26: exit0,9/9 passed. The pinned
Yarn extractor check reran: exit0, `Would update 0 manifests; 0 added, 0 orphaned
symbols.` Its output text was inspected. All cargo operations were sequential.

Four own new production/test files, two reused native fixture files, six oracle
source inputs and two own manifests were independently diffed against the
earlier restored gate SHA: exit0, byte-unchanged. The whole-crate changes above
are explicitly excluded from this preservation claim and covered by repeated
scoped gates. Python tomllib census at this merge: own nine files `(136,0,25)`;
whole Hyperliquid `(363,106,62)`. Our transition remains nine pending→ported;
the other ten ported rows belong to the integrated perp lane.

The evidence-only successor changes this Markdown file alone. No measured
production, test, dependency or manifest input changes in that successor.
The not-run list remains unchanged.
