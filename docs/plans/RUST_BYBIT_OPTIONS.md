# Bybit options on the shared Plan and Context paths (slice 4)

Approved by claude-ddi2il (2026-10-24). Stacked on the slice 1c discovery PR.
It retires the options half of ruling D5 in
[RUST_BYBIT_PLAN_ADOPTION.md](RUST_BYBIT_PLAN_ADOPTION.md). Together with the
Hyro adoption (claude-fe7lgm), it lets one PR delete the legacy trading
executor.

## Scope

**4a: option trading.** `ordinary_command` stops routing option-symbol commands
to the legacy executor. Option PlaceOrders, AmendOrder, CancelOrders,
CancelSymbolOrders, SetLeverage and SetMarginType run through `start_ordinary`
→ `ordinary_gate` → the shared planners, with byte-identical bodies:
- `category: "option"`, `orderIv`, and `orderLinkId` always sent;
- no `positionIdx`;
- one create per option;
- cancel-batch per category;
- cancel-all `{category, symbol}`.

The gate becomes option-aware before any I/O, as the legacy pre-validation was:
- IV is accepted only on option symbols;
- `format_option` validation runs in the gate, so an invalid option in a mixed
  batch still makes no request (no mode read runs first).

Option leverage and margin settings stay a typed `Unsupported`/`NotSent`
refusal, now worded for options. A disabled option product refuses trade
commands `InvalidInput`/`NotSent`, as reads do. No option code rows are added without a cited
Bybit code.

**4b: option queries and seeds.**
- `fetchOptionChain` becomes a command Read Context (as for candles).
- `fetchAccountGreeks` becomes a signed command Read: the read key as
  `Secondary`, otherwise `Primary`.
- `fetchOptionExpiries` is an admitted no-I/O answer.
- Option ticker seeds use the Background discovery read.
- It prunes the `options.rs` fingerprints and the `Then::Options` sign-error
  `cx.fail`.

**Deletion (after 4a and the Hyro adoption).** Whoever lands second removes
the executor and the other reviews.
- **Removed:**
  - `on_trade_command`'s legacy body, `advance`, `on_trade_timer`,
    `on_trade_sign_failed` and `on_trade_response`;
  - `Request`/`Then::Trade`, `Call::ResolveMode`/`Call::Hyro` and the
    executor-only `Job` fields;
  - the legacy-only acknowledgement helpers, the `MarginMode*` wire types and
    `Rest::post`.
- **Kept:** the planners (`plan_job`, `plan_place`, `format_lots`,
  `format_option`, `plan_amend`, `plan_cancel`, `plan_leverage`), the local-row
  reducers, and the acknowledgement/code modules. Ordinary trading already
  uses them.
- **Changed:**
  - `CancelOrdersWithSymbols` gets an explicit typed refusal, since legacy
    returned without settling it.
  - The 16 executor ratchet sites in `trading.rs` are pruned.
  - `apply_local_write`'s `inputs.request` field is renamed, which clears the
    17th, lexical, site.
- `send`, `resend` and the `lib.rs` chokepoint stay until the account,
  identity, history and closed reads move.

## Caller changes (4a)

Option batches take the ordinary contract:
- a partially refused batch fails `KnownPartial` with `partialOrderIds` (D4,
  the same NX gate as ordinary);
- amend acknowledgements must name their target;
- an absent-order cancel answer fences and reconciles instead of marking the
  order Canceled (D2);
- writes resend only on a classified `10002`/`10004` refusal.

## 4a original REDs (`tests/products.rs`)

- O1 `option_orders_run_on_the_ordinary_plan_with_a_native_window`
- O2 `a_partially_refused_option_batch_fails_known_partial`
- O3 `an_option_amend_acknowledging_another_order_is_unknown`
- O4 `an_absent_option_cancel_fences_and_reads_instead_of_cancelling_locally`
- O5 `option_leverage_is_refused_as_an_option_setting`

All five fail on unchanged source.

Compatibility controls:
- `products.rs` 211, 394, 692 and 765, and the multi-category option tests;
- the combined-products harness's option parity (TS-identical IV and plain
  creates, the zero-write invalid mixed batch, options-disabled refusals).

No fixture changes: the strict codec accepts a create acknowledgement without
an `orderLinkId` echo and refuses only a mismatched one. Absent-order answers
are recognised only by native code (`110001`/`170213`); if Bybit options turn
out to use another code, it needs a cited row.
