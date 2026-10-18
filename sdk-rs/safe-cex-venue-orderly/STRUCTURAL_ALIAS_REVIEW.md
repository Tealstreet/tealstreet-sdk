# Orderly structural alias review

All three assigned aliases have live users in configured websocket/account bodies.
They remain generic public aliases, with one fixed production-host profile each.
No identity fallback or arbitrary host/type instantiation is enabled.

| Alias | Actual reviewed native profile |
| --- | --- |
| `ws_controls::Value<Host>` | `Rc<Js>` retained runtime value. |
| `ws_controls::Entries<Host>` | Ordered `Vec<(Vec<u16>, Rc<Js>)>` own-entry snapshot. |
| `ws_controls::Result<Host, Value<Host>>` | `Result<Rc<Js>, Thrown>` success or original failure. |

The canonical reviewed-generic policy pins exact generics, raw/expanded compiler
ASTs, source identity, native expected types and fixed production instantiations.
Actual trait probes classify these native profiles as non-decoders; those receipts
do not execute callbacks or classify other H/T values. No other package debt is
waived. The existing Host and constructor are visible only within the crate;
live `assemble` uses that same re-export, with no wrapper or alternate host.

`native_exchange::structural_alias_tests` constructs all six native account classes.
The actual Host observes original child referents, negative-zero/NaN/infinity bits,
own undefined and lone UTF16 units. Own-entry snapshots retain source key order,
complete cardinality and live child identity independently of later key insertion.
Actual Host.call observes the real receiver/arguments and original failure object.

The same witness connects each real public socket and delivers a terminal frame
through its bound source handler. Two callbacks receive the same parsed object;
mutation by the first is visible to the second. A callback rejection through the
actual class method preserves original error identity, UTF16 message and own
undefined code. Runtime and terminal HTTP/socket objects are primitive host ports;
constructors, classes, source bodies and Host operations are production code.

`scripts/structural-alias-controls.py` independently corrupts native value bits,
entry cardinality and callback error forwarding in the real Host. Every fault
must compile, fail the named class witness, restore exact source and pass again.
Compiler failures and empty test selections are not valid controls.

Run serially from `sdk-rs`:

```sh
node safe-cex-venue-orderly/scripts/structural-alias-contracts.mjs
cargo test --locked --jobs 1 -p safe-cex-venue-orderly --lib structural_alias_tests
python3 safe-cex-venue-orderly/scripts/structural-alias-controls.py
```

These profiles do not cover other aliases, function owners, arbitrary hosts,
feature configurations or Wasm execution. Existing source/corpus assertions remain.
