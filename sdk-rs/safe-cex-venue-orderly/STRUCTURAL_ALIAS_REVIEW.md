# Orderly structural alias review

The three ws_controls aliases have live users in configured websocket/account bodies.
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

Four additional callable aliases are retained by the configured native classes:
`classes::{BoundMember,BodyFuture,NativeMember}<Host>` and
`native_exchange::OrderlyHttpFactory`. Their exact production Host profiles and
compiler declarations are registered without permitting arbitrary instantiations.
These are native invocation, future and transport capabilities, not decoders.

`native_class_callable_aliases_preserve_values_futures_and_errors` constructs all
six account classes and invokes the production Host arrow/member implementations.
It checks synchronous and asynchronous value/error identity, negative zero, lone
UTF16, own undefined, the real REST body/client and both HTTP factory option sets.
The terminal transport and runtime are the only scripted primitives.
`scripts/class-alias-controls.py` independently corrupts argument forwarding,
future results, member arguments and factory options, requiring compiled failures
and exact restored passes.

The reviewed nested `copy` and `present` bodies are byte-identical before and
after 9225c237ef. Its two surrounding deletion calls propagate their existing
Results. The canonical scan also refuses the local query glob import; its four
used names are now explicit module imports. Both helpers and query behavior are
unchanged, and the enclosing-file fingerprints cover this reviewed import edit.

Run the added capabilities serially from `sdk-rs`:

```sh
cargo +1.99.0 test --locked --jobs 1 -p safe-cex-venue-orderly --lib native_class_callable_aliases_preserve_values_futures_and_errors
python3 safe-cex-venue-orderly/scripts/class-alias-controls.py
RUSTUP_TOOLCHAIN=1.99.0 CARGO_BUILD_JOBS=1 node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-orderly --jobs=1 --measure --owner=classes::BoundMember --owner=classes::BodyFuture --owner=classes::NativeMember --owner=native_exchange::OrderlyHttpFactory
```
