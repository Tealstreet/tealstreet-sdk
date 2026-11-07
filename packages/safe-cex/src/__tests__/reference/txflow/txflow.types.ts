export const TXFLOW_MAINNET_REST_URL = 'https://rpc.txflow.com';
export const TXFLOW_TESTNET_REST_URL = 'https://rpc.txflow-testnet.xyz';
export const TXFLOW_API_VERSION = '2026-08-16';

export interface TxFlowInstrument {
  index: number;
  name: string;
  baseCurrency: string;
  quoteCurrency: string;
  currencyType: string;
  basePrecision: string;
  priceTick: string;
  maxLeverage: number;
  haltTrading: boolean;
  delisted: boolean;
  onlyIsolated: boolean;
  useNotionalMargin: boolean;
  maxLimitOrderSize?: number;
  maxMarketOrderSize?: number;
}

export interface TxFlowContext {
  funding?: string;
  markPx?: string;
  oraclePx?: string;
  openInterest?: string;
  midPx?: string;
  lastPrice?: string;
  prevDayPx?: string;
  dayNtlVlm?: string | null;
  dayBaseVlm?: string | null;
  nextFundingTime?: number;
}
export interface TxFlowTicker {
  instrumentId: number;
  lastPrice?: string;
  prevDayPx?: string;
  markPx?: string;
  oraclePx?: string;
  openInterest?: string;
  dayNtlVlm?: string;
  coinVlm?: string;
  nodeCtx?: TxFlowContext;
}
export interface TxFlowCandle {
  t: number; s: string; i: string; o: string | number; h: string | number;
  l: string | number; c: string | number; v: string | number;
}
export const TXFLOW_INTERVALS = new Set(['1m', '3m', '5m', '15m', '30m', '1h', '2h', '4h', '8h', '12h', '1d']);
export const TXFLOW_PUBLIC_READS = new Set(['perpMeta', 'activeAssetCtx', 'candleSnapshot', 'l2Book']);

export type TxFlowHistoricalIdentity = { id: string; symbol: string; base: string; ambiguous?: boolean };
