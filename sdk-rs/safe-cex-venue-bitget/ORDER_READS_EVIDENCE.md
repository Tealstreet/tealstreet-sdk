# Bitget order reads and REST recovery

Fresh branch `feat/rust-bitget-order-reads`; actual direct integration parent
`59fab229a563a163c7ffe831c844da1c1149b5a2`. No separate unreviewed lane branch
was used as an ancestor. Code/gate/review SHA:
`ffa9f9e10cce47a46be84a0010805bb688f66456`.

## Census and body mapping

Python3.11 **tomllib** compared `(manifest, kind, ts)` identities against that
parent: **8 pending -> ported, 0 -> divergent**. Eleven manifests, 227 identical
identities; **79 ported / 80 divergent / 68 pending** before, **87 / 80 / 60**
after. Enumeration stopped at sorted manifest eleven,
`bitgetusdc.exchange.ts.toml`. Census is separate from behavioral proof.

Eight actual targets in `src/order_reads.rs`:

| TypeScript field | Rust body |
| --- | --- |
| BitgetExchange.fetchOrders | fetch_orders |
| BitgetExchange.fetchNormalOrders | fetch_normal_orders |
| BitgetExchange#fetchUtaOrders | fetch_uta_orders |
| BitgetExchange#fetchUtaNormalOrders | fetch_uta_normal_orders |
| BitgetExchange#fetchUtaStrategyOrders | fetch_uta_strategy_orders |
| BitgetExchange.fetchAlgoOrders | fetch_algo_orders |
| BitgetExchange.fetchTrailingStopOrders | fetch_trailing_stop_orders |
| BitgetExchange#reconcilePrivateStateFromRest | reconcile_private_state_from_rest |

Bodies preserve ensure/routing; sequential Classic reads versus captured supplied
Promise.all; request construction order, category/product/plan/limit/cursor
parameters; 20-request caps and empty/missing/repeated cursor termination;
repeated guarded data/list/endId getters; UTA API errors with accumulated mapped
results; and direct-array/nested-list strategy shapes. Trailing direct-array
responses return only that current page even after prior accumulation, retaining
the source behavior. Recovery retains lazy guards, the running setter outside
try, captured allSettled, fast balance options, fulfilled-only updates, copied
loaded flags, and finally clear/schedule with cleanup error precedence.

Opaque native values preserve references, UTF-16 and number representation.
The 33 exemptions are actual trait, associated and operation-binding helpers.
Host-supplied JS array/method/iterator/property/object/prototype operations,
HTTP and separately mapped receiver callees are explicit contracts. The host
owns started promises and binds the original Promise intrinsic; the body does
not implement a Rust polling race or claim the concrete binding exists.
Concrete Exchange/HTTP assembly remains pending. Settled fixtures do not prove
temporal rejection order, sibling lifetime, cancellation or arbitrary overridden
native operations. These supplied obligations are not silently claimed as tests.

## Native assertions and actual-source floors

Four tests in `tests/order_reads.rs` assert native variants, independent counts,
IDs, numeric bits, UTF-16, request parameters, callback indices, state patches,
loaded flags, effect order and original error Rc identity before source output
comparisons. Direct native assertions check mapped info and recovery
orders/positions/balance aliases and that loaded is copied without changing the
original. No re-encoding is used as a native assertion.

`order-reads-capture.mjs` extracts the eight real field initializers, original
success/error helpers and actual endpoint table. Receiver callees, HTTP, mapper,
store and emitter are fixture collaborators; supplied Promise wrappers use
actual JS Promise.all/allSettled. Source array.flatMap is instrumented while
calling the original intrinsic. Its fixtures do not turn deferred host binding
obligations into concrete runtime proofs.

The **91** source cases are bounded floors:

| Group | Independent floor | Stopped at |
| --- | --- | --- |
| Eight bodies x ten scenarios | 80 | index 79, reconcile/stop |
| Repeated/throwing pagination getters | 4 | index 3, strategy second data read throws |
| Sparse strategy input | 1 | index 0, one hole plus row at callback index 1 |
| Recovery entry/post guards and cleanup overrides | 6 | index 5, clear setter throws and skips scheduling |

The cap witness supplies 21 pages and expects exactly 20 requests, ending at
request index 19; the final supplied page remains unread. Getter witnesses
change second entrustedList/endId/data reads; a throwing strategy read verifies
original error identity before mapping. Recovery controls include setter outside
try, post-await healthy state, schedule overriding an original error, and failed
clear leaving the running flag true. These are not all-operation failure or
all-shape enumerations.

## Positive gates at ffa9f9e10cce47a46be84a0010805bb688f66456

Cargo PATH includes `/opt/homebrew/opt/rustup/bin`, `--jobs 2`, one Cargo process
at a time. Tests set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Measured exit / output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | 0, Finished dev |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2 -- --include-ignored` | 0, crate unit/integration/doc tests passed, including four new tests |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` | 0, 7341 symbols; zero updates/additions/orphans, output inspected |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0, 7341 symbols; would update zero manifests, zero added/orphaned, output inspected |
| Rustfmt check of both new Rust files; `git diff --check` | 0 |
| Python3.11 tomllib identity/status census | 0, exactly eight pending -> ported |

An initial writer also ran after manifest editing before the code commit; it is
not claimed as execution at this SHA. Precommit test development first failed
compilation due to unused copied helpers (101), then actual-source comparisons
caught xhr/category evaluation order in the strategy body. Both were corrected
before the recorded SHA and all four tests passed. Neither development failure
is a discriminating control gate.

## Sub-agent review citation

`/root/account_read_review` reviewed exact committed blobs at
**ffa9f9e10cce47a46be84a0010805bb688f66456**, verified immediate parent
**59fab229a563a163c7ffe831c844da1c1149b5a2**, and found no blockers within the
declared supplied-operation body scope. It checked all eight source
initializers, read multiplicity/order, pagination, trailing-array behavior and
cleanup exception precedence; inspected the 91-case floor; independently parsed
TOML and asserted eight transitions plus 33 distinct exemptions (exit 0).
Its committed-source-only six recovery cases asserted results, operations and
final flag (exit 0); source loader root was adapted for node -e and TS dependencies
verified byte-identical. It made no edits and did not run Cargo/native Rust,
extractor, compiling controls, clippy/wasm, CI or workspace checks. Review covers
this body commit, not all intervening integration commits.

## Claims not run or not established

No workspace compilation, repo-wide builds/suites, host clippy, wasm clippy,
CI rerun, live credentials/network reads, concrete Exchange/HTTP binding or
end-to-end venue execution. No exhaustive proxy/prototype/iterator/custom-method
semantics or temporal Promise scheduling proof. Separately pending concrete
assembly and callee rows remain pending; the eight statuses cover these actual
bodies over their declared supplied native-operation contracts.

## Compiling controls at the same code SHA

Each edited only the production body and ran `cargo test --manifest-path
sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test order_reads --jobs 2
native_order_reads_then_source -- --exact`. All six compiled (Finished test),
failed at a named native assertion with **exit 101**, and restored the original
bytes. No measured survivor. The restored full four-test file then exited **0**
with the same oracle environment.

| Family / deliberate mutation | Named native failure, exit 101 |
| --- | --- |
| Classic pagination: cap 20 -> 19 | `READ native independent mapped count normal/cap` |
| UTA pagination: limit 100 -> 200 | `READ native UTF16 request params uta-normal/normal/0.limit` |
| Aggregation: algo before normal | `READ native UTF16 returned ID orders/normal` |
| Strategy: type tpsl -> wrong | `READ native UTF16 request params strategy/normal/0.type` |
| Trailing: accumulate instead of returning only direct-array page | `READ native independent mapped count trailing/mixed-array` |
| Recovery: loaded.balance false instead of true | `READ native scalar recovery patch.loaded.balance: Bool(false) != Bool(true)` |

Restored-file gate: `cargo test --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-bitget --test order_reads --jobs 2`, **exit 0**, four passed.
These controls establish discrimination for their named changes, not exhaustive
mutation coverage. The aggregation control exercises Classic ordering; temporal
Promise aggregation remains a supplied obligation.

## Integration merge and preservation

Merged integration **5911167261853a8cbcc99d79cf45b785bb016648** down in
**71832806daa02a695e4f0968f8360439d0ab2e05**. The only conflict was
`sdk-rs/parity/exempt.toml`: retained the exact integration entries and appended
all 33 own helper records. Python3.11 tomllib asserted full preservation and
unique rs identities, **exit 0**. No CLAUDE.md conflict or local edit.

`git diff --exit-code ffa9f9e10cce47a46be84a0010805bb688f66456 HEAD --
sdk-rs/safe-cex-venue-bitget/src sdk-rs/safe-cex-venue-bitget/tests
sdk-rs/safe-cex-venue-bitget/scripts` measured **0** at this merge. The same byte
check over the recursively discovered local Cargo dependency directories
(types/num/exchange/store/transport/candle-cache/crypto), workspace and crate
Cargo.toml, .cargo, and packages/safe-cex/src measured **0**. Cargo.lock changed
only the safe-cex-venue-okx package dependency record, independently parsed with
tomllib; no Bitget dependency record changed. Cargo and mutations were **not
rerun after this merge**; their measured exits remain attached to ffa9f9e10c.

Writer and extractor check both ran at **71832806daa02a695e4f0968f8360439d0ab2e05**,
**exit 0**, 7341 symbols and zero updates/additions/orphans; output inspected.
Merged tomllib census against 5911167261 measured **exit 0**, the same eight
pending -> ported transitions, 227 identities over eleven manifests, and
79/80/68 -> 87/80/60. A subsequent fetch before the documentation commit still
reported integration 5911167261. Evidence-only follow-up commits do not change
these gate inputs; no gate execution is misattributed to their SHA.
