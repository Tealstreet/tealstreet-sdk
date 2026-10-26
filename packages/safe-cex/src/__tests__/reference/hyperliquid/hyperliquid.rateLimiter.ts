import { ENDPOINTS, INFO_TYPES } from '../../../exchanges/hyperliquid/hyperliquid.types';

const REST_WEIGHT_LIMIT_PER_MINUTE = 1200;
const REST_WEIGHT_WINDOW_MS = 60_000;

const LOW_WEIGHT_INFO_TYPES = new Set([
  INFO_TYPES.ALL_MIDS,
  INFO_TYPES.L2_BOOK,
  INFO_TYPES.ORDER_STATUS,
  INFO_TYPES.PERPS_CLEARINGHOUSE_STATE,
  INFO_TYPES.SPOT_CLEARINGHOUSE_STATE,
  'exchangeStatus',
]);

const EXTRA_WEIGHT_PER_20_ITEMS_INFO_TYPES = new Set([
  INFO_TYPES.HISTORICAL_ORDERS,
  INFO_TYPES.USER_FILLS,
  INFO_TYPES.USER_FILLS_BY_TIME,
  INFO_TYPES.FUNDING_HISTORY,
  INFO_TYPES.USER_FUNDING,
  INFO_TYPES.USER_NON_FUNDING_LEDGER_UPDATES,
  'recentTrades',
  'twapHistory',
  'userTwapSliceFills',
  'userTwapSliceFillsByTime',
  'delegatorHistory',
  'delegatorRewards',
  'validatorStats',
]);

type HyperliquidRequestConfig = {
  url?: string;
  data?: unknown;
};

const isEndpoint = (url: string | undefined, endpoint: string): boolean => url === endpoint || Boolean(url?.endsWith(endpoint));

const parseRequestData = (requestData: unknown): any => {
  if (typeof requestData !== 'string') return requestData;

  try {
    return JSON.parse(requestData);
  } catch {
    return requestData;
  }
};

const getArrayLength = (value: unknown): number => (Array.isArray(value) ? value.length : 0);

const getExchangeBatchLength = (action: any): number => {
  switch (action?.type) {
    case 'order':
      return getArrayLength(action.orders);
    case 'cancel':
      return getArrayLength(action.cancels);
    case 'cancelByCloid':
      return getArrayLength(action.cancels);
    case 'batchModify':
      return getArrayLength(action.modifies);
    default:
      return 0;
  }
};

const getInfoBaseWeight = (type: unknown): number => {
  if (type === INFO_TYPES.USER_ROLE) return 60;
  if (LOW_WEIGHT_INFO_TYPES.has(type as string)) return 2;
  return 20;
};

const getInfoExtraWeight = (type: unknown, responseLimit: unknown): number => {
  const limit = Number(responseLimit);
  if (!Number.isFinite(limit) || limit <= 0) return 0;

  if (type === INFO_TYPES.CANDLE_SNAPSHOT) return Math.floor(limit / 60);
  if (EXTRA_WEIGHT_PER_20_ITEMS_INFO_TYPES.has(type as string)) return Math.floor(limit / 20);
  return 0;
};

export const getHyperliquidRestRequestWeight = (config: HyperliquidRequestConfig): number => {
  const data = parseRequestData(config.data);

  if (isEndpoint(config.url, ENDPOINTS.EXCHANGE)) {
    const batchLength = getExchangeBatchLength(data?.action);
    return 1 + Math.floor(batchLength / 40);
  }

  if (isEndpoint(config.url, ENDPOINTS.INFO)) {
    return getInfoBaseWeight(data?.type) + getInfoExtraWeight(data?.type, data?.limit);
  }

  return 1;
};

export class RateLimiter {
  private nextAvailableAt = 0;
  private tail: Promise<void> = Promise.resolve();
  private readonly msPerToken: number;

  constructor(capacity: number = REST_WEIGHT_LIMIT_PER_MINUTE, windowMs: number = REST_WEIGHT_WINDOW_MS) {
    this.msPerToken = windowMs / capacity;
  }

  async waitForToken(weight: number = 1): Promise<void> {
    const normalizedWeight = Number.isFinite(weight) && weight > 0 ? weight : 1;

    const run = this.tail.then(async () => {
      const now = Date.now();
      const startAt = Math.max(now, this.nextAvailableAt);
      const waitMs = startAt - now;
      this.nextAvailableAt = startAt + normalizedWeight * this.msPerToken;

      if (waitMs > 0) {
        await new Promise<void>((resolve) => setTimeout(resolve, waitMs));
      }
    });

    this.tail = run.catch(() => undefined);
    return run;
  }
}

export const hyperliquidRestRateLimiter = new RateLimiter();
