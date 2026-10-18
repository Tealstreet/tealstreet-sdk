# Coinbase assigned divergence audit

The exact two rows come from `assignments/codex-0efu4x.json` in the non-UTF-8
assignment index. They do not classify the rest of this venue.

| Owner | Ruling | Production observation |
| --- | --- | --- |
| `pemToPkcs8Der` | FIX at signing assembly; ACCEPT the unused unit-error facade | The actual CryptoHost calls the shared PEM algorithm with a decoder retaining its native cause, but exposed the codec's diagnostic instead of the maintained `atob` diagnostic. Both constructed Advanced and Spot WS authentication and REST placement expose that message. Keep the native cause and `InvalidCharacterError` name; preserve the two source message branches. The metadata-free standalone facade is not used by production signing. |
| `buildCoinbaseStarbasePlaceOrder` | ACCEPT, inactive typed builder | Current source and native production placement use legacy REST. The fixed production selector disables activation and selects legacy writes. The separate inert TS builder preserves arbitrary caller own fields/order; the Rust blocked gate borrows the exact typed caller, including numeric bits, but its typed serialization cannot include caller-only fields. There is no live HTTP, socket or Store consumer of this builder. This ruling does not authorize those losses on a future activated Starbase path. |

`tests/exchange_binding.rs` adds independent named production witnesses without
changing any existing source corpus or identity assertion. The separate
`scripts/divergence-binding.cjs` executes the actual maintained credential and
signing functions, including real `atob` and the ES256 selection, for eleven
explicit malformed PEM payloads. It also executes the actual inert Starbase
builder, asserts source caller identity/key order, and checks the real source
placement member still posts to the legacy endpoint.

Malformed PEM tests retain exact input/ID identity and observe both public
signing hooks and actual validated REST placement. A failed signer must send no
HTTP and must create no optimistic order. The inert-builder witness additionally
retains the exact native caller address, signed-zero bits and opposite booleans,
then constructs both live variants and observes their actual REST payload and
Store order. Existing builder, signing, alias and numeric corpora are unchanged.

The error wording follows the maintained Node `atob` oracle (invalid alphabet or
padding versus a valid alphabet with one leftover character). Browser-specific
DOMException object identity/stack is not claimed. Original native decoder errors
remain available as `ExchangeError.origin`. The standalone unit-error helper's
existing rejection contract remains unchanged. No async wrapper family changes;
no executor or per-method spawning is introduced.

`scripts/divergence-binding-controls.py` compiles independent production faults,
requires a named assertion failure, restores the exact bytes in `finally`, and
reruns the same witness. The separate inactive-builder control poisons that
builder while rerunning live placement, proving its lack of production
reachability rather than hiding an observable failure.
