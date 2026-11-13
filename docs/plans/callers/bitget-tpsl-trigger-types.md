# Bitget TP/SL trigger choices

The Classic entry-preset trigger default is **UNPROVEN**. No authenticated
DEMO account was available for a capture, and no placement or cancellation was
attempted. The Classic placement documentation exposes preset prices without a
trigger selector and does not establish a default. An example response showing
Mark is evidence for that response, not a venue-wide default.

| Native path | Selectable references | Wire fields |
| --- | --- | --- |
| Classic entry presets | None; leave the venue default unspecified | Preset prices only |
| Classic position TP/SL | Last, Mark | `triggerType=fill_price\|mark_price` on each `place-tpsl-order` |
| UTA entry presets | Last, Mark | `tpTriggerBy` / `slTriggerBy` = `market\|mark` |
| UTA position TP/SL | Last, Mark | `tpTriggerBy` / `slTriggerBy` = `market\|mark` on `place-strategy-order` |

The existing Classic position route submits each full/sized leg through
`place-tpsl-order`; it does not introduce a `place-pos-tpsl` route. That alternative
venue endpoint documents equivalent per-leg `stopSurplusTriggerType` and
`stopLossTriggerType` fields. No path offers Index. Native admission refuses an
unsupported selection before sending it, including an explicit Classic entry
preset selection. Omitting a Classic selection continues to request the venue's
unspecified default without claiming a particular reference.

Classic preset children read `presetStopSurplusTriggerType` and
`presetStopLossTriggerType` (`fill_price` / `mark_price`); the order-detail spellings
`presetStopSurplusType` / `presetStopLossType` are also accepted. Missing, blank or
unrecognized references remain absent. Protective edits that require a known
reference retain their existing refusal rather than guessing.

## UI consumer integration

`getExchangeMetadata(name, { bitgetAccountMode, tpslOrderPath })` returns the
path-specific list for Bitget, USDC and inverse names. `bitgetAccountMode` is the
detected `classic` / `uta` mode, not a truthy unified-account flag;
`tpslOrderPath` is `entry` (the default) or `position`. Unknown mode is conservative
for entries. UTA and position metadata retain the SDK's explicit Mark default,
which the writer serializes, while permitting Last. The native inverse engine
has no UTA route, so inverse UTA context offers no selections; Classic inverse
position TP/SL still offers both references. This retains existing routing
eligibility, including the UTA COIN-M restrictions in the current venue docs.

The static metadata list is empty so an older consumer cannot advertise a
Classic preset choice the venue cannot honor. The web dropdown lives outside
this SDK mirror: its consumer must pass the detected mode and current path,
hide the selector for an empty list, clear stale explicit preset selections,
and leave an absent read reference unlabeled rather than defaulting it to Last
or Mark. Until that consumer adopts this call, the conservative static list also
hides the selectable UTA/position controls.

## Protocol references

- [Classic placement, pending and detail responses](https://www.bitget.com/docs/catalog/classic-contract-trade/classic-contract-trade)
- [Classic individual and simultaneous TP/SL](https://www.bitget.com/docs/catalog/classic-contract-plan/classic-contract-plan)
- [UTA entry placement](https://www.bitget.com/docs/catalog/trading/order-management)
- [UTA position strategies](https://www.bitget.com/legacy-docs/uta/strategy/Place-Strategy-Order)
