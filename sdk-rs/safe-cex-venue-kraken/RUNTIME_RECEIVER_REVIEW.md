# Trading iterable, request and tick receivers

The live `KrakenSpotExchange.placeOrder` spreads `txids` and the attached-order
IDs. The concrete `TradingHost::iterable` formerly admitted only Array/String,
rejecting a native Set before iteration. The constructed-Scope regression failed
on that original body with `txids is not iterable`.

The direct cutover resolves the well-known iterator method once, preserves the
caller's missing-method diagnostic, and passes the acquired iterator into the
existing shared `IteratorRecord::from_iterator`. Its protocol body is unchanged;
only its visibility is widened. It caches `next`, steps original native values,
and propagates original getter/protocol exceptions. There is no secondary
iterator implementation or JSON projection. The shared accepted owner now has
separate executed `get` and `from_iterator` entrypoint corpora.

`tests/support/accepted_runtime_receivers.rs` reaches the actual private Scope of
a constructed exchange, using only terminal HTTP/runtime dependencies:

- `iterable_uses_native_collection_protocol_and_retains_cells` observes Set/Map
  order and native references, infinity, negative-zero bits, paired/unpaired
  UTF16 strings, the iterator-self protocol, cached-next getter count, original
  next receiver, exhaustion and the original thrown Error. It also retains the
  two source-expression refusal messages.
- `send_uses_actual_clients_verbs_wire_and_native_response` executes all four
  limited/unlimited × POST/PUT combinations through assembled signing and HTTP.
  Real request interceptors attach distinct client headers; the terminal witness
  checks that header, verb, exactly one request and final encoded form fields.
  Returned native identity, lone UTF16 and negative zero are inspected directly.
- `emit_tick_error_uses_message_or_fallback_through_real_base` observes nonempty
  message and empty-message fallback through the real Base console and error
  listener, including argument count and formatting. No emitter is replaced.

`runtime-receiver-source.cjs` AST-selects the actual `txids` spread inside live
Spot `placeOrder` and executes ten protocol cases. It also executes the live
Futures `tick` arrow for its message/fallback branches. Source SHA receipts are
printed by the script rather than copied into a second implementation.

`runtime-receiver-controls.py` compiles three individual production corruptions:
restore the Array/String restriction, select the wrong real client, and corrupt
the fallback. Each must produce its named native runtime failure; each exact
source restoration must pass. No panic-only or compilation-error control counts.

The three reviewed concrete receiver rows are delegated protocol/request/error
adapters with distinct evidence. Full canonical Kraken discovery reduces fourteen
refusals to exactly eleven: ApiHost retry/join three and the separately assigned
eight SpotCancel/Account/Class methods. This is not a whole-venue accepted-set
pass; aliases remain undiscovered until those receiver guards clear. Shared Store
array identity and feature/wasm admission are outside this proof.

Measured evidence: constructed native tests 3/3; compiled semantic controls 3/3
named failures with exact restored passes; stock accepted inventory 14→11
receiver refusals. The canonical shared IteratorRecord native measurement records
one executed production owner, zero failed owners, with both exact public
entrypoints separately observed. Its full compiler dependency inventory is kept.
The live source oracle reports ten spread cases and two actual tick executions;
accepted-contract schema controls pass 15/15 and the extractor reports zero drift.
