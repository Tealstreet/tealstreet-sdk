import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import type { BaseExchange } from '../../exchanges/base';
import {
  isNodeOrBunRuntime,
  safeCexAxiosCreateNoProxy,
  setInnerAdapterForRemoteSigning,
} from '../../safeCexAxiosCreate';
import type { ExchangeOptions, RemoteSigner } from '../../types';

const exchangeApiFiles = (directory: string): string[] =>
  readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) return exchangeApiFiles(path);
    return entry.name.endsWith('.api.ts') ? [path] : [];
  });

const exchangeRuntimeFiles = (directory: string): string[] =>
  readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) return exchangeRuntimeFiles(path);
    return entry.name.endsWith('.ts') ? [path] : [];
  });

describe('transport remote-signing adapter invariant', () => {
  it('does not classify browser workers as Node merely because window is absent', () => {
    expect(isNodeOrBunRuntime()).toBe(true);

    vi.stubGlobal('process', undefined);
    const browserWorkerResult = isNodeOrBunRuntime();
    vi.unstubAllGlobals();

    expect(browserWorkerResult).toBe(false);
  });

  it('keeps remote signing outside the exchange-selected inner adapter', async () => {
    const innerAdapter = vi.fn(async (config: any) => ({
      config,
      data: { ok: true },
      headers: {},
      status: 200,
      statusText: 'OK',
    }));
    const remoteSigner: RemoteSigner = {
      accountName: 'hosted-account',
      isAvailable: () => true,
      onAvailabilityChange: () => () => {},
      sign: vi.fn(async (request) => ({
        id: request.id,
        config: {
          data: request.config?.data,
          // A venue signature header: `Authorization` is our proxy credential and is filtered.
          headers: { 'X-Signature': 'signed' },
          params: request.config?.params,
          url: request.config?.url,
        },
      })),
    };
    const exchange = {
      grafanaEndpointsEvery: [],
      grafanaEndpointsPeriodic: [],
      name: 'binance',
    } as unknown as BaseExchange;
    const options = { remoteSigner } as ExchangeOptions;
    const api = safeCexAxiosCreateNoProxy({ baseURL: 'https://example.com' }, options, exchange);

    setInnerAdapterForRemoteSigning(api, innerAdapter);
    const response = await api.get('/private');

    expect(remoteSigner.sign).toHaveBeenCalledOnce();
    expect(innerAdapter).toHaveBeenCalledOnce();
    expect((innerAdapter.mock.calls[0][0] as any).headers.get('X-Signature')).toBe('signed');
    expect(response.data).toEqual({ ok: true });
  });

  // Walks the source tree; under CI load this outruns vitest's 5s default.
  it('forbids exchange factories from replacing the outer adapter directly', () => {
    const exchangesDirectory = resolve(process.cwd(), 'src/exchanges');
    const offenders = exchangeApiFiles(exchangesDirectory)
      .filter((file) => /^(?!\s*\/\/)\s*.*\.defaults\.adapter\s*=/m.test(readFileSync(file, 'utf8')))
      .map((file) => file.slice(exchangesDirectory.length + 1));

    expect(offenders).toEqual([]);
  }, 30_000);

  // Walks the source tree; under CI load this outruns vitest's 5s default.
  it('forbids hand-rolled exchange-tree sockets to TradeTunnel hosts', () => {
    const exchangesDirectory = resolve(process.cwd(), 'src/exchanges');
    const offenders = exchangeRuntimeFiles(exchangesDirectory)
      .filter((file) => /new WebSocket\(\s*['"]wss:\/\/fsjnpy143pbz\./.test(readFileSync(file, 'utf8')))
      .map((file) => file.slice(exchangesDirectory.length + 1));

    expect(offenders).toEqual([]);
  }, 30_000);
});
