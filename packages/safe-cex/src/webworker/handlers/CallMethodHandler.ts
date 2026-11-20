import type { WorkerMethodName } from '../workerMethodArguments';
import type { WriteMarks } from '../../writeTiming';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { serializeCommandError } from '../../commandOutcome';
import { WORKER_FORWARDED_METHODS, WORKER_OPTIONAL_METHODS } from '../../exchanges/base';
import { encodeWorkerOrderIds } from '../../exchanges/clientOrderId';
import { historyCompletenessOf } from '../../historyCompleteness';
import { capturePlacementIntent, runWithPlacementIntent, PLACEMENT_INTENT_METHODS } from '../../placementIntentContext';
import { PlacementIntentError, PlacementIntentLedger } from '../../placementIntentLedger';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { runWithWriteTiming, WriteTiming } from '../../writeTiming';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { isWorkerWriteMarks } from '../workerMessage';
import { captureTwapOwner } from '../twap/TwapOwner';
import { BaseHandler } from './BaseHandler';
import { ThrottleManager } from './ThrottleManager';

const logging = false;
const forwardedMethods: ReadonlySet<string> = new Set([...WORKER_FORWARDED_METHODS, ...WORKER_OPTIONAL_METHODS]);

type MethodReply = { success: boolean; result: unknown; timing?: WriteMarks; completeness?: ReturnType<typeof historyCompletenessOf> };

export class CallMethodHandler extends BaseHandler {
  private active = true;
  private readonly placementReplies = new PlacementIntentLedger<MethodReply>(1024, (reply) => reply.success);
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

  private success = (result: unknown, timing?: WriteTiming): MethodReply => {
    const completeness = historyCompletenessOf(result);
    return { result, success: true, ...(completeness ? { completeness } : {}),
      ...(timing ? { timing: timing.marks } : {}) };
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
    intentId?: string,
  ) => this.dispatchCallMethod(requestId, method, args, marks, intentId);

  private dispatchCallMethod = async (
    requestId: string, method: string, args: unknown[], marks?: WriteMarks, intentId?: unknown,
  ): Promise<void> => {
    if (!PLACEMENT_INTENT_METHODS.has(method) || intentId === undefined) {
      await this.executeCallMethod(method, args, marks,
        (reply) => this._ctx.postMessage({ op: 'callMethod:done', data: { requestId, ...reply } }));
      return;
    }
    let reply: MethodReply;
    try {
      if (!this.active) throw this.unsupportedMethod(method);
      if (typeof intentId !== 'string') throw new PlacementIntentError('invalidIntentId');
      reply = await runWithPlacementIntent(intentId, () => {
        const intent = capturePlacementIntent();
        if (!intent) throw new PlacementIntentError('invalidIntentId');
        const snapshot: unknown[] = structuredClone(args);
        return this.placementReplies.run(intent, JSON.stringify([captureTwapOwner(this._ctx.getTwapOwner), method, snapshot]),
          () => runWithPlacementIntent(intent.id, () => this.executeCallMethod(method, snapshot, marks)));
      });
    } catch (error) {
      reply = { success: false, result: serializeCommandError(error) };
    }
    this._ctx.postMessage({ op: 'callMethod:done', data: { requestId, ...reply } });
  };

  private executeCallMethod = async (
    method: string, args: unknown[], marks?: WriteMarks, respond?: (reply: MethodReply) => void,
  ): Promise<MethodReply> => {
    const finish = (reply: MethodReply): MethodReply => { respond?.(reply); return reply; };
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
        return finish(this.success(null));
      }
      if (forwardedMethods.has(method)) {
        const call: unknown = Reflect.get(this._exchange, method);
        if (typeof call !== 'function') throw this.unsupportedMethod(method);
        const start = () => Reflect.apply(call, undefined, args);
        result = await (timing ? runWithWriteTiming(timing, start) : start());
        if (method === 'updateOrder') result = encodeWorkerOrderIds(result as string[]);
        return finish(this.success(result, timing));
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
      return finish(this.success(result));
    } catch (e: unknown) {
      diagnosticConsole.error('Error calling method', e);
      const serializedError = serializeCommandError(e);
      return finish({ success: false, result: serializedError, ...(timing ? { timing: timing.marks } : {}) });
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
        void this.dispatchCallMethod(data.requestId, data.method, data.args, marks, 'intentId' in data ? data.intentId : undefined);
        break;
    }
  }

  dispose() {
    this.active = false;
    this.symbolMetadataThrottler.clear();
    super.dispose();
  }
}
