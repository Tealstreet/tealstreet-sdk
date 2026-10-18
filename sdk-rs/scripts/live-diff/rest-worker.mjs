import { createRequire } from 'node:module';
import { setTimeout as realTimeout } from 'node:timers';
import { exchangeRegistry } from '../../../packages/safe-cex/src/exchanges/registry.ts';
import { BaseExchange } from '../../../packages/safe-cex/src/exchanges/base.ts';
import { DefaultStore } from '../../../packages/safe-cex/src/store/store.base.ts';
import { virtualClock } from '../../../packages/safe-cex/src/utils/virtualClock.ts';
import { replayClock } from './replay-clock.mjs';
import { publicReadMethods } from './rest-plan.mjs';

console.log = console.warn = console.error = () => {};
const implementation = process.argv[2];
let clock,
  instance,
  native,
  id,
  store = {},
  errors = 0;
const allowed = new Set([...publicReadMethods, 'refreshMarkets', 'disablePublicWebsockets', 'start']);
process.on('message', async (message) => {
  const before = errors;
  try {
    let value;
    if (message.method === 'open') {
      const options = {
        key: '',
        secret: '',
        publicDataOnly: true,
        tealstreetProxy: false,
        tealstreetTunnel: false,
        tealstreetWsRoute: false,
        hostedMode: true,
        enableOptionMarkets:
          message.accountOptions?.enableOptionMarkets ?? ['bybit', 'bybitdemo'].includes(message.exchange),
        tuning: { consoleLogging: false },
      };
      if (implementation === 'ts') {
        if (!message.realtimeClock) {
          clock = replayClock(message.epoch);
          clock.begin();
          virtualClock.getCurrentTimeMs = () => message.epoch + clock.now();
        }
        const Exchange = await exchangeRegistry[message.exchange]();
        instance = new Exchange(message.exchange, options, new DefaultStore());
        instance.emitter.on('error', () => errors++);
      } else {
        native = createRequire(import.meta.url)(process.env.LIVE_DIFF_ADDON);
        if (typeof native.nativeAccountReplay !== 'function') throw new Error('Addon needs the #1082 replay clock ABI');
        id = await native.nativeAccountOpen(
          { exchange: message.exchange, options, ...(!message.realtimeClock ? { replayEpochMs: message.epoch } : {}) },
          (event) => {
            if (event.event === 'error') errors++;
          },
          (slices) => Object.assign(store, slices),
        );
        if (!message.realtimeClock) await native.nativeAccountReplay(id, { op: 'begin' });
      }
    } else if (message.method === '$advance') {
      if (implementation === 'ts') {
        await clock.advance(message.args[0]);
        value = { atMs: clock.now() };
      } else value = await native.nativeAccountReplay(id, { op: 'advance', atMs: message.args[0] });
    } else if (message.method === '$receipt') {
      if (implementation !== 'rust') throw new Error('Native diagnostic only');
      value = await native.nativeAccountReplay(id, { op: 'receipt' });
    } else if (message.method === '$describe') {
      if (implementation !== 'ts') throw new Error('Capabilities are taken from the maintained TS API');
      const base = new BaseExchange(instance.name, instance.options, new DefaultStore());
      value = {
        markets: instance.store.markets.map(({ id, symbol, base, quote, active }) => ({
          id,
          symbol,
          base,
          quote,
          active,
        })),
        methods: publicReadMethods.filter((m) => typeof instance[m] === 'function'),
        overrides: publicReadMethods.filter(
          (m) => typeof instance[m] === 'function' && instance[m].toString() !== base[m]?.toString(),
        ),
        tradeOverride: instance.fetchPublicTrades.toString() !== base.fetchPublicTrades.toString(),
      };
      await base.dispose();
    } else if (message.method === '$tickers' || message.method === '$markets') {
      if (implementation === 'rust') await new Promise((r) => realTimeout(r, 350));
      const field = message.method === '$tickers' ? 'tickers' : 'markets';
      value = implementation === 'ts' ? instance.store[field] : store[field];
    } else if (message.method === 'close') {
      if (implementation === 'ts') await instance?.dispose();
      else if (id !== undefined) await native.nativeAccountClose(id);
    } else {
      if (!allowed.has(message.method)) throw new Error('Not an audited public read');
      value =
        implementation === 'ts'
          ? await instance[message.method](...message.args)
          : await native.nativeAccountCallMethod(id, message.method, message.args);
    }
    await new Promise(setImmediate);
    process.send({ requestId: message.requestId, value, errors: errors - before });
  } catch (error) {
    process.send({
      requestId: message.requestId,
      failure: { name: error.name, message: String(error.message).slice(0, 600) },
      errors: errors - before,
    });
  }
});
