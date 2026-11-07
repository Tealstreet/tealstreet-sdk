import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrder } from '../../order/prepareCliOrder';
import { breakevenPrice } from '@tealstreet/script-api-core/src/order/breakevenPrice';
import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import type { Order } from '@tealstreet/safe-cex/src/types';

/**
 * Move the protective stop to break-even.
 *
 * Asked for in these words: "would like to utilise wait so then TP1 is hit,
 * price moves to Entry - ideally entry + say 3% (to allow for slippage)"
 * (Discord, 2026-09-29). `be` was typed twice before it was reported missing,
 * so the name is the one the user reached for.
 *
 * **It cancels STOP LOSSES only, never take-profits.** The obvious spelling of
 * this by hand is `cancel stops; stop $entry`, and `cancel stops` uses the
 * repo's `isStop` — "not a limit and not a market" — which sweeps up every
 * TP ladder rung along with the protective stop. Doing that here would close
 * the targets the position exists to reach.
 */
export class BreakevenCommand extends TradingBaseCommand {
  readonly name = 'be';
  readonly description = 'Move the stop loss to break-even';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['breakeven'];
  readonly examples = ['be', 'be +3%', 'be +100', 'be ETHUSDT'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) return this.error('Not connected to exchange');
    if (!exchange.placeOrder) return this.error('Exchange does not support placing orders');

    const args = this.getRawArgs(ctx);
    const targetSymbol = this.getTargetSymbol(ctx, []);

    const positions = (exchange.store?.positions ?? []).filter(
      (position) => position.symbol === targetSymbol && position.contracts !== 0,
    );
    if (positions.length === 0) return this.error(`No position found for ${targetSymbol}`);
    if (positions.length > 1) {
      return this.error(`More than one position is open on ${targetSymbol}; close one or use \`stop\` directly.`);
    }
    const position = positions[0];

    const ticker = exchange.store?.tickers?.find((t) => t.symbol === targetSymbol);
    const mark = ticker?.mark ?? ticker?.last;
    if (!mark) return this.error(`Current price is not available for ticker ${targetSymbol}, try again.`);

    /**
     * Supplies NO `bankedPnl`, so the answer is the ENTRY price.
     *
     * `Position.realizedPnl` is the field that looks like the missing input.
     * It briefly was wired in here behind a per-venue capability flag; that
     * was reverted because three of the five entries were assertions and one
     * was contradicted by our own measurements:
     *
     *   - bitunix — `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md` measures
     *     `realizedPNL` over a funding-bearing 90-day window and concludes it
     *     is net of fees AND funding. Captured `position` frames in
     *     `bitunix.ws-private.ts` show it equal to `fee` on a fresh open.
     *   - bybit — `curRealisedPnl` appears in three mapper lines and nowhere
     *     else: no fixture, test, comment or doc says what it means. What
     *     evidence exists points the other way, since `bybit/CLAUDE.md` and
     *     the journal adapter both treat bybit's position-level realised
     *     figure as net of fees and funding.
     *   - paradex — inferred from the field's NAME, which is not evidence.
     *
     * And on the venues that do report something: lifetime per symbol on
     * asterdex, surviving the close on lighter, funding and fees folded in on
     * bitget, base-denominated against a quote entry on every inverse
     * contract, absent entirely on binance, okx, blofin, kraken, woo and
     * coinbase among others.
     *
     * The web app does not read this field either. It derives banked PnL and
     * fees from the user's own FILLS (`processTrades`), reconstructing each
     * position lifecycle itself. That is the only sound route, and the CLI
     * cannot take it yet — it never calls `fetchHistoricalFills`. Until it
     * does, entry is the honest answer and the user docs say so.
     */
    const base = breakevenPrice({
      entryPrice: position.entryPrice,
      size: position.contracts,
      side: position.side,
    });
    // An offset is relative to BREAK-EVEN, not to the mark, and it is a plain
    // direction rather than a side-aware one: `+` is up the price axis for
    // both sides. On a long `be +3%` locks a little in; on a short the
    // profitable side is down, so the same buffer is `be -3%` and `+3%` there
    // WIDENS the stop. `processPrice` gives the same `+N` / `+N%` / absolute
    // grammar the rest of the CLI uses.
    const offsetArg = args.find((arg, index) => index > 0 && this.looksLikePrice(arg));
    const price = offsetArg ? parseFloat(this.processPrice(offsetArg, base)) : base;
    if (!Number.isFinite(price) || price <= 0) {
      return this.error(`Break-even price for ${targetSymbol} is not a usable price (${price}).`);
    }

    const isLong = position.side === PositionSide.Long;
    const orderSide = isLong ? OrderSide.Sell : OrderSide.Buy;

    // Cancel the protective stop we are replacing. Take-profits are left alone
    // on purpose -- see the class comment.
    const existingStops = (exchange.store?.orders ?? []).filter(
      (order: Order) => order.symbol === targetSymbol && order.type === OrderType.StopLoss && order.side === orderSide,
    );
    if (existingStops.length > 0 && exchange.cancelOrders) {
      await exchange.cancelOrders(existingStops);
    }

    // Same rule as `stop`: what the order IS depends on which side of the mark
    // it sits, because that is when it fires.
    const type = isLong
      ? price > mark
        ? OrderType.TakeProfit
        : OrderType.StopLoss
      : price <= mark
        ? OrderType.TakeProfit
        : OrderType.StopLoss;

    await exchange.placeOrder(
      prepareCliOrder(exchange, {
        symbol: targetSymbol,
        side: orderSide,
        type,
        timeInForce: OrderTimeInForce.GoodTillCancel,
        reduceOnly: true,
        price,
        triggerPrice: price,
        amount: 0,
      }),
    );

    const moved = price === base ? '' : ` (break-even ${base}, offset ${offsetArg})`;
    this.printSuccess(ctx, `Stop moved to ${price}${moved}`);
    // An UNDERWATER position has its break-even on the far side of the mark, so
    // the order above is a take-profit and the protective stop this command
    // just cancelled has not been replaced by one. Saying "Stop moved" and
    // leaving it there would report protection the position does not have.
    if (type === OrderType.TakeProfit) {
      this.printWarning(
        ctx,
        `Break-even for ${targetSymbol} is on the far side of the market, so that is a reduce-only ` +
          `take-profit at ${price}, not a stop — the position has no stop loss now.`,
      );
    }
    return this.success();
  }
}
