import type { ExchangeName } from '../../types';

import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { PRIVATE_PROXY_ASIA, PRIVATE_PROXY_GRVT_TINY, PRIVATE_PROXY_JP } from '../../utils/transportResolution';
import { resolveExchangeRouting } from '../resolveExchangeRouting';

// Frozen copy of the original getPrivateProxy (deleted when REST consumers were cut
// over to resolveExchangeRouting), including its requiresCors=false -> '' guard. Kept
// here verbatim as the immutable oracle the resolver's REST pool selection is proven
// against.
const legacyGetPrivateProxy = (url: string, requiresCors: boolean, forceJa: boolean): string => {
  if (!requiresCors) return '';
  return url.includes('grvt')
    ? PRIVATE_PROXY_GRVT_TINY
    : /binance|okx|okex|bitget/.test(url) || forceJa
      ? PRIVATE_PROXY_JP
      : PRIVATE_PROXY_ASIA;
};

// Epic 1 net: prove the declarative routing table + resolver reproduce the CURRENT
// observed packet path exactly, BEFORE later epics cut consumers over to it. WS
// host equivalence is the crux (those hosts had no test before). REST/HOW are also
// covered by the existing transport-resolution.characterization.test.ts; the
// golden-master matrix here freezes the resolver's full output for regression.

const proxy = { tealstreetProxy: true };
const offProxy = { tealstreetProxy: false };
const wsRoute = { tealstreetWsRoute: true };

describe('routing table / resolver (Epic 1 net)', () => {
  it('WS hosts reproduce the current hardcoded literals', () => {
    // alwaysProxy (arkham public+private, grvt private) — no option gates it.
    expect(resolveExchangeRouting('arkham', offProxy).wsPublic).toEqual({
      tunnelled: true,
      host: 'wss://fsjnpy143pbz.jp.tealstreet.io/arkm',
    });
    expect(resolveExchangeRouting('arkham', offProxy).wsPrivate.host).toBe('wss://fsjnpy143pbz.jp.tealstreet.io/arkm');
    expect(resolveExchangeRouting('grvt', proxy).wsPrivate.host).toBe('wss://fsjnpy143pbz.jp.tealstreet.io/grvt');

    // proxyConditional: on -> tunnel host, off -> direct.
    expect(resolveExchangeRouting('okx', proxy).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/okx/private',
    );
    expect(resolveExchangeRouting('okx', proxy).wsBusiness.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/okx/business',
    );
    expect(resolveExchangeRouting('okx', offProxy).wsPrivate).toEqual({ tunnelled: false });
    expect(resolveExchangeRouting('blofin', proxy).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/blofin/private',
    );
    expect(resolveExchangeRouting('coinbase', proxy).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/coinbase/private',
    );
    expect(resolveExchangeRouting('apexomni', proxy).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/apex/private',
    );

    // regionConditional (bitmex): JA -> jp, else -> asia, off -> direct.
    expect(resolveExchangeRouting('bitmex', { tealstreetProxy: true, proxyRegion: 'JA' }).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.jp.tealstreet.io/bitmex/private',
    );
    expect(resolveExchangeRouting('bitmex', { tealstreetProxy: true, proxyRegion: 'SG' }).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/bitmex/private',
    );
    expect(resolveExchangeRouting('bitmex', offProxy).wsPrivate).toEqual({ tunnelled: false });

    // fallbackOnly (bitget): direct primary, distinct fallback host per channel.
    expect(resolveExchangeRouting('bitget', offProxy).wsPrivate).toEqual({
      tunnelled: false,
      fallback: 'wss://fsjnpy143pbz.asia.tealstreet.io/bitget/v2/private',
    });
    expect(resolveExchangeRouting('bitget', offProxy).wsPublic).toEqual({
      tunnelled: false,
      fallback: 'wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public',
    });

    // wsRouteCandidates (lighter): gated by tealstreetWsRoute, NOT tealstreetProxy.
    expect(resolveExchangeRouting('lighter', wsRoute).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.jp.tealstreet.io/lighter/stream',
    );
    expect(resolveExchangeRouting('lighter', wsRoute).wsPublic.host).toBe(
      'wss://fsjnpy143pbz.jp.tealstreet.io/lighter/stream?readonly=true',
    );
    expect(resolveExchangeRouting('lighter', proxy).wsPrivate).toEqual({ tunnelled: false });
  });

  it('REST pool host matches getPrivateProxy across every branch', () => {
    // grvt -> tiny pool (wins over forceJa).
    expect(resolveExchangeRouting('grvt', proxy).restProxyHost).toBe(PRIVATE_PROXY_GRVT_TINY);
    expect(legacyGetPrivateProxy('https://grvt.io/api', true, false)).toBe(PRIVATE_PROXY_GRVT_TINY);

    // binance/okx/bitget -> jp pool.
    for (const ex of ['binance', 'okx', 'bitget'] as ExchangeName[]) {
      expect(resolveExchangeRouting(ex, proxy).restProxyHost).toBe(PRIVATE_PROXY_JP);
    }
    expect(legacyGetPrivateProxy('https://fapi.binance.com', true, false)).toBe(PRIVATE_PROXY_JP);

    // others -> asia; proxyRegion JA forces jp.
    expect(resolveExchangeRouting('bybit', proxy).restProxyHost).toBe(PRIVATE_PROXY_ASIA);
    expect(resolveExchangeRouting('bybit', { tealstreetProxy: true, proxyRegion: 'JA' }).restProxyHost).toBe(
      PRIVATE_PROXY_JP,
    );
    expect(legacyGetPrivateProxy('https://api.bybit.com', true, false)).toBe(PRIVATE_PROXY_ASIA);
    expect(legacyGetPrivateProxy('https://api.bybit.com', true, true)).toBe(PRIVATE_PROXY_JP);

    // no tealstreetProxy -> direct REST.
    expect(resolveExchangeRouting('bybit', offProxy).restProxyHost).toBe('');
  });

  it('variant exchanges inherit their parent class WS routing', () => {
    // okxeu extends OKXExchange {} -> same okx ws classes.
    expect(resolveExchangeRouting('okxeu', proxy).wsPrivate).toEqual(resolveExchangeRouting('okx', proxy).wsPrivate);
    expect(resolveExchangeRouting('okxeu', proxy).wsBusiness.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/okx/business',
    );
    // bitget{inverse,usdc} extend BitgetExchange -> same fallback hosts.
    for (const v of ['bitgetinverse', 'bitgetusdc'] as ExchangeName[]) {
      expect(resolveExchangeRouting(v, offProxy).wsPrivate).toEqual(
        resolveExchangeRouting('bitget', offProxy).wsPrivate,
      );
      expect(resolveExchangeRouting(v, offProxy).wsPublic.fallback).toBe(
        'wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public',
      );
    }
    // coinbasespot extends CoinbaseExchange -> same private proxy host.
    expect(resolveExchangeRouting('coinbasespot', proxy).wsPrivate.host).toBe(
      'wss://fsjnpy143pbz.asia.tealstreet.io/coinbase/private',
    );
  });

  it('howEgress is region-conditional for the region-pinned exchanges', () => {
    // bitmex (always) and bybit (when tunnelling) pin the HOW adapter to proxyRegion:
    // JA -> jp, otherwise the bare-adapter fallback -> sg.
    for (const ex of ['bitmex', 'bitmextestnet', 'bybit', 'bybitdemo', 'bybitinverse', 'bybitspot'] as ExchangeName[]) {
      expect(resolveExchangeRouting(ex, { proxyRegion: 'JA' }).howEgress).toBe('jp');
      expect(resolveExchangeRouting(ex, { proxyRegion: 'SG' }).howEgress).toBe('sg');
      expect(resolveExchangeRouting(ex, {}).howEgress).toBe('sg');
    }
    // a non-pinned, non-TOKYO exchange stays sg regardless of region.
    expect(resolveExchangeRouting('phemex', { proxyRegion: 'JA' }).howEgress).toBe('sg');
    // a TOKYO-list exchange stays jp regardless of region.
    expect(resolveExchangeRouting('binance', { proxyRegion: 'SG' }).howEgress).toBe('jp');
  });

  it('resolved packet-path matrix (golden master)', () => {
    const BOOL = [false, true];
    const REGIONS: Array<string | undefined> = [undefined, 'JA', 'SG'];
    const matrix: Record<string, Record<string, unknown>> = {};
    for (const exchange of Object.keys(exchangeMetadata) as ExchangeName[]) {
      const per: Record<string, unknown> = {};
      for (const tealstreetProxy of BOOL) {
        for (const tealstreetWsRoute of BOOL) {
          for (const proxyRegion of REGIONS) {
            const key = `proxy=${Number(tealstreetProxy)} wsRoute=${Number(tealstreetWsRoute)} region=${
              proxyRegion ?? 'NONE'
            }`;
            per[key] = resolveExchangeRouting(exchange, {
              tealstreetProxy,
              tealstreetWsRoute,
              ...(proxyRegion ? { proxyRegion } : {}),
            });
          }
        }
      }
      matrix[exchange] = per;
    }
    expect(matrix).toMatchSnapshot();
  });
});
