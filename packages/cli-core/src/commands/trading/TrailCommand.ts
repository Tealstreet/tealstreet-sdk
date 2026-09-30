import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrder } from '../../order/prepareCliOrder';
import { getExchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

/**
 * A trailing stop on the open position — the "runner" leg.
 *
 * Asked for as "after TP3, would like to have a runner - how do I do that?",
 * with a screenshot of another platform's ladder whose last leg is a trailing
 * stop armed at a profit threshold (Discord, 2026-09-29).
 *
 * The arming half is `when`: `when (price >= 0.0716) (trail 20%)` arms and
 * returns, so the runner waits without holding the prompt. `wait` was the
 * original suggestion and cannot do it — it blocks its own line and is capped
 * at ten minutes, while a take-profit sits unfilled for hours.
 *
 * **Distance is from the CURRENT price, not from entry**, matching how venues
 * define a trailing stop: it follows the market and never moves against you.
 *
 * **The whole position trails.** Bybit implements this through
 * SET_TRADING_STOP, which is position-level and takes no amount, so a
 * partial-size runner is not expressible there. Sizing the runner is done by
 * taking profit on the rest first, which is what the ladder does anyway.
 */
export class TrailCommand extends TradingBaseCommand {
  readonly name = 'trail';
  readonly description = 'Place a trailing stop on the open position';
  readonly category = CommandCategory.Trading;
  readonly aliases = [];
  readonly examples = ['trail 20%', 'trail 500', 'trail 20% ETHUSDT', 'when (price >= 0.0716) (trail 20%)'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) return this.error('Not connected to exchange');
    if (!exchange.placeOrder) return this.error('Exchange does not support placing orders');

    // Fail by NAME rather than letting the venue reject an order it has no
    // endpoint for. The metadata flag is the authority on which venues those
    // are — do not restate the list here, it has already drifted once.
    const metadata = exchange.name ? getExchangeMetadata(exchange.name as ExchangeName) : undefined;
    if (metadata && !metadata.supportsTrailingStopLoss) {
      return this.error(`${exchange.name} does not support trailing stops.`);
    }

    const args = this.getRawArgs(ctx);
    const targetSymbol = this.getTargetSymbol(ctx, []);

    const distanceArg = args.find((arg, index) => index > 0 && this.looksLikePrice(arg));
    if (!distanceArg) {
      return this.error(`trail: no distance in "${ctx.raw}". Expected e.g. \`trail 20%\` or \`trail 500\`.`);
    }

    const positions = (exchange.store?.positions ?? []).filter(
      (position) => position.symbol === targetSymbol && position.contracts !== 0,
    );
    if (positions.length === 0) return this.error(`No position found for ${targetSymbol}`);
    if (positions.length > 1) {
      return this.error(`More than one position is open on ${targetSymbol}; close one first.`);
    }
    const position = positions[0];

    const ticker = exchange.store?.tickers?.find((t) => t.symbol === targetSymbol);
    const last = ticker?.last;
    if (!last) return this.error(`Current price is not available for ticker ${targetSymbol}, try again.`);

    // A percentage is of the current PRICE, so `trail 20%` is the same 20% the
    // venue's own field means. An absolute is taken as-is.
    const distance = distanceArg.trim().endsWith('%')
      ? (last * Math.abs(parseFloat(distanceArg))) / 100
      : Math.abs(parseFloat(distanceArg));
    if (!Number.isFinite(distance) || distance <= 0) {
      return this.error(`trail: "${distanceArg}" is not a usable distance.`);
    }

    const isLong = position.side === PositionSide.Long;
    // The stop sits BEHIND the position: below a long, above a short.
    const price = isLong ? last - distance : last + distance;
    if (price <= 0) {
      return this.error(`trail: a distance of ${distanceArg} puts the stop at ${price}, below zero.`);
    }

    await exchange.placeOrder(
      prepareCliOrder(exchange, {
        symbol: targetSymbol,
        side: isLong ? OrderSide.Sell : OrderSide.Buy,
        type: OrderType.TrailingStopLoss,
        timeInForce: OrderTimeInForce.GoodTillCancel,
        reduceOnly: true,
        price,
        amount: 0,
      }),
    );

    this.printSuccess(ctx, `Trailing stop ${distanceArg} behind ${last} (currently ${price})`);
    return this.success();
  }
}
