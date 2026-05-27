import { BaseBuySellCommand } from './BaseBuySellCommand';
import { OrderSide } from '@tealstreet/safe-cex/src/types';

/**
 * Sell command - places market or limit sell orders
 */
export class SellCommand extends BaseBuySellCommand {
  readonly name = 'sell';
  readonly description = 'Sell market or limit order';
  readonly aliases = ['s'];
  readonly examples = [
    'sell $100',
    'sell $100 reduce',
    'sell WIFUSDT $100',
    'sell $100 WIFUSDT',
    'sell WIFUSDT,ENAUSDT $100',
    'sell $100 at 50000',
    'sell $100 at 50000 ETHUSDT',
  ];

  protected get side(): OrderSide {
    return OrderSide.Sell;
  }
}
