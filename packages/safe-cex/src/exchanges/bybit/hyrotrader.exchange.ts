import { BybitExchange } from './bybit.exchange';
import { HYROTRADER_SIMULATOR_URL, HYROTRADER_SIMULATOR_WS_URL } from './bybit.types';

export class HyrotraderExchange extends BybitExchange {
  isDemo = true;

  get supportedCategories() {
    return ['linear'];
  }

  getBybitRestBaseUrl(network: 'livenet' | 'testnet') {
    const baseUrl = HYROTRADER_SIMULATOR_URL[network];
    if (!baseUrl) {
      throw new Error('HyroTrader simulator REST URL is not configured');
    }
    return baseUrl;
  }

  getBybitPublicWsUrl(category: 'linear' | 'inverse', network: 'livenet' | 'testnet') {
    if (category !== 'linear') {
      throw new Error('HyroTrader simulator only supports linear public WebSocket data');
    }
    const wsUrl = HYROTRADER_SIMULATOR_WS_URL.public.linear[network];
    if (!wsUrl) {
      throw new Error('HyroTrader simulator public WebSocket URL is not configured');
    }
    return wsUrl;
  }

  getBybitPrivateWsUrl(network: 'livenet' | 'testnet') {
    const wsUrl = HYROTRADER_SIMULATOR_WS_URL.private[network];
    if (!wsUrl) {
      throw new Error('HyroTrader simulator private WebSocket URL is not configured');
    }
    return wsUrl;
  }

  getSettleCoinsForCategory(category: string) {
    if (category === 'linear') return ['USDT'];
    this.emitAndLogError(`Unknown category ${category}`);
    throw new Error(`Unknown category ${category}`);
  }

  protected requiresOrderLinkId() {
    return true;
  }
}
