# Bitget protective edit admission and put-back

Classic preset-only TP/SL edits use `modify-order` without price or size and
retain both presets. UTA partial TP/SL edits use `modify-strategy-order` and
retain each leg's trigger reference, execution type and limit price. A native
refusal leaves protection unchanged; an ambiguous reply never starts a cancel.

Full-mode UTA strategy edits, trailing edits and unsupported attached edits
retain the cancel/place path. Every destructive claim, including a 40009 re-sign,
checks the original rows, position mode, size rules and a genuine own-source
price received within five seconds. Store/frame receipt time alone is not price
evidence. Frame-fed engines obtain a matching public ticker before canceling;
there is no new read between cancel acknowledgment and replacement.

Known standalone strategy parameters permit one put-back after a definitive
replacement refusal. It restores the original venue quantity, full sentinel,
references, execution prices and trailing parameters. Success carries typed
`Restored`; a definitive lost protection carries `Unprotected` and re-place
instructions. Unknown outcomes never trigger a put-back or claim protection.
Put-back presentation is registered before dispatch for engine-sealed failures.
Attached fallbacks never guess a restore. Missing execution evidence refuses
before canceling. Existing command ownership covers the whole sequence.

UTA full-mode native modification remains excluded: the documented `qty` is
required but described as partial-mode only. UTA trailing native modification is
undocumented and excluded. These are contract limits, not inferred support.

Contracts: [Classic modify-order](https://www.bitget.com/docs/catalog/classic-contract-trade/classic-contract-trade),
[UTA strategy modification](https://www.bitget.com/docs/catalog/trading/strategy-trading).
