import { CrossTradeManager } from '../../crossTrade/runtime/CrossTradeManager';
import { exchangeWorkers } from '../store.webworker';
import { crossTradePlaceOrder } from './crossTradePlaceOrder';

export { CrossTradeManager };

export const crossTradeManagerInstance = new CrossTradeManager({
  getExchange: (account) => exchangeWorkers[account]?.exchange ?? undefined,
  placeOrder: crossTradePlaceOrder,
});
