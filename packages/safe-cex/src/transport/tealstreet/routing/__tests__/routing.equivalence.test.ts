import type { ExchangeName } from '../../../../types';

import { describe, expect, it } from 'vitest';

import { isTokyoEgress } from '@tealstreet/http-over-websocket/src/egressRegion';
import { exchangeMetadata } from '../../../../exchangeMetadata';
import { PRIVATE_PROXY_ASIA, PRIVATE_PROXY_GRVT_TINY, PRIVATE_PROXY_JP } from '../../transportResolution';
import { resolveExchangeRouting } from '../resolveExchangeRouting';

// Frozen oracle: each exchange's current REST API baseURL — the exact string the
// HOW adapter inspects at request time (via isTokyoEgress(config.baseURL)) to pick
// jp vs sg egress. Kept here independently of the api modules so the single-source
// lock test below can prove the table's NAME-based egress agrees with the adapter's
// runtime BASEURL-based egress. If an exchange's baseURL changes such that its
// Tokyo match flips, this map (and the reconciliation it forces) is the tripwire.
const REST_BASE_URLS: Record<ExchangeName, string> = {
  apexomni: 'https://omni.apex.exchange/api',
  asterdex: 'https://fapi.asterdex.com',
  backpack: 'https://api.backpack.exchange',
  binance: 'https://fapi.binance.com',
  binanceinverse: 'https://dapi.binance.com',
  binanceoptions: 'https://eapi.binance.com',
  binancepm: 'https://papi.binance.com',
  binancespot: 'https://api.binance.com',
  bingx: 'https://open-api.bingx.com',
  bitget: 'https://api.bitget.com',
  bitgetinverse: 'https://api.bitget.com',
  bitgetusdc: 'https://api.bitget.com',
  bitrue: 'https://fapi.bitrue.com',
  bitunix: 'https://fapi.bitunix.com',
  blofin: 'https://openapi.blofin.com',
  breakoutprop: 'https://breakout.prosp.devexperts.com/dxsca-web',
  bybit: 'https://api.bybit.com',
  bybitdemo: 'https://api-demo.bybit.com',
  hyrotrader: 'https://hyrotrader-simulator',
  bybitinverse: 'https://api.bybit.com',
  bybitspot: 'https://api.bybit.com',
  coinbase: 'https://api.coinbase.com',
  coinbasespot: 'https://api.coinbase.com',
  cryptocom: 'https://api.crypto.com',
  // grvt issues REST across three hosts (market-data/trades/edge .grvt.io); all
  // match /grvt/i, so any one represents the egress decision faithfully.
  grvt: 'https://trades.grvt.io',
  hyperliquid: 'https://api.hyperliquid.xyz',
  hyperliquidspot: 'https://api.hyperliquid.xyz',
  kodiakfi: 'https://api-evm.orderly.org',
  kraken: 'https://futures.kraken.com/derivatives/api',
  krakenspot: 'https://api.kraken.com',
  kucoin: 'https://api-futures.kucoin.com',
  kucoinspot: 'https://api.kucoin.com',
  lighter: 'https://mainnet.zklighter.elliot.ai',
  'lighter-robinhood': 'https://api.rh.lighter.xyz',
  mexc: 'https://api.mexc.com',
  mode: 'https://api-evm.orderly.org',
  okx: 'https://www.okx.com',
  okxdemo: 'https://openapi.okx.com',
  okxeu: 'https://eea.okx.com',
  okxeudemo: 'https://eea.okx.com',
  paradex: 'https://api.prod.paradex.trade/v1',
  phemex: 'https://api.phemex.com',
  polymarket: 'https://clob.polymarket.com',
  salsadex: 'https://api-evm.orderly.org',
  'tealstreet-dex': 'https://api-evm.orderly.org',
  txflow: 'https://rpc.txflow.com',
  weex: 'https://api-contract.weex.com',
  what: 'https://api-evm.orderly.org',
  woo: 'https://api.woox.io',
  woofi: 'https://api-evm.orderly.org',
};

// Exchanges whose HOW adapter is region-PINNED from the account's proxyRegion
// (bybit family when tunnelling) rather than auto-detected from the
// baseURL. Their egress is covered by the region-conditional test above, so they're
// excluded from the baseURL-vs-name agreement assertion.
const REGION_PINNED_HOW: ExchangeName[] = ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot'];

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
const bybitBrazil = { bybitSiteId: 'BRA_BTL' as const };

// AWS_EXIT.md step 3e moved every host below from fsjnpy143pbz.{jp,asia} (AWS) to the
// Vultr host: the tunnel, the relay, the bybitbra/grvt/lighter paths, and the
// okx/okxeu/bitget/blofin private sockets, which became relay urls in place of the
// AWS Python ws-proxy paths, as did coinbase's once the relay took root paths (5a); apex went direct. The hosted
// exchange server dials these same hosts since its pin was removed (AWS_EXIT.md step 4).
const TYO = 'wss://fsjnpy143pbz-tyo.tealstreet.io';
const relayOf = (upstream: string) => `${TYO}/tunnel?ts-relay=${encodeURIComponent(upstream)}`;

describe('routing table / resolver (Epic 1 net)', () => {
  it('WS hosts reproduce the current hardcoded literals', () => {
    // alwaysProxy (grvt private) — no option gates it.
    expect(resolveExchangeRouting('grvt', proxy).wsPrivate.host).toBe(`${TYO}/grvt`);

    // proxyConditional: on -> tunnel host, off -> direct.
    expect(resolveExchangeRouting('okx', proxy).wsPrivate.host).toBe(relayOf('wss://ws.okx.com/ws/v5/private'));
    expect(resolveExchangeRouting('okx', proxy).wsBusiness.host).toBe(relayOf('wss://ws.okx.com/ws/v5/business'));
    expect(resolveExchangeRouting('okx', offProxy).wsPrivate).toMatchObject({
      tunnelled: false,
      fallback: 'wss://okx-proxy-ja.tealstreet.io/ws/v5/private',
    });
    expect(resolveExchangeRouting('blofin', proxy).wsPrivate.host).toBe(relayOf('wss://openapi.blofin.com/ws/private'));
    // Was the AWS ws-proxy `…asia…/coinbase/private`; the relay dials its host root (5a).
    expect(resolveExchangeRouting('coinbase', proxy).wsPrivate.host).toBe(
      relayOf('wss://advanced-trade-ws-user.coinbase.com/'),
    );
    // Was the AWS ws-proxy `…asia…/apex/private`; apex connects directly since 3e (5c).
    expect(resolveExchangeRouting('apexomni', proxy).wsPrivate).toEqual({ tunnelled: false });

    // bitget private moved from fallbackOnly to proxyConditional + a relay that
    // also rides the tunnelled route (40018 IP-bound logins, 2026-10-21); public
    // is still fallbackOnly. UTA accounts get the v3 equivalents.
    // Literal on purpose: sdk-rs's routing.mjs loads this file with a fixed import map.
    const bitgetRelay = (version: 'v2' | 'v3') =>
      `wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fws.bitget.com%2F${version}%2Fws%2Fprivate`;
    // host and fallback were the AWS ws-proxy `…asia…/bitget/{v2,v3}/private`; since
    // 3e they are the relay itself, so the relay candidate repeats it.
    expect(resolveExchangeRouting('bitget', offProxy).wsPrivate).toEqual({
      tunnelled: false,
      fallback: bitgetRelay('v2'),
      initialSilentConnects: 1,
      ignoredMessages: ['pong'],
      relay: bitgetRelay('v2'),
    });
    // The Rust engine host routes its UTA (v3) socket through privateUta by URL.
    expect(resolveExchangeRouting('bitget', proxy).wsPrivate).toEqual({
      tunnelled: true,
      host: bitgetRelay('v2'),
      relay: bitgetRelay('v2'),
    });
    expect(resolveExchangeRouting('bitget', proxy).wsPrivateUta).toEqual({
      tunnelled: true,
      host: bitgetRelay('v3'),
      relay: bitgetRelay('v3'),
    });
    expect(resolveExchangeRouting('okx', proxy).wsPrivateUta).toBeUndefined();
    expect(resolveExchangeRouting('bitget', offProxy).wsPublic).toEqual({
      tunnelled: false,
      fallback: 'wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public',
      initialSilentConnects: 1,
    });

    // wsRouteCandidates (lighter): gated by tealstreetWsRoute, NOT tealstreetProxy.
    expect(resolveExchangeRouting('lighter', wsRoute).wsPrivate.host).toBe(`${TYO}/lighter/stream`);
    expect(resolveExchangeRouting('lighter', wsRoute).wsPublic.host).toBe(`${TYO}/lighter/stream?readonly=true`);
    expect(resolveExchangeRouting('lighter', proxy).wsPrivate).toEqual({ tunnelled: false });
    expect(resolveExchangeRouting('lighter-robinhood', wsRoute).wsPrivate.host).toBe(`${TYO}/lighter-rh/stream`);
    expect(resolveExchangeRouting('lighter-robinhood', wsRoute).wsPublic.host).toBe(`${TYO}/lighter-rh/stream`);
    expect(resolveExchangeRouting('lighter-robinhood', proxy).wsPrivate).toEqual({ tunnelled: false });

    // bybitBrazilConditional: Bybit Brazil uses a dedicated proxy path that
    // injects x-site-id upstream. It is keyed only by bybitSiteId, not proxy flags.
    expect(resolveExchangeRouting('bybit', bybitBrazil).wsPrivate.host).toBe(`${TYO}/bybitbra/v5/private`);
    expect(resolveExchangeRouting('bybit', bybitBrazil).wsPublic.host).toBe(`${TYO}/bybitbra/v5/public/linear`);
    expect(resolveExchangeRouting('bybitinverse', bybitBrazil).wsPublic.host).toBe(
      `${TYO}/bybitbra/v5/public/inverse`,
    );
    expect(resolveExchangeRouting('bybitspot', bybitBrazil).wsPublic.host).toBe(`${TYO}/bybitbra/v5/public/spot`);
    // Without the site id the route is direct (plus the athena fallback, asserted below).
    expect(resolveExchangeRouting('bybit', offProxy).wsPrivate.tunnelled).toBe(false);
    expect(resolveExchangeRouting('bybitdemo', bybitBrazil).wsPrivate).toEqual({ tunnelled: false });
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
    // okxeu extends OKXExchange {} -> same okx ws classes, but tunnels to the EEA
    // entity's hosts (wseea.okx.com) via its own relay upstreams — EEA
    // credentials do not authenticate on the global ws host.
    expect(resolveExchangeRouting('okxeu', proxy).wsPrivate.host).toBe(relayOf('wss://wseea.okx.com/ws/v5/private'));
    expect(resolveExchangeRouting('okxeu', proxy).wsBusiness.host).toBe(
      relayOf('wss://wseea.okx.com/ws/v5/business'),
    );
    expect(resolveExchangeRouting('okx', proxy).wsPrivate.host).toBe(relayOf('wss://ws.okx.com/ws/v5/private'));
    expect(resolveExchangeRouting('okxdemo', proxy).wsPrivate).toEqual({ tunnelled: false });
    expect(resolveExchangeRouting('okxdemo', proxy).wsBusiness).toEqual({ tunnelled: false });
    expect(resolveExchangeRouting('okxeudemo', proxy).wsPrivate).toEqual({ tunnelled: false });
    expect(resolveExchangeRouting('okxeudemo', proxy).wsBusiness).toEqual({ tunnelled: false });
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
      relayOf('wss://advanced-trade-ws-user.coinbase.com/'),
    );
  });

  it('athena passthrough fallbacks (bybit/okx) attach only to DIRECT resolutions', () => {
    // Direct path -> the Tokyo passthrough rides along as `fallback` for the class's
    // WsDirectFallback. One hostname per venue; the path selects the channel.
    expect(resolveExchangeRouting('bybit', offProxy).wsPublic).toEqual({
      tunnelled: false,
      fallback: 'wss://bybit-proxy-ja.tealstreet.io/v5/public/linear',
    });
    expect(resolveExchangeRouting('bybit', offProxy).wsPrivate).toMatchObject({
      tunnelled: false,
      fallback: 'wss://bybit-proxy-ja.tealstreet.io/v5/private',
    });
    expect(resolveExchangeRouting('bybitinverse', offProxy).wsPublic.fallback).toBe(
      'wss://bybit-proxy-ja.tealstreet.io/v5/public/inverse',
    );
    expect(resolveExchangeRouting('bybitspot', offProxy).wsPublic.fallback).toBe(
      'wss://bybit-proxy-ja.tealstreet.io/v5/public/spot',
    );
    expect(resolveExchangeRouting('okx', offProxy).wsPublic).toEqual({
      tunnelled: false,
      fallback: 'wss://okx-proxy-ja.tealstreet.io/ws/v5/public',
    });
    expect(resolveExchangeRouting('okx', offProxy).wsBusiness.fallback).toBe(
      'wss://okx-proxy-ja.tealstreet.io/ws/v5/business',
    );

    // Tunnelled resolutions carry NO fallback: the Brazil path injects a header and
    // the tealstreetProxy path is a deliberate choice; neither may degrade to a
    // plain passthrough.
    expect(resolveExchangeRouting('bybit', bybitBrazil).wsPrivate).toEqual({
      tunnelled: true,
      host: `${TYO}/bybitbra/v5/private`,
    });
    expect(resolveExchangeRouting('bybit', bybitBrazil).wsPublic.fallback).toBeUndefined();
    expect(resolveExchangeRouting('okx', proxy).wsPrivate).toEqual({
      tunnelled: true,
      host: relayOf('wss://ws.okx.com/ws/v5/private'),
    });

    // Variants whose direct host is NOT stream.bybit.com / ws.okx.com get none:
    // demo and EEA terminate elsewhere, hyrotrader is a first-party simulator.
    for (const ex of ['bybitdemo', 'hyrotrader', 'okxeu', 'okxdemo', 'okxeudemo'] as ExchangeName[]) {
      expect(resolveExchangeRouting(ex, offProxy).wsPublic.fallback, ex).toBeUndefined();
      expect(resolveExchangeRouting(ex, offProxy).wsPrivate.fallback, ex).toBeUndefined();
    }
  });

  it('tunnel relay candidates attach to bybit/okx PRIVATE direct resolutions only', () => {
    // Relay url = the egress region's tunnel host + ?ts-relay=<private upstream>.
    // bybit is region-conditional (JA -> jp tunnel, else sg); both regions are the
    // one Vultr host since 3e.
    expect(resolveExchangeRouting('bybit', offProxy).wsPrivate.relay).toBe(
      'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fstream.bybit.com%2Fv5%2Fprivate',
    );
    expect(resolveExchangeRouting('bybit', { proxyRegion: 'JA' }).wsPrivate.relay).toBe(
      'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fstream.bybit.com%2Fv5%2Fprivate',
    );
    expect(resolveExchangeRouting('bybitinverse', offProxy).wsPrivate.relay).toContain(
      'stream.bybit.com%2Fv5%2Fprivate',
    );
    expect(resolveExchangeRouting('bybitspot', offProxy).wsPrivate.relay).toContain('stream.bybit.com%2Fv5%2Fprivate');
    // okx's HOW egress is whatever egressRegion.ts says; every region is one host.
    expect(resolveExchangeRouting('okx', offProxy).wsPrivate.relay).toBe(relayOf('wss://ws.okx.com/ws/v5/private'));
    expect(resolveExchangeRouting('okx', offProxy).wsBusiness.relay).toContain(
      'ws.okx.com%2Fws%2Fv5%2Fbusiness',
    );

    // Public channels never relay (the tunnel refuses public streams anyway).
    expect(resolveExchangeRouting('bybit', offProxy).wsPublic.relay).toBeUndefined();
    expect(resolveExchangeRouting('okx', offProxy).wsPublic.relay).toBeUndefined();

    // Tunnelled resolutions carry neither fallback nor relay.
    expect(resolveExchangeRouting('bybit', bybitBrazil).wsPrivate.relay).toBeUndefined();
    expect(resolveExchangeRouting('okx', proxy).wsPrivate.relay).toBeUndefined();

    // Variants whose direct host differs get nothing.
    for (const ex of ['bybitdemo', 'hyrotrader', 'okxeu', 'okxdemo', 'okxeudemo'] as ExchangeName[]) {
      expect(resolveExchangeRouting(ex, offProxy).wsPrivate.relay, ex).toBeUndefined();
    }
  });

  it('howEgress is region-conditional for the region-pinned exchanges', () => {
    // bybit (when tunnelling) pins the HOW adapter to proxyRegion:
    // JA -> jp, otherwise the bare-adapter fallback -> sg.
    for (const ex of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot'] as ExchangeName[]) {
      expect(resolveExchangeRouting(ex, { proxyRegion: 'JA' }).howEgress).toBe('jp');
      expect(resolveExchangeRouting(ex, { proxyRegion: 'SG' }).howEgress).toBe('sg');
      expect(resolveExchangeRouting(ex, {}).howEgress).toBe('sg');
    }
    // a non-pinned, non-TOKYO exchange stays sg regardless of region.
    expect(resolveExchangeRouting('phemex', { proxyRegion: 'JA' }).howEgress).toBe('sg');
    // a TOKYO-list exchange stays jp regardless of region.
    expect(resolveExchangeRouting('binance', { proxyRegion: 'SG' }).howEgress).toBe('jp');
  });

  it('howEgress single-source: table name-match == adapter baseURL-match for every exchange', () => {
    // The declarative table computes howEgress from isTokyoEgress(NAME); the HOW
    // adapter computes the runtime egress from isTokyoEgress(baseURL). For the table
    // to be a faithful description of the actual packet path, the two must agree for
    // every (non-region-pinned) exchange. This locks the single source: the egress
    // list now lives only in http-over-websocket/egressRegion, and any future drift
    // between an exchange's name and its baseURL match trips here.
    for (const exchange of Object.keys(exchangeMetadata) as ExchangeName[]) {
      const baseURL = REST_BASE_URLS[exchange];
      // No silent gaps: a new exchange added to metadata must declare its baseURL here.
      expect(baseURL, `missing frozen REST baseURL for ${exchange}`).toBeDefined();

      if (REGION_PINNED_HOW.includes(exchange)) continue;

      const byName = isTokyoEgress(exchange);
      const byBaseURL = isTokyoEgress(baseURL);
      expect(byName, `name vs baseURL egress disagree for ${exchange}`).toBe(byBaseURL);
      expect(resolveExchangeRouting(exchange, {}).howEgress).toBe(byBaseURL ? 'jp' : 'sg');
    }
  });

  it('resolved packet-path matrix (golden master)', () => {
    const BOOL = [false, true];
    const REGIONS: Array<string | undefined> = [undefined, 'JA', 'SG'];
    // Pin Brazil routing for every exchange, including venues where it is a no-op.
    const SITES = [undefined, 'BRA_BTL'] as const;
    const matrix: Record<string, Record<string, unknown>> = {};
    for (const exchange of Object.keys(exchangeMetadata) as ExchangeName[]) {
      const per: Record<string, unknown> = {};
      for (const tealstreetProxy of BOOL) {
        for (const tealstreetWsRoute of BOOL) {
          for (const proxyRegion of REGIONS) {
            for (const bybitSiteId of SITES) {
              const key = `proxy=${Number(tealstreetProxy)} wsRoute=${Number(tealstreetWsRoute)} region=${
                proxyRegion ?? 'NONE'
              } site=${bybitSiteId ?? 'NONE'}`;
              per[key] = resolveExchangeRouting(exchange, {
                tealstreetProxy,
                tealstreetWsRoute,
                ...(proxyRegion ? { proxyRegion } : {}),
                ...(bybitSiteId ? { bybitSiteId } : {}),
              });
            }
          }
        }
      }
      matrix[exchange] = per;
    }
    expect(matrix).toMatchSnapshot();
  });
});
