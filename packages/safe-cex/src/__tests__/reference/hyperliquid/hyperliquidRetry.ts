import type { IsReadRequest } from '../../../utils/retryReadsOnly';

// Every Hyperliquid request is a POST; `/exchange` is the only path that writes.
export const isHyperliquidRead: IsReadRequest = (config) => config.url?.split('?')[0] !== '/exchange';
