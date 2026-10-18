import { createRequire } from 'node:module';
import { exchangeRegistry } from '../../../packages/safe-cex/src/exchanges/registry.ts';
import { DefaultStore } from '../../../packages/safe-cex/src/store/store.base.ts';
import { virtualClock } from '../../../packages/safe-cex/src/utils/virtualClock.ts';
import { signBinanceRequest } from '../../../packages/safe-cex/src/exchanges/binance/binance.signing.ts';

// Venue error paths sometimes console.log entire Axios configs. Forward only
// the typed outcome and event counts through IPC, never library console output.
console.log = console.warn = console.error = () => {};
let instance, native, id, signerTime, options;
let store = {};
let errors = 0;
const implementation = process.argv[2];
virtualClock.getCurrentTimeMs = () => signerTime ?? Date.now();
const sign = async (request) => {
  if (request.exchangeName !== 'binance') throw new Error('paired remote signing currently supports Binance USD-M');
  const config = await signBinanceRequest(request.config, { ...options, remoteSigner: undefined }, 'binance', 60000);
  return { id: request.id, config: { params: config.params, data: config.data, headers: config.headers } };
};
process.on('message', async (message) => {
  await new Promise(setImmediate);
  const initialErrors = errors;
  signerTime = message.time;
  try {
    let value;
    if (message.method === 'open') {
      options = message.options;
      const pairedSign = options.key && message.exchange === 'binance';
      if (implementation === 'ts') {
        const Exchange = await exchangeRegistry[message.exchange]();
        instance = new Exchange(
          message.exchange,
          {
            ...options,
            ...(pairedSign
              ? {
                  secret: '',
                  remoteSigner: {
                    sign,
                    isAvailable: () => true,
                    onAvailabilityChange: () => () => {},
                  },
                }
              : {}),
          },
          new DefaultStore(),
        );
        instance.emitter.on('error', () => errors++);
      } else {
        native = createRequire(import.meta.url)(
          process.env.LIVE_DIFF_ADDON || '../../safe-cex-ffi/dist/node/safe_cex_ffi.node',
        );
        id = await native.nativeAccountOpen(
          { exchange: message.exchange, options: { ...options, ...(pairedSign ? { secret: '' } : {}) } },
          (event) => {
            if (event.event === 'error') errors++;
          },
          (slices) => Object.assign(store, slices),
          pairedSign ? sign : undefined,
        );
      }
    } else if (['$startPublic', '$startPrivate', '$startPublicWS'].includes(message.method)) {
      if (message.method !== '$startPrivate' && !options.publicDataOnly)
        throw new Error('Public startup requires a public-only account');
      if (implementation === 'ts') {
        if (message.method !== '$startPublicWS') instance.disablePublicWebsockets();
        await instance.start();
      } else {
        if (message.method !== '$startPublicWS')
          await native.nativeAccountCallMethod(id, 'disablePublicWebsockets', []);
        const started = await native.nativeAccountCallMethod(id, 'start', []);
        if (!started) throw new Error('Native public startup failed');
        for (let count = 0; count < 100 && !store.markets?.length; count++) {
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
      }
      value = null;
    } else if (message.method === '$waitLiveTickers') {
      const [minimum = 1, deadlineMs = 20000] = message.args;
      const deadline = Date.now() + deadlineMs;
      let updated = 0;
      do {
        const tickers = implementation === 'ts' ? instance.store.tickers : store.tickers;
        updated = (tickers ?? []).filter((ticker) =>
          ['last', 'bid', 'ask', 'mark'].some((field) => Number.isFinite(ticker[field]) && ticker[field] > 0),
        ).length;
        if (updated >= minimum) break;
        await new Promise((resolve) => setTimeout(resolve, 100));
      } while (Date.now() < deadline);
      value = { updated };
    } else if (message.method === '$settle') {
      // A JS sleep can observe a partially processed native message. Wait for
      // the native LocalSet to finish its current synchronous venue handler.
      if (implementation === 'rust') await native.nativeAccountCallMethod(id, 'getActiveTickerSymbols', []);
      await new Promise((resolve) => setTimeout(resolve, message.args[0] ?? 500));
      value = null;
    } else if (message.method === '$markets' || message.method === '$tickers' || message.method === '$balance') {
      const field = message.method.slice(1);
      if (implementation === 'rust' && field === 'balance') {
        await new Promise((resolve) => setTimeout(resolve, 300));
      }
      value = implementation === 'ts' ? instance.store[field] : store[field];
    } else if (message.method === 'close') {
      if (implementation === 'ts') await instance?.dispose();
      else if (id !== undefined) await native.nativeAccountClose(id);
    } else {
      value =
        implementation === 'ts'
          ? await instance[message.method](...message.args)
          : await native.nativeAccountCallMethod(id, message.method, message.args);
    }
    await new Promise(setImmediate);
    process.send({ requestId: message.requestId, value, errors: errors - initialErrors });
  } catch (error) {
    process.send({
      requestId: message.requestId,
      failure: {
        name: error.name,
        // Forward only the error summary; the parent removes credential values.
        message: String(error.message).slice(0, 240),
      },
      errors: errors - initialErrors,
    });
  }
});
