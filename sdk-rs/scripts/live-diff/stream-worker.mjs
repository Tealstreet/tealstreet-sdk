// Production account APIs only. TS Store delivery uses ExchangeWorker's actual
// payload preparer/throttle; nativeAccountOpen already owns that Rust pipeline.
import { createRequire } from 'node:module';
import { setTimeout as realTimeout } from 'node:timers';
import { replayClock, socketReceipts, httpReceipts } from './replay-clock.mjs';
import { verifyRawTaskDrain } from './stream-relay.mjs';
import { exchangeRegistry } from '../../../packages/safe-cex/src/exchanges/registry.ts';
import { DefaultStore } from '../../../packages/safe-cex/src/store/store.base.ts';
import { ExchangeWorker } from '../../../packages/safe-cex/src/webworker/ExchangeWorker.ts';

console.log = console.warn = console.error = () => {};
const implementation = process.argv[2];
let instance, delivery, native, id, clock, receipts, pendingHttp;
let state = {},
  observing = false;
let outputs = [];
const cleanups = [];
function emit(value) {
  if (observing) outputs.push(structuredClone(value));
}
function onStore(slices) {
  Object.assign(state, slices);
  emit({ kind: 'store', data: slices });
}
function onEvent(event, data) {
  if (event === 'listenOrderBook' || event === 'listenOHLCV') {
    emit({ kind: event === 'listenOrderBook' ? 'orderbook' : 'candles', data: data.data });
  } else emit({ kind: 'event', event, ...(event === 'ws-reconnected' ? {} : { data }) });
}
process.on('message', async (message) => {
  await new Promise(setImmediate);
  try {
    let value;
    if (message.method === 'open') {
      const options = message.options;
      if (message.replayEpoch !== undefined && implementation === 'ts') {
        clock = replayClock(message.replayEpoch);
        receipts = socketReceipts();
        pendingHttp = httpReceipts();
      }
      if (implementation === 'ts') {
        const Exchange = await exchangeRegistry[message.exchange]();
        instance = new Exchange(message.exchange, options, new DefaultStore());
        delivery = new ExchangeWorker(
          { postMessage: ({ op, data }) => (op === 'store' ? onStore(data) : onEvent(op, data)) },
          'live-public-probe',
          message.exchange,
          options,
          { placeOrder: {} },
          { supportedTPSLTriggerTypes: [], privateProxyIps: [] },
          '',
        );
        // Attach the constructed venue to the real production worker delivery
        // pipeline without starting a second venue or external clock service.
        delivery._exchange = instance;
        delivery._store = instance.store;
        delivery._initializeProcessorsAndHandlers();
        delivery._setupExchangeEventListeners();
      } else {
        native = createRequire(import.meta.url)(process.env.LIVE_DIFF_ADDON);
        id = await native.nativeAccountOpen(
          { exchange: message.exchange, options, replayEpochMs: message.replayEpoch },
          ({ event, data }) => onEvent(event, data),
          onStore,
        );
      }
    } else if (message.method === 'start') {
      if (implementation === 'ts') {
        await instance.start();
        delivery.sendSnapshot();
      } else {
        if (!(await native.nativeAccountCallMethod(id, 'start', []))) throw new Error('Native startup failed');
      }
    } else if (message.method === 'describe') {
      const markets = implementation === 'ts' ? instance.store.markets : state.markets;
      value = {
        markets: (markets ?? []).map(({ id, symbol, base, quote, active }) => ({ id, symbol, base, quote, active })),
      };
    } else if (message.method === 'subscribe') {
      const [symbol, interval, to] = message.args;
      const candleOptions = { symbol, interval, to };
      if (implementation === 'ts') {
        cleanups.push(instance.listenOrderBook(symbol, (data) => emit({ kind: 'orderbook', data })));
        cleanups.push(instance.listenOHLCV(candleOptions, (data) => emit({ kind: 'candles', data })));
      } else {
        for (const [method, args] of [
          ['listenOrderBook', [symbol]],
          ['listenOHLCV', [candleOptions]],
        ]) {
          const subscription = await native.nativeAccountCallMethod(id, method, args);
          cleanups.push(() => native.nativeAccountCallMethod(id, 'unlisten', [subscription]));
        }
      }
    } else if (message.method === 'begin') {
      if (clock) {
        delivery._throttledStoreUpdate.cancel();
        clock.begin();
      } else if (implementation === 'rust') {
        value = await native.nativeAccountReplay(id, { op: 'begin' });
      }
      outputs = [];
      observing = true;
    } else if (message.method === 'advance') {
      if (implementation === 'ts') {
        await clock.advance(message.args[0]);
        value = { atMs: clock.now(), sockets: receipts.sockets(), pendingJobs: pendingHttp() + receipts.pendingJobs() };
      } else value = await native.nativeAccountReplay(id, { op: 'advance', atMs: message.args[0] });
    } else if (message.method === 'receipt') {
      value =
        implementation === 'ts'
          ? { atMs: clock.now(), sockets: receipts.sockets(), pendingJobs: pendingHttp() + receipts.pendingJobs() }
          : await native.nativeAccountReplay(id, { op: 'receipt' });
    } else if (message.method === 'barrier') {
      if (implementation === 'rust') await native.nativeAccountCallMethod(id, 'getActiveTickerSymbols', []);
      // Finish startup subscription Promises under real scheduling before
      // activating the replay clock. Post-play barriers require the same fence.
      if (implementation === 'rust' || clock) {
        while (true) {
          const receipt =
            implementation === 'rust'
              ? await native.nativeAccountReplay(id, { op: 'receipt' })
              : { pendingJobs: pendingHttp() + receipts.pendingJobs() };
          if (!Number.isSafeInteger(receipt.pendingJobs)) throw new Error('Missing pending-job receipt');
          if (implementation === 'rust' && (!receipt.rawTasks || receipt.rawTasks.version !== 1))
            throw new Error('Addon lacks raw-task receipts');
          if (receipt.pendingJobs === 0 && (implementation !== 'rust' || verifyRawTaskDrain(receipt.rawTasks))) break;
          await new Promise((resolve) => realTimeout(resolve, 1));
        }
      }
      // Include the trailing production Store throttle after the last frame.
      await new Promise((resolve) => realTimeout(resolve, 350));
    } else if (message.method === 'drain') {
      observing = false;
      value = { outputs, state };
      outputs = [];
    } else if (message.method === 'close') {
      observing = false;
      for (const cleanup of cleanups) await cleanup?.();
      if (implementation === 'ts') delivery?.dispose();
      else if (id !== undefined) await native.nativeAccountClose(id);
    } else throw new Error(`Unknown stream probe method ${message.method}`);
    await new Promise(setImmediate);
    process.send({ requestId: message.requestId, value });
  } catch (error) {
    process.send({
      requestId: message.requestId,
      failure: { name: error.name, message: String(error.message).slice(0, 500) },
    });
  }
});
