# Shared value graph

The Binance family uses `safe_cex_exchange::js_runtime::value::Js` directly.
`EventValue` is a public alias; `EventOps` supplies the existing decoded-data
operations on that graph. There is no venue object, array or categorical-value
storage. Categorical constructors create the source string primitive. Arrays
use the common optional slots, and source `forEach` skips holes while an array
iterator yields undefined for them.

Fresh decoded objects retain their captured `PrototypeRealm` through the common
object prototype. `Js::Builtin` retains the existing native intrinsic reference,
so late scalar-field mutations by core socket statistics remain visible without
copying or serializing that reference. Identity includes the realm. This is the
existing data-reference profile: native intrinsic execution, arbitrary object
fields on those intrinsic references, and complete intrinsic descriptors are
outside it. The bridge refuses unsupported writes. Ordinary runtime functions
and objects retain their existing implementation.

`EventOps` retains the venue's checked UTF-8 decoded-data operations and coercion
profile. The canonical graph stores UTF-16; this change does not implement the
separately planned cross-crate typed string model. DTO entry and exit use the
shared native typed bridge, preserving numeric bits before explicit JSON wire
projection. Existing wire and source-domain choices remain on their owning
manifest rows.

`tests/private_classes.rs::shared_runtime_graph_enters_bound_private_orders_without_projection`
feeds common `Js` values directly to all three bound private-order receivers. It
checks hole skipping, source shallow-spread versus retained-info identity, live
nested mutations, Infinity, NaN and string-parsed negative zero. Existing Node
oracles still execute the maintained TypeScript bodies and compare their output,
side effects, coercion/error order and alias behavior.

Core `tests/prototype_js_values.rs` compares six native intrinsic referents with
Node and separately checks captured-realm identity, live fields in both directions,
original-receiver prototype access, own shadowing, native f64 bits and native
error identity. The full Binance source oracle remains the acceptance gate:

```sh
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" yarn venue-oracle:binance
```

The Binance declared-DTO entrypoint selects
`to_typed_preserving_undefined`: own undefined fields remain present as native
unit values, and callable fields refuse conversion. This preserves the previous
DTO boundary (including synthetic position `info.symbolConfig`) using the same
shared native visitor. Ordinary shared `to_typed` keeps its JSON-shaped omission
policy. The pinned sixteen-method transport test independently checks this shape.

Validation of the migration checkpoint: the mandatory complete venue oracle
passes 300 tests across 73 targets; core runtime/typed/prototype/cancellation
checks pass 74 tests. Three compiling controls separately break retained
intrinsic identity, live native-field writes, and declared-DTO undefined presence.
Each fails its named assertion; after restoring the source, all eight prototype
and typed-value tests pass. Invention/status gates pass and extractor check
reports zero changes. This evidence validates the graph migration, not the
previously declared whole-venue divergences.

After merging current master, the integrated checks pass 79 core tests and all
64 tests in the six affected receiver/oracle targets. Strict host all-feature,
all-target Clippy and wasm library Clippy pass for core and Binance; formatting
passes. Registry and public facade compile with all features, covering downstream
consumers of the added common variant. Parity checks pass (7459 rows / 17118
public items), and extractor check reports zero updates.
