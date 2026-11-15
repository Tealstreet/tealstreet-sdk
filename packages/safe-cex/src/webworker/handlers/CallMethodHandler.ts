import type { WorkerMethodName } from '../workerMethodArguments';
import type { WriteMarks } from '../../writeTiming';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { serializeCommandError } from '../../commandOutcome';
import { WORKER_FORWARDED_METHODS, WORKER_OPTIONAL_METHODS } from '../../exchanges/base';
import { encodeWorkerOrderIds } from '../../exchanges/clientOrderId';
import { historyCompletenessOf } from '../../historyCompleteness';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { runWithWriteTiming, WriteTiming } from '../../writeTiming';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { isWorkerWriteMarks } from '../workerMessage';
import { BaseHandler } from './BaseHandler';
import { ThrottleManager } from './ThrottleManager';

const logging = false;
const forwardedMethods: ReadonlySet<string> = new Set([...WORKER_FORWARDED_METHODS, ...WORKER_OPTIONAL_METHODS]);

export class CallMethodHandler extends BaseHandler {
  private symbolMetadataThrottler: ThrottleManager<typeof this._exchange.updateSymbolMetadata>;
  private readonly THROTTLE_DELAY = 500; // 0.5 second throttle window

  constructor(...args: ConstructorParameters<typeof BaseHandler>) {
    super(...args);
    this.symbolMetadataThrottler = new ThrottleManager(
      (...args) => this._exchange?.updateSymbolMetadata(...args),
      this.THROTTLE_DELAY,
      (symbol: string) => symbol, // Key generator function
    );
  }

  private replySuccess = (requestId: string, result: unknown, timing?: WriteTiming) => {
    const completeness = historyCompletenessOf(result);
    this._ctx.postMessage({
      op: 'callMethod:done',
      data: {
        requestId,
        result,
        success: true,
        ...(completeness ? { completeness } : {}),
        ...(timing ? { timing: timing.marks } : {}),
      },
    });
  };

  private unsupportedMethod = (method: string) => {
    const message = `${this._exchange.name} does not support ${method}`;
    return new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
  };

  handleCallMethod = (
    requestId: string,
    method: WorkerMethodName,
    args: unknown[],
    marks?: WriteMarks,
  ) => this.dispatchCallMethod(requestId, method, args, marks);

  private dispatchCallMethod = async (
    requestId: string,
    method: string,
    args: unknown[],
    marks?: WriteMarks,
  ) => {
    let result: unknown = null;
    // The caller's write timing: the exchange stamps the engine stages and the caller emits.
    const timing = marks ? new WriteTiming(method, { ...marks }) : undefined;
    timing?.mark('received');
    if (logging) {
      diagnosticConsole.log('Calling method', method);
    }
    try {
      // Mirrors forward every metadata refresh; the worker keeps coalescing them.
      if (method === 'updateSymbolMetadata') {
        Reflect.apply(this.symbolMetadataThrottler.call, this.symbolMetadataThrottler, args);
        this.replySuccess(requestId, null);
        return;
      }
      if (forwardedMethods.has(method)) {
        const call: unknown = Reflect.get(this._exchange, method);
        if (typeof call !== 'function') throw this.unsupportedMethod(method);
        const start = () => Reflect.apply(call, undefined, args);
        result = await (timing ? runWithWriteTiming(timing, start) : start());
        if (method === 'updateOrder') result = encodeWorkerOrderIds(result as string[]);
        this.replySuccess(requestId, result, timing);
        return;
      }
      switch (method) {
        case 'scheduleCancel':
          result = await Reflect.apply(this._exchange.scheduleCancel, this._exchange, args);
          break;
        case 'fetchLastLogsFromWebsockets':
          result = this._exchange.fetchLastLogsFromWebsockets();
          break;
        case 'fetchAndSetPrivateSnapshot':
          result = await this._exchange.fetchAndSetPrivateSnapshot();
          break;
        case 'fetchEngineSnapshot':
          result = await this._exchange.fetchEngineSnapshot();
          break;
        case 'reconnect':
          this._exchange.reconnect();
          break;
        case 'recoverAfterOffline':
          result = this._exchange.recoverAfterOffline();
          break;
        case 'fetchOHLCV':
          result = await Reflect.apply(this._exchange.fetchOHLCV, this._exchange, args);
          break;
        case 'fetchHistoricalFills':
          result = await Reflect.apply(this._exchange.fetchHistoricalFills, this._exchange, args);
          break;
        case 'fetchOrders':
          result = await Reflect.apply(this._exchange.fetchOrders, this._exchange, args);
          break;
        case 'fetchAndSetOrders':
          result = await Reflect.apply(this._exchange.fetchAndSetOrders, this._exchange, args);
          break;
        case 'removeOrders':
          await Reflect.apply(this._exchange.removeOrders, this._exchange, args);
          break;
        case 'fetchHistoricalOrders':
          // Without a case here the handler falls through to `default` and returns
          // success:false, so the method exists but is unreachable from the web app.
          result = await Reflect.apply(this._exchange.fetchHistoricalOrders, this._exchange, args);
          break;
        case 'fetchClosedPnl':
          result = await Reflect.apply(this._exchange.fetchClosedPnl, this._exchange, args);
          break;
        case 'fetchIncomeHistory':
          result = await Reflect.apply(this._exchange.fetchIncomeHistory, this._exchange, args);
          break;
        case 'fetchIncomeHistoryRange':
          result = await Reflect.apply(this._exchange.fetchIncomeHistoryRange, this._exchange, args);
          break;
        default:
          throw this.unsupportedMethod(method);
      }
      if (logging) {
        diagnosticConsole.log('Method called', method, result);
      }
      this.replySuccess(requestId, result);
    } catch (e: unknown) {
      diagnosticConsole.error('Error calling method', e);
      const serializedError = serializeCommandError(e);
      this._ctx.postMessage({
        op: 'callMethod:done',
        data: { requestId, success: false, result: serializedError, ...(timing ? { timing: timing.marks } : {}) },
      });
    }
  };

  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'callMethod':
        const data: unknown = payload.data;
        if (!data || typeof data !== 'object' || !('requestId' in data) || typeof data.requestId !== 'string') return;
        if (!('method' in data) || typeof data.method !== 'string' || !('args' in data) || !Array.isArray(data.args)) {
          const error = new ExchangeError('Invalid worker method envelope', {
            message: 'Invalid worker method envelope', kind: 'unsupported', outcome: 'notSent',
          });
          this._ctx.postMessage({ op: 'callMethod:done', data: {
            requestId: data.requestId, success: false, result: serializeCommandError(error),
          } });
          return;
        }
        // Arguments stay unknown at the wire boundary; venue validators retain their authority.
        const marks = 'timing' in data && isWorkerWriteMarks(data.timing) ? data.timing : undefined;
        this.dispatchCallMethod(data.requestId, data.method, data.args, marks);
        break;
    }
  }

  dispose() {
    this.symbolMetadataThrottler.clear();
    super.dispose();
  }
}
