import type { HttpPacket, HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it, vi } from 'vitest';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { bybitPublicRateLimitReport } from './bybitPublicRateLimitTelemetry';
import { directTransport } from './direct';
import { createEngineTransport } from './engine';

vi.mock('../index', () => ({ createExchange: vi.fn() }));

const packet: HttpPacket = {
  request: 7,
  method: 'GET',
  url: 'https://api.bybit.com/v5/market/kline?category=linear&symbol=BTCUSDT&interval=1',
  headers: { 'X-Referer': 'fixture' },
};
const text = '{"retCode":10006,"retMsg":"private vendor text","result":{}}';
const headers = [
  'X-Tealstreet-Egress: proxy-02',
  'x-tealstreet-egress-target: tokyo-02',
  'X-Bapi-Limit: 120',
  'X-Bapi-Limit-Status: 0',
  'X-Bapi-Limit-Reset-Timestamp: 1800000000000',
  'Retry-After: 10',
  'Authorization: secret-response-header',
].join('\n');
const expected = {
  egress: 'proxy-02',
  target: 'tokyo-02',
  limit: '120',
  remaining: '0',
  resetTimestamp: '1800000000000',
  retryAfter: '10',
};
const response = (streamed: boolean): HttpResponse => ({
  status: 200,
  headers,
  body: streamed
    ? (async function* () {
        yield new TextEncoder().encode(text.slice(0, 10));
        yield new TextEncoder().encode(text.slice(10));
      })()
    : new TextEncoder().encode(text),
});
const raw = (reply: HttpResponse): Transport => ({
  now: () => 0,
  setTimeout,
  clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
  connect: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
  request: vi.fn(async () => reply),
  prepareHttp: () => ({ bind: () => ({ ready: () => true, dispatch: async () => reply }) }),
});
const read = async (reply: HttpResponse) => {
  if (reply.body instanceof Uint8Array) return new TextDecoder().decode(reply.body);
  let body = '';
  for await (const chunk of reply.body) body += new TextDecoder().decode(chunk);
  return body;
};

describe('Bybit public diagnostics on managed host responses', () => {
  it.each([undefined, 'direct', 'publicCors'] as const)(
    'reports once on route %s, preserving streamed headers and body',
    async (route) => {
      const report = vi.fn();
      const transport = createEngineTransport(
        'bybit',
        {
          key: '',
          secret: '',
          transport: { ...directTransport, publicCorsProxy: () => 'https://public-proxy.test' },
        },
        raw(response(true)),
        undefined,
        undefined,
        undefined,
        report,
      );
      try {
        const request = { ...packet, route };
        const prepared = await transport.prepareHttp!({ ...request, intent: 'public' });
        const reply = await prepared.bind(request).dispatch();
        expect(reply.headers).toBe(headers);
        expect(await read(reply)).toBe(text);
        expect(report).toHaveBeenCalledExactlyOnceWith(expected);
        const serialized = JSON.stringify(bybitPublicRateLimitReport('bybit', report.mock.calls[0][0]));
        for (const secret of ['private vendor text', 'secret-response-header', 'BTCUSDT', 'https:', 'retMsg'])
          expect(serialized).not.toContain(secret);
      } finally {
        transport.dispose?.();
      }
    },
  );

  it('uses the unsigned request allowlist even on primary egress, and excludes signed reads', async () => {
    const report = vi.fn();
    const transport = createEngineTransport(
      'bybit',
      { key: '', secret: '', transport: directTransport },
      raw(response(false)),
      undefined,
      undefined,
      undefined,
      report,
    );
    try {
      const request = { ...packet, route: 'primary' as const };
      await (await transport.prepareHttp!({ ...request, intent: 'primary' })).bind(request).dispatch();
      expect(report).toHaveBeenCalledExactlyOnceWith(expected);
      const signed = { ...request, request: 8, headers: { 'X-BAPI-API-KEY': 'secret', 'X-BAPI-SIGN': 'signature' } };
      await (await transport.prepareHttp!({ ...signed, intent: 'primary' })).bind(signed).dispatch();
      expect(report).toHaveBeenCalledTimes(1);
    } finally {
      transport.dispose?.();
    }
  });

  it('feeds the same managed 10006 response to EngineHost and preserves its typed command refusal', async () => {
    const report = vi.fn();
    const transport = createEngineTransport(
      'bybit',
      { key: '', secret: '', transport: directTransport },
      raw(response(true)),
      undefined,
      undefined,
      undefined,
      report,
    );
    const wirePacket = { ...packet, headers: Object.entries(packet.headers) };
    let command = 0;
    let delivered = '';
    const detail = {
      message: 'Bybit klines: Too many visits',
      kind: 'rateLimited',
      venueCode: '10006',
      retryAfterMs: 10000,
    };
    const host = new EngineHost(
      {
        command(json: string) {
          command = JSON.parse(json).command;
          return JSON.stringify([
            { op: 'managedHttp', phase: 'prepare', preparation: 'p1', intent: 'public', packet: wirePacket },
          ]);
        },
        httpPreparationPending: () => true,
        httpPrepared: () =>
          JSON.stringify([{ op: 'managedHttp', phase: 'bind', preparation: 'p1', permit: 'r1', packet: wirePacket }]),
        httpClaim: () => '{"status":"released"}',
        prepareDispatch: () => '',
        httpStart: (_id: number, status: number, received: string) => {
          expect(status).toBe(200);
          expect(received).toBe(headers);
          return '';
        },
        httpChunk: (_id: number, chunk: Uint8Array) => {
          delivered += new TextDecoder().decode(chunk);
          return '';
        },
        httpEnd: () => JSON.stringify([{ op: 'done', command, error: detail.message, detail }]),
      } as never,
      transport,
    );
    try {
      await expect(host.command({ op: 'fetchCandles', symbol: 'BTCUSDT', interval: '1m' })).rejects.toMatchObject(
        detail,
      );
      expect(delivered).toBe(text);
      expect(report).toHaveBeenCalledExactlyOnceWith(expected);
    } finally {
      host.dispose();
    }
  });
});
