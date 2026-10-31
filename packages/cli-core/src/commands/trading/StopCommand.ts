import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrder } from '../../order/prepareCliOrder';
import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';

/**
 * Stop command - creates stop orders
 */
export class StopCommand extends TradingBaseCommand {
  readonly name = 'stop';
  readonly description = 'Create a stop order';
  readonly category = CommandCategory.Trading;
  readonly aliases = [];
  readonly examples = [
    // `at` is optional; the bare form is listed first because it is what
    // people type before reading anything.
    'stop 18500',
    'stop at 0.1%',
    'stop at 0.1% ETHUSDT',
    'stop at 18500',
    'stop buy at +100',
    'stop sell at 1%',
    'stop $500 at 1%',
    'stop $500 at 1% ETHUSDT',
    'stop buy $500 at 1%',
    // Limit stops: trigger keyword separates trigger from limit price.
    'stop at 0.1% trigger 18500',
    'stop buy at +100 trigger 19500',
    'stop 500$ at 1% trigger 19000',
    'stop buy 500$ at 1% trigger 1500',
  ];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }
    if (!exchange.placeOrder) {
      return this.error('Exchange does not support placing orders');
    }

    const args = this.getRawArgs(ctx);
    const targetSymbol = this.getTargetSymbol(ctx, ['buy', 'sell', 'at', 'trigger']);

    const isBuySellFirstArg = ['buy', 'sell'].includes(String(args[1]));
    const sizePosition = this.looksLikeSize(args[1]) ? 1 : isBuySellFirstArg && this.looksLikeSize(args[2]) ? 2 : -1;

    // `at` is OPTIONAL. The price is whatever follows it, or -- when it is
    // absent -- the first price-shaped argument after the side and the size. A
    // bare number can never be a size (`looksLikeSize` requires a `$`), so
    // `stop 18500` is unambiguous. Reported from Discord 2026-09-29: `stop
    // 0.049570` is the natural thing to type, and it fell through to the
    // catch-all at the bottom of this method, which named neither the problem
    // nor the expected form.
    const atIdx = this.findArgIndex(args, 'at');
    const pricePosition =
      atIdx !== -1
        ? atIdx + 1
        : args.findIndex((arg, index) => index > 0 && index !== sizePosition && this.looksLikePrice(arg));

    if (pricePosition > 0 && args[pricePosition] !== undefined) {
      const ticker = exchange.store?.tickers?.find((ticker) => ticker.symbol === targetSymbol);
      const currentPrice = ticker?.last;
      if (!currentPrice) {
        return this.error(`Current price is not available for ticker ${targetSymbol}, try again.`);
      }

      const side = this.hasArgValue(args, 'buy') ? OrderSide.Buy : this.hasArgValue(args, 'sell') ? OrderSide.Sell : undefined;
      const limitPrice = parseFloat(this.processPrice(args[pricePosition], currentPrice));

      // Optional `trigger <abs>` keyword turns this into a true limit
      // stop. Without it, trigger == limit (market stop). Spec: trigprice
      // must be absolute, so processPrice is intentionally NOT applied.
      const triggerIdx = this.findArgIndex(args, 'trigger');
      let triggerPrice = limitPrice;
      if (triggerIdx !== -1) {
        const triggerArg = args[triggerIdx + 1];
        if (!triggerArg) {
          return this.error('trigger: missing absolute price argument');
        }
        const parsed = parseFloat(triggerArg);
        if (!Number.isFinite(parsed) || parsed <= 0) {
          return this.error(`trigger: '${triggerArg}' must be an absolute positive price`);
        }
        triggerPrice = parsed;
      }

      // Use mark price if available, otherwise use last price
      const mark = ticker?.mark ?? currentPrice;

      const positionSides: { side: PositionSide }[] = [];
      if (side) {
        positionSides.push({ side: side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long });
      } else {
        // When side is not specified, find all positions for the symbol
        const positions = (exchange.store?.positions ?? []).filter(
          (position) => position.symbol === targetSymbol && position.contracts !== 0,
        );
        if (positions.length > 1) {
          return this.error('More than one position are opened, specify <buy|sell> in a command.');
        }
        positionSides.push(...positions.map((p) => ({ side: p.side })));
      }

      if (positionSides.length === 0) {
        return this.error(`No position found for ${targetSymbol}`);
      }

      for (const { side } of positionSides) {
        const isLong = side === PositionSide.Long;
        // TP vs SL is decided by trigger vs mark — that's when the
        // order fires, regardless of what limit price it routes to.
        const stopOrder = {
          symbol: targetSymbol,
          side: isLong ? OrderSide.Sell : OrderSide.Buy,
          type: isLong
            ? triggerPrice > mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss
            : triggerPrice <= mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          reduceOnly: true,
          price: limitPrice,
          triggerPrice,
          amount:
            sizePosition !== -1 && sizePosition < args.length
              ? this.parseSizeAndConvertIfUsd(exchange, targetSymbol, args[sizePosition])
              : 0,
        };
        await exchange.placeOrder(prepareCliOrder(exchange, stopOrder));
      }

      this.printSuccess(ctx, triggerIdx !== -1 ? 'Placing a limit stop order' : 'Placing a stop order');
      return this.success();
    }

    // Naming the expected form matters more than it looks: the previous message
    // here was the bare string "Not implemented", which a user reasonably read
    // as "this command does not exist yet" and gave up on.
    const usage = `stop: could not find a trigger price in "${ctx.raw}". Expected e.g. \`stop 18500\`, \`stop at -1%\`, \`stop sell $500 at 1%\`.`;
    this.printError(ctx, usage);
    return this.error(usage);
  }
}
