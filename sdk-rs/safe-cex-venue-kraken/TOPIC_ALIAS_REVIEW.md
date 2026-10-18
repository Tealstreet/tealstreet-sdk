# Public subscription Topic

`public_subscriptions::Topic` is the concrete `Vec<u16>` used by the constructed
socket's public subscription host. It inherits Vec's Serde decoder: arrays of
valid u16 code units are accepted, including unpaired surrogates. JSON strings,
non-arrays and invalid elements are refused. This is a code-unit collection
contract, not a string decoder or a non-decoder capability. The native observation
inspects the decoded elements before any serialization.

`scripts/topic-alias-contracts.mjs` obtains fresh compiler artifacts, alias source
claims, verified macro origins and the actual expanded target through the shared
maintained context helper. Its `--expect-unclassified` mode records the original
missing-contract failure without inventing a classification. The regular mode
runs native Serde and alternate-conversion detection for this one alias.
It also authenticates four separate outgoing helper APIs: topic cell creation,
interval preparation, Spot envelope assembly and live listener frame preparation.
These have specific encoder-role classifications, not decoder execution receipts.
`--expect-helper-unclassified` records their four real missing-contract failures;
the constructed physical-wire tests separately prove their actual execution.

The production `topic` helper previously used `String::from_utf16_lossy`. The
compiled physical Futures wire test failed on `[0xd800]`: Rust sent U+FFFD while
the live TypeScript sent the escaped original code unit. Public subscription and
listener frames now go directly through canonical `RetainedJsonInput` and its
shared JSON writer. Both paths preserve field order, numeric/omitted interval
behavior and the original symbol units. Private authentication frames are unchanged.

`tests/support/accepted_topic_alias.rs` uses an actual constructed Kraken Scope,
socket host, assembled listener/onOpen members and physical WebSocket recorder.
Separate Futures and Spot tests compare six independent symbols (ordinary,
high/low lone surrogates, separated surrogates, a valid pair and JSON control
characters) against unchanged live TypeScript onOpen arrows selected by AST.
All four prepared subscription shapes, live book registration, reconnection
resubscription and Spot unsubscription reach the real wire. No subscription or
encoding algorithm is replaced by a test mock. The terminal host controls only
HTTP, socket transport and clock boundaries.

`scripts/topic-alias-controls.py` reintroduces only the lossy topic conversion,
requires both named compiled physical-wire assertions to fail, restores exact
production bytes and requires all three native tests to pass. Run it serially
from `sdk-rs` with `CARGO_INCREMENTAL=0`, followed by the native alias runner,
accepted native fixture suite, exchange binding target, full inventory/index,
all-features cancellation/override gates, fmt and strict all-target Clippy.
The six other aliases and their consumers remain independently unclassified.
