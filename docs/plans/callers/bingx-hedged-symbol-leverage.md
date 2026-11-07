# BingX shared leverage slider in hedge mode

Rust BingX `setLeverage(symbol, leverage)` with no side now updates LONG and
then SHORT under the existing serial trading Plan. BingX's documented
`POST /openApi/swap/v2/trade/leverage` requires LONG/SHORT in hedge mode;
BOTH is reserved for one-way mode. The retained TypeScript reference rejects
a missing hedge side. Rust deliberately accepts the shared-symbol intent instead.

Source: [BingX's API reference](https://github.com/BingX-API/api-ai-skills/blob/main/skills/swap-trade/api-reference.md#20-set-leverage).

Affected caller: NX Margin's shared LeverageSlider, which supplies no side when
there is no open position and independent hedge leverage is not enabled for
that margin type. No NX or public SDK API change is required. Explicit sides
still update only that side; one-way mode still sends one BOTH request.

Each rail and matching open position is updated only after its own accepted
response. A LONG refusal stops before SHORT. The command succeeds only after
both acknowledgements; a later SHORT refusal, malformed reply, signing failure
or transport loss reports the earlier LONG update and the unconfirmed SHORT
update, retaining the underlying error and its classification. There is no
automatic rollback: LONG's acknowledged value stays published, and SHORT's
unacknowledged value is retained until reconciliation.

Callers must handle rejection as a possible partial settings change rather than
assuming neither side changed. Do not repeat the entire operation blindly on
an uncertain error; refresh symbol settings first. This adds no deletion or
rollout prerequisite.
