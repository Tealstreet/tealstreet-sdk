import { beforeEach, describe, expect, it, vi } from 'vitest';

const constructedUrls: string[] = [];
const websocketInstances: Array<{ reconnect: () => Promise<void> }> = [];
const connectionAttempts: Array<Promise<void>> = [];

const flushConnectionAttempts = async () => {
  await Promise.all(connectionAttempts.splice(0));
};

vi.mock('@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket', () => {
  class MockReconnectingWebSocket {
    onerror: unknown;
    onopen: unknown;
    onmessage: unknown;
    onclose: unknown;

    private urlProvider: string | (() => string | Promise<string>);

    constructor(url: string | (() => string | Promise<string>)) {
      this.urlProvider = url;
      websocketInstances.push(this);
      connectionAttempts.push(this.reconnect());
    }

    reconnect = async () => {
      const url = typeof this.urlProvider === 'function' ? await this.urlProvider() : this.urlProvider;
      constructedUrls.push(url);
    };

    addEventListener = vi.fn();
  }

  return { ReconnectingWebSocket: MockReconnectingWebSocket };
});

import { configureHOW } from './config';
import { HOWWebSocket } from './HOWWebSocket';

describe('HOWWebSocket URL admission ticket', () => {
  beforeEach(() => {
    constructedUrls.length = 0;
    websocketInstances.length = 0;
    connectionAttempts.length = 0;
    configureHOW({
      getProxyToken: async () => 'proxy-token',
      getTunnelUrlTicket: undefined,
      isTokenExpired: () => false,
    });
  });

  it('keeps existing tunnel URLs unchanged when no URL ticket is configured', async () => {
    const ws = new HOWWebSocket('wss://example.test/tunnel');

    await ws.connectAndSubscribe();
    await flushConnectionAttempts();

    expect(constructedUrls).toHaveLength(1);
    const url = new URL(constructedUrls[0]);
    expect(url.searchParams.get('v')).toBe('2');
    expect(url.searchParams.get('t')).toBeTruthy();
    expect(url.searchParams.has('tt')).toBe(false);
  });

  it('appends tt when a URL ticket provider returns a token', async () => {
    configureHOW({
      getTunnelUrlTicket: async () => 'ticket.header.signature',
    });
    const ws = new HOWWebSocket('wss://example.test/tunnel');

    await ws.connectAndSubscribe();
    await flushConnectionAttempts();

    expect(constructedUrls).toHaveLength(1);
    expect(new URL(constructedUrls[0]).searchParams.get('tt')).toBe('ticket.header.signature');
  });

  it('fails open if URL ticket generation fails', async () => {
    configureHOW({
      getTunnelUrlTicket: async () => {
        throw new Error('issuer unavailable');
      },
    });
    const ws = new HOWWebSocket('wss://example.test/tunnel');

    await ws.connectAndSubscribe();
    await flushConnectionAttempts();

    expect(constructedUrls).toHaveLength(1);
    expect(new URL(constructedUrls[0]).searchParams.has('tt')).toBe(false);
  });

  it('refreshes tt on reconnect', async () => {
    let ticketVersion = 0;
    configureHOW({
      getTunnelUrlTicket: async () => `ticket.v${++ticketVersion}.signature`,
    });
    const ws = new HOWWebSocket('wss://example.test/tunnel');

    await ws.connectAndSubscribe();
    await flushConnectionAttempts();
    await websocketInstances[0].reconnect();

    expect(constructedUrls).toHaveLength(2);
    expect(new URL(constructedUrls[0]).searchParams.get('tt')).toBe('ticket.v1.signature');
    expect(new URL(constructedUrls[1]).searchParams.get('tt')).toBe('ticket.v2.signature');
  });
});
