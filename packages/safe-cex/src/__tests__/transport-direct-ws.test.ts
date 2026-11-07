import { afterEach, describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from './reference/bitget/bitget.exchange';
import { GrvtExchange } from './reference/grvt/grvt.exchange';
import { LighterExchange } from './reference/lighter/lighter.exchange';
import { DefaultStore } from '../store/store.base';
import { directTransport } from '../transport/direct';

// The routing tables only ever ran under the Tealstreet transport, so three ws
// classes dereferenced hosts that the direct transport never resolves.
const live: { dispose: () => void }[] = [];
afterEach(() => live.splice(0).forEach((exchange) => exchange.dispose()));

const track = <T extends { dispose: () => void }>(exchange: T): T => {
  live.push(exchange);
  return exchange;
};

describe('direct transport websockets', () => {
  it('bitget never switches to a fallback host the transport does not declare', () => {
    const exchange = track(
      new BitgetExchange(
        'bitget',
        { key: '', secret: '', publicDataOnly: true, transport: directTransport },
        new DefaultStore(),
      ),
    );
    const socket = exchange.publicWebsocket as any;
    const createWebsocket = vi.spyOn(socket, 'createWebsocket').mockReturnValue({ addEventListener: vi.fn() });
    socket.useFallbackUrl = true;

    socket.connectAndSubscribe();

    expect(createWebsocket).toHaveBeenCalledTimes(1);
    expect(createWebsocket.mock.calls[0][0]).toEqual(expect.stringMatching(/^wss:\/\/.*bitget/));
  });

  it('grvt refuses to open its private stream without a proxy host, saying why', () => {
    const exchange = track(
      new GrvtExchange(
        'grvt',
        {
          key: 'k',
          secret: '0x1',
          grvtSignerAddress: '0x0000000000000000000000000000000000000001',
          grvtTradingAccountId: 'acct',
          transport: directTransport,
        },
        new DefaultStore(),
      ),
    );
    const socket = (exchange as any).privateWebsocket;
    const createWebsocket = vi.spyOn(socket, 'createWebsocket');
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});

    socket.connectAndSubscribe();
    socket.connectAndSubscribe();

    expect(createWebsocket).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledTimes(1);
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('needs a proxy transport'));
  });

  it('lighter treats a ws route the transport cannot resolve as no route', () => {
    const exchange = track(
      new LighterExchange(
        'lighter',
        {
          key: 'k',
          secret: 's',
          apiKeyIndex: '0',
          publicDataOnly: true,
          tealstreetWsRoute: true,
          transport: directTransport,
        },
        new DefaultStore(),
      ),
    );

    expect((exchange as any).publicWebsocket.isWsProxyRouteEnabled()).toBe(false);
  });
});

describe('direct transport ws route requests', () => {
  it('warns once per exchange that a requested ws route is going direct', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    directTransport.wsRouting('bitunix', { tealstreetWsRoute: true });
    directTransport.wsRouting('bitunix', { tealstreetWsRoute: true });
    expect(warn.mock.calls.filter(([msg]) => String(msg).includes('[bitunix] tealstreetWsRoute'))).toHaveLength(1);
  });
});
