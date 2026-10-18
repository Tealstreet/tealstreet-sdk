# Native SDK dispatch bridge

The accepted-function record covers the real private `exchange_trait::call`
instantiated as `Vec<Candle>`. Six profiles construct the actual Futures, PM and
Spot SDK classes using `native_dependencies`, real shared clients, DefaultStore
and the production async reaction bridge. Only terminal HTTP responses are
scripted. The independent public SDK test invokes `Exchange::fetch_ohlcv`; its
maintained-source oracle constructs the full TypeScript classes and replaces
only the Axios terminal adapter.

Candle fields are inspected directly before JSON projection. The corpus includes
negative zero, Infinity, NaN and a missing volume field; binary64 bits and NaN
classification are preserved in the observation. The real lower-case wire
method, URL, query and telemetry count are captured independently on both sides.
Two additional native-only cases pass argument-conversion errors into the private
bridge and inspect the returned name, message, code (including negative zero),
context and absence of HTTP. These are Rust glue cases, not a separate TS API.

`scripts/sdk-call-controls.py` invokes the maintained compiler inventory and
native probe driver. Each temporary fault must compile and produce only named
function-output failures: losing typed result forwarding breaks six response
profiles; losing argument metadata breaks two error profiles. Each fault is
followed by byte-exact source restoration and a successful native measurement.
Old result files are removed before every run; compiler errors and classification
failures cannot count as behavioral RED. Optimized Python is rejected before
measurement or mutation.

The shared `exchange-native-runtime.rs` fixture contains the unchanged setup,
assembly, runtime driver and wire projection formerly inside
`exchange-native-observers.rs`. Private constructor/context observations remain
in their original fixture. This extraction allows public SDK integration tests
to reuse terminal I/O setup without widening production visibility.

This is a bounded generic instantiation and finite source corpus. It does not
claim every `DeserializeOwned` result, every class method, arbitrary native
object graphs or the complete feature matrix. The four class/State owner records
remain separate obligations. No production SDK body changes or exemptions are
introduced.
