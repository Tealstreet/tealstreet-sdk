import type { ExchangeName } from '../types';

/**
 * The unit of `OrderFillEvent.amount`, for venues whose fill event is VERIFIED
 * to report one execution: `native` = the unit orders are placed in, `base` =
 * base-asset quantity. A venue absent here is unverified — it emits a running
 * total, a terminal total, two events per execution, or a unit nobody checked —
 * and must not drive anything that adds fills up (provisional positions).
 */
const FILL_EVENT_UNITS: Partial<Record<ExchangeName, 'native' | 'base'>> = {
  bybit: 'native',
  bybitdemo: 'native',
  bybitinverse: 'native',
  bybitspot: 'native',
  hyrotrader: 'native',
  bitrue: 'native',
  binance: 'native',
  binancepm: 'native',
  binancespot: 'native',
  hyperliquid: 'native',
  hyperliquidspot: 'native',
  grvt: 'native',
  asterdex: 'native',
  weex: 'native',
  backpack: 'native',
  okx: 'base',
  okxeu: 'base',
  okxdemo: 'base',
  okxeudemo: 'base',
  kucoin: 'base',
  blofin: 'base',
  mexc: 'base',
};

export const fillEventUnits = (exchangeName: string): 'native' | 'base' | undefined =>
  FILL_EVENT_UNITS[exchangeName as ExchangeName];

export const FILL_EVENT_UNIT_VENUES = Object.keys(FILL_EVENT_UNITS);
