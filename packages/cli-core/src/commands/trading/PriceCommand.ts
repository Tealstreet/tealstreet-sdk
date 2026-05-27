import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import type { Ticker } from '@tealstreet/safe-cex/src/types';

/**
 * Base class for price commands
 */
abstract class BasePriceCommand extends TradingBaseCommand {
  abstract get priceKey(): keyof Ticker;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }

    const args = this.getRawArgs(ctx);
    const symbolsOfInterest = args[1] ? this.validateSymbols(exchange, args[1].split(',')) : [ctx.symbol];

    const resultMessages: string[] = [];
    for (const symbol of symbolsOfInterest) {
      const price = exchange.store?.tickers?.find((ticker) => ticker.symbol === symbol)?.[this.priceKey];
      resultMessages.push(
        price
          ? `${this.priceKey[0].toUpperCase() + this.priceKey.slice(1)} Price: ${symbol} @ ${String(price)}`
          : `Ticker ${symbol} was not found or is not loaded yet`,
      );
    }

    this.print(ctx, resultMessages);
    return this.success();
  }
}

/**
 * Last price command
 */
export class LastPriceCommand extends BasePriceCommand {
  readonly name = 'last-price';
  readonly description = 'Show last price';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['lp', 'l'];
  readonly examples = ['last-price', 'lp', 'l', 'last-price BTCUSDT'];
  readonly requiresConnection = true;

  get priceKey(): keyof Ticker {
    return 'last';
  }
}

/**
 * Mark price command
 */
export class MarkPriceCommand extends BasePriceCommand {
  readonly name = 'mark-price';
  readonly description = 'Show mark price';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['mp', 'm'];
  readonly examples = ['mark-price', 'mp', 'm', 'mark-price BTCUSDT'];
  readonly requiresConnection = true;

  get priceKey(): keyof Ticker {
    return 'mark';
  }
}

/**
 * Index price command
 */
export class IndexPriceCommand extends BasePriceCommand {
  readonly name = 'index-price';
  readonly description = 'Show index price';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['ip', 'i'];
  readonly examples = ['index-price', 'ip', 'i', 'index-price BTCUSDT'];
  readonly requiresConnection = true;

  get priceKey(): keyof Ticker {
    return 'index';
  }
}
