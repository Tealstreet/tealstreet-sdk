The iterator cutover added four compiler owners and three entrypoints to the
existing `Js` owner. These contracts are reviewed independently of venue binding
coverage. They preserve the existing Exchange contract shard and change no
production runtime behavior.

| Owner | Reviewed role and native evidence | Compiled fault |
| --- | --- | --- |
| `builtins::cached_iterator_member` | Non-decoder: upgrades or records an already native callable under an internal static key. Actual Map/Set lookup retains live method identity and invokes that member. | Return a fresh replacement instead of the live cached method. |
| `collections::iterator_object` | Non-decoder: constructs an iterator object carrying a native continuation, with no serialized input. Observe its brand, iterable-self identity, own-property surface and actual step. | Drop the retained continuation slot. |
| `collections::iterator_next_method` | Non-decoder: constructs a native dispatch function for an internal brand. Execute it against another valid receiver and reject ordinary/forged receivers. | Invert brand admission. |
| `collections::Collection<T>` | Observed `Deref` API: borrow the actual dense ordered native values. The selected `Collection<Js>` fixture also observes signed-zero bits and reference identity directly. | Discard the pushed native value. |
| `value::Js` | Preserve all existing entrypoints; observe computed symbol access separately from string keys, actual array/string iterator values and exhaustion, and Map/Set iterable consumption. | Independently break symbol lookup, GetIterator and iterable-value forwarding. |

`tests/iterator_contract_oracle.cjs` authors the engine operations and expected
outputs independently of Rust. It executes native JS Map/Set methods and
computed property/iterator operations. The dense storage expectation is the
ordered value list underlying the JS collection, rather than a claim that Rust's
`Deref` is a JS decoder. The three callable factories emit explicit
classification-only receipts; their behavioral witnesses are supplementary
native tests, not fabricated decoder receipts.

Run fresh selected discovery/probes first, then the five direct native witnesses
and the sequential controls:

```sh
node sdk-rs/safe-cex-exchange/scripts/iterator-accepted-probes.mjs
cargo test --manifest-path sdk-rs/Cargo.toml --locked --jobs 1 \
  -p safe-cex-exchange --lib accepted_iterator_ -- --test-threads=1
python3 sdk-rs/safe-cex-exchange/scripts/iterator-accepted-controls.py
```

The controls capture that compiler document before changing one body line at a
time; these finite faults preserve declaration spans and APIs. They use bounded
Cargo subprocesses, private temporary logs and exact source restoration in
`finally`. Each fault must compile, fail its named native assertion and pass
after restoration. Collection/Js faults must also fail their actual production
contract output observation. All five production receipts run again after exact
source restoration. Compiler errors are not evidence.

The selected accepted-contract regression runner uses fresh production rustdoc,
the maintained compiler inventory, `loadContracts()` and the ordinary native
probe harness:

```sh
node sdk-rs/safe-cex-exchange/scripts/iterator-accepted-probes.mjs
```

It covers these five native/default owners only. Two owners have executed output
observations; three have reviewed non-decoder receipts. This selected scope is
separate from the ordinary package/transitive accepted gate and does not claim
feature, wasm, local-owner or full accepted-domain coverage. The final executed
results and any unrelated gate failures are recorded in the PR.
