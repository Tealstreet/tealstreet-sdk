/**
 * KuCoin Futures live integration test harness (manual, gitignored creds).
 *
 * Run:   ../../../node_modules/.bin/tsx src/exchanges/kucoin/test-live.ts <phase>
 *   phase: rest | ws | order-limit | order-market | all
 *   env:   KC_TRANSPORT=direct|tunnel   (default direct — forces Node http
 *          adapter, bypassing the HOW tunnel that kucoin installs for auth REST)
 *
 * Safety: trades XBTUSDTM only, 1 contract = 0.001 BTC. Never opens past
 * MAX_CONTRACTS (=5 → 0.005 BTC). Order phases flatten on exit.
 */
import { readFileSync } from 'fs';
import { join } from 'path';

import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { KucoinExchange } from './kucoin.exchange';
import { createKucoinSnapshotWebsockets } from './kucoin.ws-public';

const PHASE = process.argv[2] || 'rest';
const TRANSPORT = (process.env.KC_TRANSPORT || 'direct') as 'direct' | 'tunnel';
const MAX_BASE_BTC = 0.005; // hard cap on total BTC position size

const mask = (s?: string) => (s ? `${s.slice(0, 4)}…${s.slice(-3)}` : '(none)');
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const log = (...a: any[]) => console.log(...a);
const hr = (t: string) => log(`\n===== ${t} =====`);
const j = (v: any) => JSON.stringify(v);

async function pollUntil<T>(fn: () => Promise<T>, ok: (v: T) => boolean, ms = 12000, every = 1000): Promise<T> {
  const start = Date.now();
  let last = await fn();
  while (!ok(last) && Date.now() - start < ms) {
    await sleep(every);
    last = await fn();
  }
  return last;
}

async function main() {
  const cfg = JSON.parse(readFileSync(join(__dirname, '.apikey.json'), 'utf-8'));
  log(`creds key=${mask(cfg.key)} pass=${mask(cfg.passphrase)} | transport=${TRANSPORT} | phase=${PHASE}`);

  await virtualClock.start().catch((e: any) => log('virtualClock.start failed (continuing):', e?.message));

  const store = new DefaultStore();
  const exchange = new KucoinExchange(
    'kucoin' as any,
    {
      key: cfg.key,
      secret: cfg.secret,
      passphrase: cfg.passphrase,
      testnet: !!cfg.testnet,
      tealstreetTunnel: TRANSPORT === 'tunnel',
      tealstreetProxy: false,
    } as any,
    store,
  );

  exchange.emitter.on('error', (e: any) => log('  [emitter:error]', typeof e === 'string' ? e : j(e)));
  exchange.emitter.on('warning', (e: any) => log('  [emitter:warning]', typeof e === 'string' ? e : j(e)));
  exchange.emitter.on('fill', (e: any) => log('  [emitter:FILL]', j(e)));

  if (TRANSPORT === 'direct') {
    // kucoin.api.ts installs the HOW tunnel adapter for all authenticated REST.
    // For a direct-from-Node test, force the standard http adapter instead.
    (exchange.xhr.defaults as any).adapter = 'http';
    (exchange.unlimitedXHR.defaults as any).adapter = 'http';
    log('direct transport → forced http adapter on xhr + unlimitedXHR');
  }

  // ---- shared bootstrap: validate + load markets/tickers ----
  const bootstrap = async () => {
    const v = await exchange.validateAccount();
    log('validateAccount →', v === '' ? 'OK' : `ERROR: ${v}`);
    const markets = await exchange.fetchMarkets();
    store.update({ markets, loaded: { ...store.loaded, markets: true } });
    const tickers = await exchange.fetchTickers();
    store.update({ tickers, loaded: { ...store.loaded, tickers: true } });
    const btc =
      markets.find((m) => m.base === 'BTC' && m.quote === 'USDT' && !m.isExpiring) ||
      markets.find((m) => m.id === 'XBTUSDTM');
    if (!btc) throw new Error('BTC futures market not found');
    const ticker = store.tickersBySymbol[btc.symbol];
    return { markets, tickers, btc, ticker };
  };

  const btcPositionContracts = async (symbol: string) => {
    const positions = await exchange.fetchPositions([symbol]);
    const p = positions.find((p) => p.symbol === symbol && p.contracts > 0);
    return { contracts: p?.contracts ?? 0, side: p?.side, position: p };
  };

  // ============ PHASES ============
  if (PHASE === 'rest' || PHASE === 'all') {
    hr('REST');
    const v = await exchange.validateAccount();
    log('validateAccount →', v === '' ? 'OK (empty)' : `ERROR: ${v}`);

    const markets = await exchange.fetchMarkets();
    store.update({ markets, loaded: { ...store.loaded, markets: true } });
    log('markets:', markets.length);
    const btc =
      markets.find((m) => m.base === 'BTC' && m.quote === 'USDT' && !m.isExpiring) ||
      markets.find((m) => m.id === 'XBTUSDTM');
    log(
      'BTC market:',
      btc && {
        id: btc.id,
        symbol: btc.symbol,
        base: btc.base,
        quote: btc.quote,
        contractSize: btc.contractSize,
        precision: btc.precision,
        limits: btc.limits,
        active: btc.active,
      },
    );

    const tickers = await exchange.fetchTickers();
    store.update({ tickers, loaded: { ...store.loaded, tickers: true } });
    const t = btc && tickers.find((x) => x.symbol === btc.symbol);
    log(
      'BTC ticker:',
      t && {
        symbol: t.symbol,
        bid: t.bid,
        ask: t.ask,
        last: t.last,
        mark: t.mark,
        index: t.index,
        percentage: t.percentage,
        fundingRate: t.fundingRate,
        volume: t.volume,
      },
    );

    const bal = await exchange.fetchBalance();
    log('balance:', bal);

    const positions = await exchange.fetchPositions(btc ? [btc.symbol] : undefined);
    log('positions: total', positions.length, '| non-zero', j(positions.filter((p) => p.contracts > 0)));

    const orders = await exchange.fetchOrders();
    log('open orders:', orders.length, j(orders.slice(0, 5)));
  }

  if (PHASE === 'ws' || PHASE === 'all') {
    hr('WEBSOCKET');
    const { btc } = await bootstrap();
    log('subscribing public+private WS for', btc.symbol, `(${btc.id})`);

    let obUpdates = 0;
    let candleUpdates = 0;
    let lastOb: any = null;
    let lastCandle: any = null;

    await exchange.publicWebsocket.connectAndSubscribe();
    // Spin up the snapshot-socket pool exactly like start() does (bulk
    // last/24h%/funding/volume, sharded under the 400-topic cap).
    exchange.snapshotWebsockets = createKucoinSnapshotWebsockets(exchange, store.markets.length);
    exchange.snapshotWebsockets.forEach((ws) => ws.connectAndSubscribe());
    log(`snapshot pool size: ${exchange.snapshotWebsockets.length} connection(s)`);
    await exchange.privateWebsocket.connectAndSubscribe();
    // Wait for the public socket to actually be OPEN before per-symbol subs,
    // else listenOrderBook/listenOHLCV send their subscribe frame to a closed
    // socket (mirrors the app, where these are called after start()).
    await pollUntil(
      async () => (exchange.publicWebsocket as any).ws?.readyState,
      (rs) => rs === 1,
      4000,
      250,
    );
    // Raw frame counters straight off the socket — distinguishes "subscribe
    // never took effect" (raw=0) from "frames arrive but handler drops them".
    let rawL2 = 0;
    let rawCandle = 0;
    (exchange.publicWebsocket as any).ws?.addEventListener('message', (e: any) => {
      const t = (() => {
        try {
          return JSON.parse(e.data)?.topic || '';
        } catch {
          return '';
        }
      })();
      if (t.includes('level2')) rawL2++;
      if (t.includes('limitCandle')) rawCandle++;
    });

    exchange.listenOrderBook(btc.symbol, (ob) => {
      obUpdates++;
      lastOb = ob;
    });
    exchange.listenOHLCV({ symbol: btc.symbol, interval: '1m' } as any, (c) => {
      candleUpdates++;
      lastCandle = c;
    });

    // sample tickers over time to confirm live updates
    const tick = () => store.tickersBySymbol[btc.symbol];
    const t0 = tick();
    log('t0 ticker bid/ask/last:', t0 && [t0.bid, t0.ask, t0.last]);
    await sleep(10000);
    const t1 = tick();
    log(
      't1 ticker FULL:',
      t1 && {
        bid: t1.bid,
        ask: t1.ask,
        last: t1.last,
        mark: t1.mark,
        index: t1.index,
        percentage: t1.percentage,
        fundingRate: t1.fundingRate,
        volume: t1.volume,
        quoteVolume: t1.quoteVolume,
      },
    );
    log(
      `orderbook: raw level2 frames=${rawL2} | forwarded callbacks=${obUpdates} | top:`,
      lastOb && [lastOb.bids?.[0], lastOb.asks?.[0]],
    );
    log(`candle: raw limitCandle frames=${rawCandle} | forwarded callbacks=${candleUpdates} | last:`, lastCandle);
    log('private WS readyState:', (exchange.privateWebsocket as any).ws?.readyState);
    log('public WS readyState:', (exchange.publicWebsocket as any).ws?.readyState);

    exchange.publicWebsocket.dispose?.();
    exchange.privateWebsocket.dispose?.();
  }

  if (PHASE === 'order-limit' || PHASE === 'all') {
    hr('ORDER: limit place + cancel (no fill)');
    const { btc, ticker } = await bootstrap();
    const last = ticker?.last || ticker?.mark;
    if (!last) throw new Error('no BTC price');
    const farPrice = Math.round(last * 0.5); // far below market → post-only BUY that cannot fill
    log(`BTC last=${last} → placing post-only limit BUY 1 contract @ ${farPrice} (far below, won't fill)`);

    const ids = await exchange.placeOrder({
      symbol: btc.symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: farPrice,
      timeInForce: OrderTimeInForce.PostOnly,
    } as any);
    log('placeOrder → ids', j(ids));
    const myId = String((ids as any)?.[0] ?? '');

    // Find MY order by id only — never touch the account's pre-existing TP/SL.
    const orders = await pollUntil(
      () => exchange.fetchOrders(),
      (os) => os.some((o) => String(o.id) === myId),
      8000,
    );
    const mine = orders.find((o) => String(o.id) === myId);
    log(
      'my order:',
      mine && {
        id: mine.id,
        symbol: mine.symbol,
        side: mine.side,
        type: mine.type,
        price: mine.price,
        amount: mine.amount,
        status: mine.status,
        reduceOnly: mine.reduceOnly,
      },
    );

    if (mine) {
      await exchange.cancelOrders([mine]);
      log('cancelOrders([mine]) called');
    } else {
      log('⚠️ my order not found to cancel');
    }
    const after = await pollUntil(
      () => exchange.fetchOrders(),
      (os) => !os.some((o) => String(o.id) === myId),
      8000,
    );
    log(
      'my order still open after cancel:',
      after.some((o) => String(o.id) === myId),
    );
  }

  if (PHASE === 'order-market' || PHASE === 'all') {
    hr('ORDER: market open 0.001 BTC + close (surgical — restores pre-existing position)');
    const { btc } = await bootstrap();
    const cs = btc.contractSize ?? 1; // 0.001 BTC per contract for XBTUSDTM
    let openedNative = 0; // native contracts I added (to close back out)
    try {
      const pre = await btcPositionContracts(btc.symbol); // base BTC (post-fix)
      log(`pre-existing position: ${pre.contracts} BTC (${pre.side ?? 'flat'})`);
      if (pre.contracts + cs > MAX_BASE_BTC) {
        throw new Error(`pre (${pre.contracts}) + 0.001 would exceed ${MAX_BASE_BTC} BTC; aborting`);
      }

      log('placing MARKET BUY 1 contract (0.001 BTC)…');
      const ids = await exchange.placeOrder({
        symbol: btc.symbol,
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 1,
      } as any);
      openedNative = 1;
      log('placeOrder → ids', j(ids));

      const opened = await pollUntil(
        () => btcPositionContracts(btc.symbol),
        (s) => s.contracts >= pre.contracts + cs - 1e-9,
        12000,
      );
      log(
        `position after open: ${opened.contracts} BTC (${opened.side}) | native=${(opened.position as any)?.extra?.contractsNative}`,
      );
      log(
        'mapped position:',
        opened.position && {
          contracts: opened.position.contracts,
          notional: opened.position.notional,
          entryPrice: opened.position.entryPrice,
          leverage: opened.position.leverage,
          unrealizedPnl: opened.position.unrealizedPnl,
        },
      );
      if (opened.contracts > MAX_BASE_BTC + 1e-9) log('⚠️ exceeded max — closing immediately');
    } finally {
      if (openedNative > 0) {
        hr('CLOSE my delta (reduce-only — leaves pre-existing position intact)');
        try {
          await exchange.placeOrder({
            symbol: btc.symbol,
            type: OrderType.Market,
            side: OrderSide.Sell,
            amount: openedNative,
            reduceOnly: true,
          } as any);
          await sleep(2500);
          const final = await btcPositionContracts(btc.symbol);
          log(`position after close: ${final.contracts} BTC (${final.side ?? 'flat'})`);
        } catch (e: any) {
          log('⚠️ CLOSE FAILED — manual check needed:', e?.message);
        }
      }
    }
  }

  log('\nDONE.');
  process.exit(0);
}

main().catch((e) => {
  console.error('FATAL:', e?.stack || e?.message || e);
  process.exit(1);
});
