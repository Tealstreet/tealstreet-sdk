/**
 * KuCoin SPOT live integration test harness (manual, gitignored creds).
 *
 * Run:   ../../../node_modules/.bin/tsx src/exchanges/kucoin/test-live-spot.ts <phase>
 *   phase: rest | ws | order-limit | all
 *   env:   KC_TRANSPORT=direct|tunnel (default direct — forces Node http adapter)
 *
 * Spot trades BASE units directly (0.001 BTC, no contracts). Symbol = BTC-USDT.
 * order-limit places a post-only limit that cannot fill, then cancels by id.
 */
import { readFileSync } from 'fs';
import { join } from 'path';

import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { KucoinSpotExchange } from './kucoinspot.exchange';

const PHASE = process.argv[2] || 'rest';
const TRANSPORT = (process.env.KC_TRANSPORT || 'direct') as 'direct' | 'tunnel';
const SYMBOL = 'BTC-USDT';

const mask = (s?: string) => (s ? `${s.slice(0, 4)}…${s.slice(-3)}` : '(none)');
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const log = (...a: any[]) => console.log(...a);
const hr = (t: string) => log(`\n===== ${t} =====`);
const j = (v: any) => JSON.stringify(v);

async function pollUntil<T>(fn: () => Promise<T>, ok: (v: T) => boolean, ms = 8000, every = 1000): Promise<T> {
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
  const exchange = new KucoinSpotExchange(
    'kucoinspot' as any,
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
    (exchange.xhr.defaults as any).adapter = 'http';
    (exchange.unlimitedXHR.defaults as any).adapter = 'http';
    log('direct transport → forced http adapter on xhr + unlimitedXHR');
  }

  const bootstrap = async () => {
    const v = await exchange.validateAccount();
    log('validateAccount →', v === '' ? 'OK' : `ERROR: ${v}`);
    const markets = await exchange.fetchMarkets();
    store.update({ markets, loaded: { ...store.loaded, markets: true } });
    const tickers = await exchange.fetchTickers();
    store.update({ tickers, loaded: { ...store.loaded, tickers: true } });
    const btc = markets.find((m) => m.symbol === SYMBOL);
    if (!btc) throw new Error(`${SYMBOL} market not found`);
    return { markets, tickers, btc, ticker: store.tickersBySymbol[SYMBOL] };
  };

  if (PHASE === 'rest' || PHASE === 'all') {
    hr('REST');
    const { markets, btc, ticker } = await bootstrap();
    log('markets:', markets.length);
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
      },
    );
    log(
      'BTC ticker:',
      ticker && {
        bid: ticker.bid,
        ask: ticker.ask,
        last: ticker.last,
        percentage: ticker.percentage,
        volume: ticker.volume,
        quoteVolume: ticker.quoteVolume,
      },
    );

    const bal = await exchange.fetchBalance();
    log('balance USD total/free:', bal.total, '/', bal.free);
    log('  USDT:', bal.currencies?.USDT, '| BTC:', bal.currencies?.BTC);

    const orders = await exchange.fetchOrders([SYMBOL]);
    log('open BTC-USDT orders:', orders.length, j(orders.slice(0, 5)));
  }

  if (PHASE === 'ws' || PHASE === 'all') {
    hr('WEBSOCKET');
    const { btc } = await bootstrap();
    let obUpdates = 0;
    let candleUpdates = 0;
    let lastOb: any = null;
    let lastCandle: any = null;

    await exchange.publicWebsocket.connectAndSubscribe();
    await exchange.privateWebsocket.connectAndSubscribe();
    await pollUntil(
      async () => (exchange.publicWebsocket as any).ws?.readyState,
      (rs) => rs === 1,
      4000,
      250,
    );

    exchange.listenOrderBook(btc.symbol, (ob) => {
      obUpdates++;
      lastOb = ob;
    });
    exchange.listenOHLCV({ symbol: btc.symbol, interval: '1m' } as any, (c) => {
      candleUpdates++;
      lastCandle = c;
    });

    const tick = () => store.tickersBySymbol[btc.symbol];
    const t0 = tick();
    log('t0 ticker bid/ask/last:', t0 && [t0.bid, t0.ask, t0.last]);
    await sleep(9000);
    const t1 = tick();
    log(
      't1 ticker FULL:',
      t1 && { bid: t1.bid, ask: t1.ask, last: t1.last, percentage: t1.percentage, volume: t1.volume },
    );
    log(`orderbook updates: ${obUpdates} | top:`, lastOb && [lastOb.bids?.[0], lastOb.asks?.[0]]);
    log(`candle updates: ${candleUpdates} | last:`, lastCandle);
    log('public WS readyState:', (exchange.publicWebsocket as any).ws?.readyState);
    log('private WS readyState:', (exchange.privateWebsocket as any).ws?.readyState);
  }

  if (PHASE === 'order-limit' || PHASE === 'all') {
    hr('ORDER: spot limit place + cancel (no fill)');
    const { btc, ticker } = await bootstrap();
    const last = ticker?.last;
    if (!last) throw new Error('no BTC price');
    const amount = 0.001;
    // A post-only BUY reserves amount*price USDT. The spot trade wallet is
    // small, so cap the price to what's affordable (and far below market so it
    // can't fill). min order is 0.00001 BTC.
    const bal = await exchange.fetchBalance();
    const freeUsdt = bal.currencies?.USDT?.free ?? 0;
    const budgetPrice = Math.floor((freeUsdt * 0.9) / amount);
    const farPrice = Math.max(1, Math.min(Math.round(last * 0.5), budgetPrice));
    log(
      `BTC last=${last}, free USDT=${freeUsdt} → post-only limit BUY ${amount} BTC @ ${farPrice} (reserves ~${(amount * farPrice).toFixed(2)} USDT)`,
    );

    const ids = await exchange
      .placeOrder({
        symbol: btc.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount,
        price: farPrice,
        timeInForce: OrderTimeInForce.PostOnly,
      } as any)
      .catch((e: any) => {
        log('placeOrder threw:', e?.message);
        return [] as string[];
      });
    log('placeOrder → ids', j(ids));
    const myId = String((ids as any)?.[0] ?? '');
    if (!myId) {
      log('no order id (likely insufficient spot USDT) — see emitter error above');
      return;
    }

    const orders = await pollUntil(
      () => exchange.fetchOrders([btc.symbol]),
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
        timeInForce: mine.timeInForce,
      },
    );

    if (mine) {
      await exchange.cancelOrders([mine]);
      log('cancelOrders([mine]) called');
    }
    const after = await pollUntil(
      () => exchange.fetchOrders([btc.symbol]),
      (os) => !os.some((o) => String(o.id) === myId),
      8000,
    );
    log(
      'my order still open after cancel:',
      after.some((o) => String(o.id) === myId),
    );
  }

  if (PHASE === 'cancel') {
    // Cleanup: cancel a specific resting order by id (argv[3]) on BTC-USDT.
    hr('CANCEL by id');
    await bootstrap();
    const id = process.argv[3];
    if (!id) throw new Error('usage: cancel <orderId>');
    await exchange.cancelOrders([{ id, symbol: SYMBOL } as any]);
    log('cancelOrders called for', id);
    const open = await exchange.fetchOrders([SYMBOL]);
    log('open BTC-USDT orders now:', open.length, j(open.map((o) => ({ id: o.id, price: o.price, amount: o.amount }))));
  }

  if (PHASE === 'trade-ws') {
    hr('TRADE + WS PINGBACKS (spot)');
    const { btc, ticker } = await bootstrap();
    const last = ticker?.last || 0;
    const bal0 = await exchange.fetchBalance();
    const freeUsdt = bal0.currencies?.USDT?.free ?? 0;
    log(`last≈${last}, free USDT=${freeUsdt}`);
    const amount = 0.0001; // ~$7 at $71k — fits the ~10 USDT budget

    await exchange.publicWebsocket.connectAndSubscribe();
    await exchange.privateWebsocket.connectAndSubscribe();
    await pollUntil(
      async () => (exchange.privateWebsocket as any).ws?.readyState,
      (rs) => rs === 1,
      6000,
      250,
    );

    const frames: { t: number; step: string; topic: string; data: any }[] = [];
    const emitterEvents: { step: string; kind: string; e: any }[] = [];
    let step = 'idle';
    (exchange.privateWebsocket as any).ws?.addEventListener('message', (e: any) => {
      try {
        const m = JSON.parse(e.data);
        if (['welcome', 'pong', 'ack'].includes(m.type)) return;
        frames.push({ t: Date.now(), step, topic: m.topic || m.subject, data: m.data });
      } catch {
        /* noop */
      }
    });
    for (const k of ['fill', 'error', 'warning'] as const) {
      exchange.emitter.on(k, (e: any) => emitterEvents.push({ step, kind: k, e }));
    }
    const storeOrder = (id: string) => exchange.store.orders.find((o) => String(o.id) === id);

    const runStep = async (name: string, fn: () => Promise<void>, waitMs = 4000) => {
      step = name;
      hr(`STEP ${name}`);
      try {
        await fn();
      } catch (e: any) {
        log(`  ⚠️ step threw: ${e?.response?.data?.msg || e?.message}`);
      }
      await sleep(waitMs);
      const fr = frames.filter((x) => x.step === name);
      log(`  private-WS frames (${fr.length}):`);
      fr.forEach((x) =>
        log(`   → ${x.topic} type=${x.data?.type} status=${x.data?.status} ${j(x.data || {}).slice(0, 240)}`),
      );
      const ev = emitterEvents.filter((x) => x.step === name);
      if (ev.length) log('  emitter events:', j(ev));
      step = 'idle';
    };

    // S-1: limit place + cancel (S1: is the cancel transiently mislabeled Closed?)
    let s1Id = '';
    const farPrice = Math.max(1, Math.min(Math.round(last * 0.5), Math.floor((freeUsdt * 0.9) / amount)));
    await runStep('S1-place-limit', async () => {
      const ids = await exchange.placeOrder({
        symbol: btc.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount,
        price: farPrice,
        timeInForce: OrderTimeInForce.PostOnly,
      } as any);
      s1Id = String((ids as any)?.[0] ?? '');
      log(
        `  placed id=${s1Id} @ ${farPrice}; store:`,
        j(
          storeOrder(s1Id) && {
            type: storeOrder(s1Id)!.type,
            price: storeOrder(s1Id)!.price,
            amount: storeOrder(s1Id)!.amount,
            status: storeOrder(s1Id)!.status,
          },
        ),
      );
    });
    await runStep(
      'S1-cancel-limit',
      async () => {
        await exchange.cancelOrders([storeOrder(s1Id) || ({ id: s1Id, symbol: btc.symbol } as any)]);
        // sample store status rapidly to catch the WS-handler value BEFORE the 250ms REST refetch heals it
        for (let i = 0; i < 8; i++) {
          await sleep(40);
          log(`   t+${(i + 1) * 40}ms store status: ${storeOrder(s1Id)?.status ?? 'GONE'}`);
        }
      },
      3000,
    );

    // S-2: tiny market fill round-trip (S2: does a fill event fire, in base units?)
    await runStep('S2-market-buy', async () => {
      const b = await exchange.fetchBalance();
      if ((b.currencies?.USDT?.free ?? 0) < amount * last * 1.1) {
        log('  SKIP: insufficient USDT for a market buy');
        return;
      }
      await exchange.placeOrder({ symbol: btc.symbol, type: OrderType.Market, side: OrderSide.Buy, amount } as any);
    });
    await runStep('S2-market-sell', async () => {
      const b = await exchange.fetchBalance();
      const btcFree = b.currencies?.BTC?.free ?? 0;
      if (btcFree >= amount * 0.8) {
        await exchange.placeOrder({
          symbol: btc.symbol,
          type: OrderType.Market,
          side: OrderSide.Sell,
          amount: btcFree,
        } as any);
      } else {
        log(`  nothing to flatten (BTC free=${btcFree})`);
      }
    });

    // S-3: TP/SL rejection (S3: does it emit before throwing, or silently throw?)
    await runStep('S3-tpsl-reject', async () => {
      const before = emitterEvents.length;
      try {
        await exchange.placeOrder({
          symbol: btc.symbol,
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount,
          price: farPrice,
          stopLoss: Math.round(last * 0.4),
        } as any);
        log('  ⚠️ unexpectedly did NOT throw');
      } catch (e: any) {
        log(`  threw: "${e?.message}"`);
        log(
          `  >>> emitter events fired by the throw: ${emitterEvents.length - before} (S3 bug → 0 = silent, no toast)`,
        );
      }
    });

    hr('FINAL STATE (spot)');
    const finalOrders = await exchange.fetchOrders([btc.symbol]);
    log('open orders:', finalOrders.length, j(finalOrders.map((o) => ({ id: o.id, status: o.status }))));
    const finalBal = await exchange.fetchBalance();
    log('USDT total:', finalBal.currencies?.USDT?.total, '| BTC total:', finalBal.currencies?.BTC?.total);
  }

  if (PHASE === 'limit-tests') {
    hr('LIMIT ORDER TESTS (spot) — resting place/cancel + immediately-filled limits');
    const { btc } = await bootstrap();
    const amount = 0.0001;

    await exchange.publicWebsocket.connectAndSubscribe();
    await exchange.privateWebsocket.connectAndSubscribe();
    await pollUntil(
      async () => (exchange.privateWebsocket as any).ws?.readyState,
      (rs) => rs === 1,
      6000,
      250,
    );

    const frames: { step: string; topic: string; data: any }[] = [];
    const fills: { step: string; e: any }[] = [];
    let step = 'idle';
    (exchange.privateWebsocket as any).ws?.addEventListener('message', (e: any) => {
      try {
        const m = JSON.parse(e.data);
        if (['welcome', 'pong', 'ack'].includes(m.type)) return;
        frames.push({ step, topic: m.topic || m.subject, data: m.data });
      } catch {
        /* noop */
      }
    });
    exchange.emitter.on('fill', (e: any) => fills.push({ step, e }));
    const storeOrder = (id: string) => exchange.store.orders.find((o) => String(o.id) === id);
    const liveTicker = () => store.tickersBySymbol[btc.symbol];

    const runStep = async (name: string, fn: () => Promise<void>, waitMs = 4000) => {
      step = name;
      hr(`STEP ${name}`);
      try {
        await fn();
      } catch (e: any) {
        log(`  ⚠️ threw: ${e?.response?.data?.msg || e?.message}`);
      }
      await sleep(waitMs);
      const fr = frames.filter((x) => x.step === name && `${x.topic}`.includes('tradeOrdersV2'));
      log(`  order WS frames (${fr.length}):`);
      fr.forEach((x) =>
        log(
          `   → type=${x.data?.type} status=${x.data?.status} filled=${x.data?.filledSize} match=${x.data?.matchSize}`,
        ),
      );
      const sf = fills.filter((x) => x.step === name);
      if (sf.length) log('  fill events:', j(sf.map((x) => x.e)));
      step = 'idle';
    };

    // --- A: place TWO resting limits, then cancel both (multi-cancel) ---
    let idA = '';
    let idB = '';
    await runStep('A-place-2-resting', async () => {
      const t = liveTicker();
      const pA = Math.round((t?.last || 0) * 0.5);
      const pB = Math.round((t?.last || 0) * 0.4);
      [idA] = (await exchange.placeOrder({
        symbol: btc.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount,
        price: pA,
        timeInForce: OrderTimeInForce.PostOnly,
      } as any)) as any;
      [idB] = (await exchange.placeOrder({
        symbol: btc.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount,
        price: pB,
        timeInForce: OrderTimeInForce.PostOnly,
      } as any)) as any;
      log(`  placed A=${idA} @ ${pA}, B=${idB} @ ${pB}`);
      log(`  store: A=${storeOrder(String(idA))?.status} B=${storeOrder(String(idB))?.status}`);
    });
    await runStep('A-cancel-both', async () => {
      const os = [storeOrder(String(idA)), storeOrder(String(idB))].filter(Boolean) as any[];
      await exchange.cancelOrders(
        os.length
          ? os
          : ([
              { id: idA, symbol: btc.symbol },
              { id: idB, symbol: btc.symbol },
            ] as any),
      );
      await sleep(1200);
      log(
        `  store after cancel: A=${storeOrder(String(idA))?.status ?? 'GONE'} B=${storeOrder(String(idB))?.status ?? 'GONE'}`,
      );
    });

    // --- B: immediately-filled limit BUY (price crosses the ask → fills as taker) ---
    let idFill = '';
    await runStep('B-immediate-fill-buy', async () => {
      const t = liveTicker();
      const cross = Math.round((t?.ask || t?.last || 0) * 1.003); // above ask → marketable limit
      log(`  ask≈${t?.ask} → GTC limit BUY ${amount} @ ${cross} (crosses; fills immediately)`);
      [idFill] = (await exchange.placeOrder({
        symbol: btc.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount,
        price: cross,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      } as any)) as any;
      log(`  placed id=${idFill}`);
    });
    log(`>>> B store order status: ${storeOrder(String(idFill))?.status ?? '(not in store)'} (expect closed/filled)`);

    // --- C: sell it back via an immediately-filled limit SELL ---
    await runStep('C-immediate-fill-sell', async () => {
      const b = await exchange.fetchBalance();
      const btcFree = b.currencies?.BTC?.free ?? 0;
      if (btcFree < amount * 0.8) {
        log(`  nothing to sell (BTC free=${btcFree})`);
        return;
      }
      const t = liveTicker();
      const cross = Math.round((t?.bid || t?.last || 0) * 0.997); // below bid → marketable limit
      log(`  bid≈${t?.bid} → GTC limit SELL ${btcFree} @ ${cross} (crosses; fills immediately)`);
      await exchange.placeOrder({
        symbol: btc.symbol,
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: btcFree,
        price: cross,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      } as any);
    });

    hr('FINAL STATE (spot)');
    const finalOrders = await exchange.fetchOrders([btc.symbol]);
    log('open orders:', finalOrders.length, j(finalOrders.map((o) => ({ id: o.id, status: o.status }))));
    const fb = await exchange.fetchBalance();
    log('USDT total:', fb.currencies?.USDT?.total, '| BTC total:', fb.currencies?.BTC?.total);
  }

  log('\nDONE.');
  process.exit(0);
}

main().catch((e) => {
  console.error('FATAL:', e?.stack || e?.message || e);
  process.exit(1);
});
