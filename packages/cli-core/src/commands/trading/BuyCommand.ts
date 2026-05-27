import { BaseBuySellCommand } from './BaseBuySellCommand';
import { OrderSide } from '@tealstreet/safe-cex/src/types';

/**
 * Buy command - places market or limit buy orders
 */
export class BuyCommand extends BaseBuySellCommand {
  readonly name = 'buy';
  readonly description = 'Buy market or limit order';
  readonly aliases = ['b'];
  readonly examples = [
    'buy $100',
    'buy $100 reduce',
    'buy WIFUSDT $100',
    'buy $100 WIFUSDT',
    'buy WIFUSDT,ENAUSDT $100',
    'buy $100 at 50000',
    'buy $100 at 50000 ETHUSDT',
  ];

  protected get side(): OrderSide {
    return OrderSide.Buy;
  }
}
