import type { JSONRPCMessage } from '@modelcontextprotocol/sdk/types.js';

import { afterEach, expect, it, vi } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';

const factory = vi.hoisted(() => ({ create: vi.fn() }));
vi.mock('@tealstreet/safe-cex/src/index', () => ({ createExchange: factory.create }));

import { createServer } from './server';

let instance: Awaited<ReturnType<typeof createServer>> | undefined;
let client: Client | undefined;
afterEach(async () => {
  await client?.close();
  await instance?.server.close();
  instance?.exchange.dispose();
  client = undefined;
  instance = undefined;
});

it.each(['get_fills', 'get_income'])(
  '%s serializes a request-owned partial result without changing an unflagged concurrent response',
  async (name) => {
    const read = async ({ symbol }: { symbol?: string }) => {
      if (symbol === 'failed') throw new Error('terminal history failure');
      const rows = symbol === 'empty' ? [] : [{ id: symbol, timestamp: 1 }];
      return symbol === 'full' ? rows : Object.defineProperty(rows, 'incomplete', { value: true });
    };
    factory.create.mockResolvedValue({
      name: 'bybit',
      options: {},
      capabilities: { fillHistory: true, incomeHistory: true },
      start: vi.fn(),
      loadCapabilities: vi.fn().mockResolvedValue(undefined),
      disablePublicWebsockets: vi.fn(),
      dispose: vi.fn(),
      fetchHistoricalFills: read,
      fetchIncomeHistory: read,
    });
    instance = await createServer(
      { exchange: 'bybit', options: { key: '', secret: '', publicDataOnly: true } },
      'test',
    );
    const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
    const responses: JSONRPCMessage[] = [];
    const send = serverTransport.send.bind(serverTransport);
    serverTransport.send = async (message) => {
      const wire = JSON.parse(JSON.stringify(message)) as JSONRPCMessage;
      responses.push(wire);
      await send(wire);
    };
    await instance.server.connect(serverTransport);
    client = new Client({ name: 'history-completeness', version: 'test' });
    await client.connect(clientTransport);
    const [partial, full, empty] = await Promise.all(
      ['partial', 'full', 'empty'].map(async (symbol) => {
        const result = await client!.callTool({ name, arguments: { symbol } });
        expect(result.isError).not.toBe(true);
        return JSON.parse((result.content as Array<{ text: string }>)[0].text);
      }),
    );
    expect(partial).toMatchObject({ rows: [{ id: 'partial' }], completeness: { incomplete: true } });
    expect(empty).toMatchObject({ rows: [], completeness: { incomplete: true } });
    expect(full).toEqual([expect.objectContaining({ id: 'full' })]);
    for (const value of [partial, empty]) {
      const packet = responses.find((message) => {
        if (!('result' in message)) return false;
        const content = (message.result as { content?: Array<{ text: string }> }).content;
        return content && JSON.parse(content[0].text).requestId === value.requestId;
      });
      expect(packet).toHaveProperty('id', value.requestId);
      expect(value.requestId).toBeDefined();
    }
    expect(partial.requestId).not.toBe(empty.requestId);
    const failed = await client!.callTool({ name, arguments: { symbol: 'failed' } });
    expect(failed.isError).toBe(true);
    expect(JSON.stringify(failed.content)).toContain('terminal history failure');
    expect(JSON.stringify(failed)).not.toContain('completeness');
  },
);
