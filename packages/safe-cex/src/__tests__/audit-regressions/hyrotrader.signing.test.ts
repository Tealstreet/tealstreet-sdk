import type { InternalAxiosRequestConfig } from 'axios';

import { createHmac } from 'node:crypto';

import { afterEach, describe, expect, it } from 'vitest';

import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { HyrotraderExchange } from '../reference/bybit/hyrotrader.exchange';
import { DefaultStore } from '../../store/store.base';

class TestHyroExchange extends HyrotraderExchange {
  getBybitRestBaseUrl() {
    return 'https://simulator.test';
  }
}
const exchanges: TestHyroExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

describe('HyroTrader transmitted request signing', () => {
  it.each(['get', 'post'] as const)('signs the exact transmitted %s query or JSON body', async (method) => {
    const exchange = new TestHyroExchange(
      'hyrotrader',
      { key: 'fixture-key', secret: 'fixture-secret' },
      new DefaultStore(),
    );
    exchanges.push(exchange);
    let captured!: InternalAxiosRequestConfig;
    exchange.xhr.defaults.adapter = async (config) => {
      captured = config;
      return {
        config,
        status: 200,
        statusText: 'OK',
        headers: {},
        data: { retCode: 0, retMsg: 'OK', result: { list: [] } },
      };
    };
    if (method === 'get') {
      await exchange.xhr.get(ENDPOINTS.BALANCE, { params: { accountType: 'UNIFIED', cursor: 'a+b /?' } });
    } else {
      await exchange.xhr.post(ENDPOINTS.CREATE_ORDER, {
        category: 'linear',
        symbol: 'BTCUSDT',
        qty: '0.001',
        orderLinkId: 'fixed-client-id',
      });
    }
    expect(captured.baseURL).toBe('https://simulator.test');
    const content = method === 'get' ? exchange.xhr.getUri(captured).split('?')[1] : captured.data;
    if (method === 'post') expect(typeof content).toBe('string');
    const headers = captured.headers;
    if (method === 'post') expect(headers['Content-Type']).toBe('application/json');
    const signature = createHmac('sha256', 'fixture-secret')
      .update(`${headers['X-BAPI-TIMESTAMP']}fixture-key${headers['X-BAPI-RECV-WINDOW']}${content}`)
      .digest('hex');
    expect(headers['X-BAPI-SIGN']).toBe(signature);
    expect(headers['X-BAPI-API-KEY']).toBe('fixture-key');
    expect(headers['x-site-id']).toBeUndefined();
  });
});
