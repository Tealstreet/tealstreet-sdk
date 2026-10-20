/**
 * Independent of the npm `version` in package.json. Bumped only when the
 * runtime-visible CoreApi contract changes (additions allowed in minor,
 * breaking changes require major). Phase 1 ships the full read-side
 * surface + ApiProxy seam + targetCheck.
 *
 * 1.0.0 — Phase 1: read-side getters, ApiProxy + InProcessProxy, AdapterContext,
 *         targetCheck, accountResolver, utils, constants. Action/io/storage
 *         types ship but impls land in later phases.
 * 1.1.0 — Add typed Bybit BBO and TickSize execution controls to place-order
 *         inputs. Hosts pass these POJO fields through unchanged.
 * 1.1.1 — No contract change. `script.ts` gained a corrected doc-path
 *         reference when the types-bundle test moved to
 *         `packages/script-authoring`; bumped because `check:api-versions`
 *         watches the file, and a patch bump is cheaper than teaching the
 *         gate to distinguish comments from surface.
 * 1.2.0 — Three ADDITIVE order-flow extensions, all needed so the OEMS slice
 *         could be expressed on this surface rather than beside it:
 *         `CancelFilter.ids` cancels exactly the enumerated orders (the other
 *         fields are ignored when it is present); `setLeverage` takes an
 *         optional `side` for one leg of a hedge-mode position; and
 *         `UpdateOrderPatch` replaces `UpdateOrderOpts` in `updateOrder`,
 *         making `order` optional because every host discards it and
 *         re-resolves by id. Existing callers keep typechecking.
 * 1.3.0 — `PlaceOrderOptions` gains `triggerPrice` and `triggerType`, so a
 *         standalone stop-like order can be expressed here at all. safe-cex's
 *         `PlaceOrderOpts` has always carried both; omitting them meant every
 *         caller placing a bare stop had to go around this surface, which is
 *         how the CLI's `stop` command ended up sending an unrounded trigger.
 *         `triggerPrice` is rounded to the venue tick like `price`.
 * 1.3.1 — `api.constants.ExchangeName` gains `txflow`, matching the
 *         safe-cex ExchangeName union for the dev-only TxFlow scaffold.
 * 1.3.2 — `api.constants.ExchangeName` gains `binanceinverse`, matching the
 *         safe-cex ExchangeName union for Binance COIN-M inverse support.
 * 1.3.3 — `api.constants.ExchangeName` gains `binanceoptions`, matching the
 *         safe-cex ExchangeName union for Binance European options.
 */
export const CORE_API_VERSION = '1.3.3';
