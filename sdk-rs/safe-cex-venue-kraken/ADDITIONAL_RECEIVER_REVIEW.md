# Kraken cancellation, account and class receivers

The concrete Spot cancellation host converted a caught `Thrown` into
`ExchangeError`, then selected its UTF-8 `message` and built another typed error.
The live `KrakenSpotExchange.cancelOrders` catch instead selects `err?.message ||
fallback`, forwards that original value to `emitAndLogError`, then constructs a
fresh Error. An order-ID getter throwing D800/0058/DC00 reproduced a native
FFFD/0058/FFFD rejection on master; the unchanged TypeScript arrow preserves all
three units.

Spot cancellation now carries `Thrown` and `Js` through every host operation and
await. Only the fresh Error's message uses native ToString, after reporting, with
the shared Error object/prototype constructor. The class adapter no longer
round-trips this result through `ExchangeError`. The public typed Exchange edge
retains the existing canonical error projection. There is no JSON projection,
second cancellation implementation, or shared model widening.

`tests/support/accepted_additional_receivers.rs` obtains the actual Scope from a
constructed KrakenExchange. Its eight receiver witnesses cover:

- `post_cancel`: real signed Spot HTTP, original IDs, endpoint, form field, nonce
  and API headers.
- `error_message`: original truthy native cells, UTF-16, nonfinite numbers,
  falsy fallback and original getter errors.
- `emit_error`: actual BaseExchange console and error event with context.
- `position_id`: retained UTF-16 concatenation, signed-zero coercion and original
  symbol-coercion errors.
- `optional_preference`: actual parent maps, original key/value cells, missing
  entries, nullish maps and noncallable member refusal.
- `imported`: both initialized endpoint tables, exact ordered contents and an
  undeclared import's ReferenceError. Module-array referential identity remains
  with the separately owned array work; this classification claims static value
  construction only.
- `member`: actual production dispatch, supplied receiver, exact native argument
  and result cells through the existing public socket delegate.
- `prototype`: shared prototype member on the matching socket kind, native this,
  arguments, return identity, original exceptions and scheduled future result.

Two additional constructed cancellation witnesses check the UTF-16 rejection
through the typed boundary and twelve original-message/falsy-fallback profiles.
`scripts/additional-receiver-source.cjs` extracts and executes the unchanged live
TypeScript cancel arrow and actual endpoint initializer for those twelve cases.
The original fourteen-case cancellation corpus retains all 49 effects, 70
settlement checkpoints and seven failures. Its fixture keeps the original error
alive while comparing fresh metadata identity, preventing allocator reuse from
invalidating that observation after the native-error cutover.

`scripts/additional-receiver-controls.py` injects one semantic fault per concrete
receiver and a separate retained-message narrowing fault. Each must compile and
fail its named real Scope witness; exact source restoration must pass. Compiler
errors and empty selections do not count. Receiver classifications pin the
complete compiler-selected impl bodies and explain each delegated or construction
role individually. Other receiver cohorts, array identity and whole-venue
accepted behavior remain independent obligations.
