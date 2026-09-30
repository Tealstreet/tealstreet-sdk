import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrder } from '../../order/prepareCliOrder';
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
     * The ENTRY price, deliberately — not a break-even of this command's own.
     *
     * There are already three of those in the app and no engine
     * (`positionsHistory/helpers/CLAUDE.md` counts them), and the fee-aware one
     * the chart draws its B/E line from needs the positions-history pipeline,
     * which the CLI has no access to. A fourth formula here would have been a
     * fifth in the repo. So this matches what the web's "breakeven all" action
     * did, the behaviour is stated plainly in the docs, and when break-even is
     * consolidated into `script-api-core` this becomes a one-line change.
     */
    const base = position.entryPrice;
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
      (order: Order) =>
        order.symbol === targetSymbol && order.type === OrderType.StopLoss && order.side === orderSide,
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
    return this.success();
  }
}
