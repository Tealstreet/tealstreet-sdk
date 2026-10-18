# Backpack remaining methods and owned adapters

Requested parent: `acf29148c3f26a07b72f93d1d964b49e81016ec6`.
Branch: `feat/rust-backpack-adapter`.
Code/gate SHA: `9a2dc4e81dfe252691ea712044fa2e3ac186fb58`.
There are no earlier unmerged feature commits between that requested integration
parent and this slice. Initial tracked tree was clean; the three untracked aimux
briefs were context files. No inherited mutation was present.

Python 3.11 tomllib reads each `[[symbol]]` independently at the parent and code
commit: 147 Backpack rows; parent 82 ported / 56 divergent / 9 pending; code 87
ported / 60 divergent / 0 pending. **9 rows moved: 5 pending to ported and 4 to
divergent.** This is a manifest census, not exhaustive behavioral proof.

The five ported bodies are SHA-512, single placement, order replacement, candle
subscription and depth subscription. The API factory and the three real owned
classes have per-row CHOSEN/MIXED notes. These classes instantiate concrete
state and bind the existing source bodies, transport/interceptor pipeline,
public/private protocols, reconciliation and shared SDK dispatch. They are not
namespace aliases or method-name registries.

## Measured gates at the code SHA

| Command | Measured exit and output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2` | 0; `Finished dev profile`; scoped crate and targets only. |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test adapter_methods --test adapter_classes --test private_auth --test private_events --test runtime_methods --jobs 2` | 0; respectively 6, 5, 10, 5 and 4 passed, 30 total; zero failed. |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 and output `Would update 0 manifests; 0 added, 0 orphaned symbols`, 7341 symbols across 512 sources. The output text was read. |
| `git diff --exit-code HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests` before the controls | 0; gate inputs matched the code SHA. |

The writer `parity:extract` ran after the nine manifest edits and before the code
commit: exit 0, `Updated 0 manifests; 0 added, 0 orphaned symbols`. It is a writer
observation on the precommit tree, not a newly measured commit-bound gate. No
`rust_files` arrays were hand-edited.

Raw command logs and mutation runners are temporary `/tmp` files. This manually
authored tracked sheet records the measured commands and limits.

## Witness scope

`adapter_oracle.cjs` executes the actual TypeScript exchange, API and public
socket bodies after transpilation, supplying imported host/runtime operations.
The signing source installs its actual hash callback on the noble-shaped hook;
Node crypto provides the imported SHA-512 primitive for that witness. It does
not prove the full noble/Ed25519 signing implementation.

The Rust oracle consumer independently pins **26 ordered rows**: placement
iterations 0–7; replacement 0–3; hash 0–3; API 0–3; candle and depth 0–2. It pins
family/index content, independent native request/result/callback cardinalities,
hash inputs and byte length, candle callback counts and depth callback counts.
These enumerations stopped at those iterations and are finite floors.

Native assertions inspect UTF-16 units, enum values, f64 bits, request fields,
book levels/totals/sequence, result values and Rc identities directly. They do
not compare decode/re-encode roundtrips. Source wire output is only the external
expectation for those decoded native assertions.

The five class tests independently exercise the same-base API emitter, concrete
socket slots, preflight before transport, ticker binding and leading/trailing
cancellation, detached listener ownership, private order dispatch/reconciliation,
non-JSON HTTP success handling and the eleventh request waiting after ten calls.
They are native integration witnesses, not an enumeration of every inherited
method. RPS is a deterministic eleven-request scheduler probe, not a load test.

## Deliberate boundaries and review

Request configuration uses prepared JsJsonValue/UTF-8 values and preserves shallow
header identity. Successful response data stays canonical retained Input through
the shared Axios transform, interceptor and native callback; model info retains
unread UTF-16 keys/values, nonfinite numbers and own undefined. These snapshots
exclude executable Axios properties, symbols, sparse holes, getters and source aliases. Placement/replacement body witnesses use
prepared Options and compatible existing-record reads, not arbitrary JS getters.

Concrete classes use static Rust dispatch and host-provided HTTP/socket/crypto
operations, weak socket parents and stored listener frames, and cancellable
owned work. Native transport/rate/retry scheduling uses the shared SDK contracts.
Disposal follows the source overrides: the exchange sets its shared flag before
public then private disposal; each socket clears only its declared source state
and closes its current transport. Store subscriptions/listeners, handlers/topics,
ticker throttles and position/balance work omitted from source cleanup remain
intact. These adapters do not invoke superclass disposal. Three bound native
regressions fail before this repair and pass against the actual source arrows in
`tests/disposal_binding.rs` / `tests/disposal-source.cjs`. Existing whole-class
representation and ownership choices remain separately declared.

Read-only sub-agent review found and prompted fixes for constructor/API identity,
public-only/signers, caller-ID shape/split validation, malformed-frame parsing,
text HTTP responses, stale throttle trailing work, private open sequencing,
negative-zero PnL accumulation, callback borrows and listener ownership/error
reporting. The final review reported no further material defect within the
named SDK/runtime boundaries. The reviewer ran no Cargo or mutation gate.

Development observations were not green gates: early scoped checks exited 101
for binding/signature/lifetime errors, early new-test compilation exited 101 for
fixture signature/import issues, and a class dispatcher test exited 101 because
its draft input omitted the required `stream` envelope. All were corrected before
the commit-bound gates above. The first placement oracle draft used incorrect
capitalized enum literals and was corrected before being relied upon; the final
corpus exercises actual TP/SL and trigger branches.

Not run: workspace compilation, any full crate/workspace/monorepo suite, other
Backpack test targets, host clippy, wasm clippy/build/tests, live network/credential
or venue trading tests, CI reruns or force-triggered CI. None is claimed.

## Discriminating controls at the code SHA

Controls stopped at **iteration 10**. Nine production mutations were run in two
disjoint groups to avoid repeated crate builds: one six-method group and one
three-class group. Every listed test reached its own named native assertion;
these were runtime panics, not compiler failures. Grouped observation is not a
claim that nine separate Cargo commands were run.

For the method group, the scoped `cargo test ... --test adapter_methods --jobs 2`
exited **101**, six tests failed at the six listed assertions, and after restoring
all bytes the identical command exited **0**, six passed. The class group used
`--test adapter_classes --jobs 2`: **101**, three named failures and two passes;
restored **0**, five passes. Both commands used the TypeScript path shown above.

| Iteration | Deliberate mutation / call site | Named assertion that panicked |
| --- | --- | --- |
| 0 | Placement API side Bid/Ask reversed in `order_actions::place_order` | `PLACE native request text` |
| 1 | Replacement amount increased by one in `order_actions::update_order` | `REPLACE native amount` |
| 2 | Raw SHA-512 output bytes reversed | `HASH native output bytes` |
| 3 | API fallback timeout 30000 changed to 30001 | `API native timeout` |
| 4 | Candle high/low swapped symmetrically in native callback construction | `CANDLE native decoded field` |
| 5 | Snapshot bids/asks swapped symmetrically in native book population | `DEPTH native price` |
| 6 | Owned exchange leverage default 1 changed to 2 | `CLASS native exchange leverage default` |
| 7 | Owned ticker handler incorrectly bound to depth updater | `PUBLIC CLASS native ticker binding` |
| 8 | Native private order amount replaced with remaining | `PRIVATE CLASS native order amount` |
| 9 | Placement supplement-case enumeration truncated from eight rows to seven | `ADAPTER independent corpus SIZE` |
| 10 | Homogeneous 26-row corpus made from row zero, preserving each expected family/index label | `PLACE corpus input type` |

Iterations 9 and 10 ran the already-restored native test binary directly:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript sdk-rs/target/debug/deps/adapter_methods-9d940cc3a9284f47 placement_native_source_requests --exact`.
Each individually exited **101** at its named assertion; byte-identical restoration
followed by the same command exited **0**. These are test-binary exits, not Cargo
exits. The homogeneous control retained the independent size and label pins and
was still rejected on independent input content.

Every mutation was restored byte-identically. Measured SHA-256 values:

| Restored file relative to the Backpack crate | SHA-256 |
| --- | --- |
| `src/order_actions.rs` | `8b8e0ae504633a588e6379e70c7387b03eb5c6c9277cfb4fa426ee151d702dd4` |
| `src/signing.rs` | `83e638a10466746f95248c228a54304c08cb90e422f57b9cc46647bcd09afeeb` |
| `src/api_factory.rs` | `946d24a46f2ed5ca637cf75bac5db7b504e4668da3c2013ab1ddae47d5f41ac2` |
| `src/public_listeners.rs` | `ca594a653de49f83a83c36f3b645744ab9fc600c57d601ea4892970e774b1b14` |
| `src/adapter.rs` | `f84b8f4be446b6dd197e6f0864cdb0345c1ba26124ad785071aca5a510fd1697` |
| `src/public_adapter.rs` | `206a2f04cba984ae4f5b54c4c531cc70c788db1e541c38b509f534f76674e62e` |
| `src/model_bridge.rs` | `be02ef04f1d770de265f84d0a46d0e6ee531d56898bae0042b8551b8bc9067e9` |
| `tests/adapter_oracle.cjs` | `13c5cc2d8ccb898e8654143f9d9daeb75549cd1e9cb118c66364fdf49d50f9c1` |

After all controls, `git diff --exit-code 9a2dc4e81dfe252691ea712044fa2e3ac186fb58 -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`
exited **0**. No survivor was observed among these eleven controls. No claim is
made about unenumerated mutations.

## Integration merge preservation

Fetched integration head: `f6c865ddd8331c32516f063d90d045761bd2c73b`.
`git merge --no-edit --no-stat FETCH_HEAD` exited **0**, producing merge SHA
`bab10998d28c0a13c5603b26fd3410e72c927ccf`. No conflicts occurred. This is a
merge down into the feature branch, not a merge into integration.

At that merge SHA, the scoped diff against code/gate SHA
`9a2dc4e81dfe252691ea712044fa2e3ac186fb58` exited **0** for the venue source,
tests, Cargo manifest and parity manifests; workspace Cargo.toml; actual-source
Backpack directory, types.ts and LogSeverity.ts; the imported TradeTunnel frames;
and all seven reachable internal dependency package directories. The eighth
internal package is Backpack itself. No previously measured exit is relabeled as
a new merge-SHA Cargo run.

The Cargo.lock closure was resolved from the Backpack package at both commits,
through the complete declared dependency closure reached in that traversal:
**481 package records**, equal as full parsed records. There were **8 internal
packages**: Backpack, candle-cache, crypto, exchange, num, store, transport and
types. Entire Cargo.lock bytes differ because integration changed unrelated venue
dependency entries; the reachable records and tested Rust inputs were unchanged.
This claim covers that declared lock graph and the listed tracked inputs, not
untracked compiler/toolchain/cache contents.

At the merge SHA, `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`
exited **0** with output `Would update 0 manifests; 0 added, 0 orphaned symbols`.
A fresh Python 3.11 tomllib census still measured **147 rows: 87 ported / 60
divergent / 0 pending**. The source/test/control gates remain observations at
the code SHA with the above preservation proof. Cargo and the controls were not
rerun at the merge SHA because their tracked inputs and reachable lock graph were
unchanged. The following delivery commit changes only this evidence sheet.

Disposal repair gates: all three bound regression tests compiled and failed
against the old adapters at source-absent superclass/task cleanup, then passed
against the actual TypeScript disposal arrows after repair. Native and managed
relay close-state tests pass; the complete core library (139 tests) and all seven
cancellation-inventory checks pass after reviewing four unchanged guard dependency
pins. Host all-feature/all-target and wasm library strict Clippy, formatting,
parity and extractor zero drift pass. The complete Backpack suite including
ignored source oracles passes 110 tests across 27 targets.
