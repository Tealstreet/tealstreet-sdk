# Retired duplicate public stream adapter

The three `stream_adapters::{BookCallback, CandleCallback, Unsubscribe}` aliases
had no production caller. A repository-wide tracked-source search for the module,
its five functions and `PublicStreams` found only its module declaration, its own
mock `Socket` test, historical evidence, manifest anchors, and string identifiers
in the class descriptors and native dispatcher. No native implementation of
`PublicStreams` existed. Registry construction, FFI/napi/wasm exposure and the
public Exchange trait all construct `binding::KrakenExchange`, not that trait.

Those class descriptor strings are dispatch identifiers: `ClassHost::member`
calls `binding/members.rs::dispatch`. Its book and Spot candle branches call
`delegate_public`, which constructs the actual socket and invokes its native
member. Its Futures socket candle branches implement the source no-ops directly.
The strings never resolve or call the removed Rust functions. They remain stable
so existing descriptors and their native class receiver observations stay intact.
The five TS manifest rows now explicitly map to this live dispatcher, with rename
provenance. The Spot constructor keeps every other split member mapping.

The canonical scoped baseline at `54abdd08c2` (the adapter blob is identical to
official master `2a028c645c`) (`accepted-set.mjs --package=safe-cex-venue-kraken
--measure --module=stream_adapters --jobs=1`) executed six owner probes: all three
aliases failed by name for missing accepted contracts; the duplicate `futures_listen_order_book`, `spot_listen_order_book` and local
`current` helper were also unclassified. Removing this dead module
removes those owners rather than granting fabricated non-decoder classifications.
The remaining live NumericChanges/Level/Levels aliases remain separate work.

Delete the mock-only test, its source-oracle/corpus and its five mutation controls.
Keep the unrelated orders control. Remove exactly the three dead adapter hook-edge
rows from override-call-sites.json. There is no package.json, workflow or wiring
script selector for the deleted Cargo target; repository-wide search checked this
before deletion. Historical GATE_EVIDENCE records remain historical receipts.

Live coverage stays in `tests/exchange_binding.rs`: constructed Futures/Spot
book frames and eleven Spot candle input cases compare native outputs against
the maintained TypeScript bodies using the actual HTTP/socket/Store pipeline.
`tests/support/accepted_stream_tail.rs` additionally reaches both candle no-ops on
the actual constructed Futures public socket, checks fresh/reusable disposers,
undefined results and no callback/HTTP/socket effect for eight input shapes.
Its source script executes those exact live TS arrows selected by compiler AST.
`stream-binding-controls.py` compiles independent book, Spot candle, cleanup and
handler faults in the real dispatcher; each must fail its named native test, then
pass after exact byte restoration. Compilation errors never count as RED.
