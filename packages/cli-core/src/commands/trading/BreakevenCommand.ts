import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrder } from '../../order/prepareCliOrder';
import { breakevenPrice } from '@tealstreet/script-api-core/src/order/breakevenPrice';
import { exchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import type { ExchangeName, Order } from '@tealstreet/safe-cex/src/types';

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
  readonly examples = ['be', 'be +3%', 'be -0.5%', 'be +100', 'be ETHUSDT'];
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
     * Shares the arithmetic with the web. The banked figure is supplied only
     * where `Position.realizedPnl` is worth placing a stop on, which is a
     * per-venue question answered by `reportsPositionRealizedPnl` — the same
     * metadata `trail` reads `supportsTrailingStopLoss` from. The evidence for
     * each venue lives on that field; it is not restated here.
     *
     * Read off the map rather than through `getExchangeMetadata`, which THROWS
     * on a name it does not know. An unrecognised venue must degrade to the
     * entry price, not crash the command that is trying to protect a position.
     *
     * The inverse guard is belt-and-braces. Inverse venues are flagged `false`
     * individually, but they inherit their linear sibling's metadata by spread,
     * so a future venue added the same way would arrive here flagged `true`
     * with a BASE-denominated numerator. Refusing on the market is the check
     * that cannot be forgotten in a metadata entry.
     *
     * **This number excludes FEES**, because a live position does not carry
     * them. The chart's B/E line reads the positions-history pipeline, which
     * does, so the two can differ by the round trip's fees. That difference
     * predates this command and is stated in the user docs.
     */
    const venue = exchange.name as ExchangeName | undefined;
    const trustsRealizedPnl = venue ? exchangeMetadata[venue]?.reportsPositionRealizedPnl === true : false;
    const market = exchange.store?.markets?.find((m) => m.symbol === targetSymbol);
    // A MISSING market is not a linear one. `!market?.isInverse` reads as the
    // same check and is the opposite failure: an absent market makes it true,
    // so a lookup that missed would trust a figure it could not verify.
    const isLinearMarket = market ? market.isInverse !== true : false;
    const banked =
      trustsRealizedPnl && isLinearMarket && Number.isFinite(position.realizedPnl) ? position.realizedPnl : undefined;

    const base = breakevenPrice({
      entryPrice: position.entryPrice,
      size: position.contracts,
      side: position.side,
      bankedPnl: banked,
    });
    // An offset is relative to BREAK-EVEN, not to the mark -- `be +3%` means
    // "3% the good side of flat". `processPrice` gives the same `+N` / `+N%` /
    // absolute grammar the rest of the CLI uses.
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
