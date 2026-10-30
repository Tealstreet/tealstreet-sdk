import { serializeCommandError } from '../../commandOutcome';
import { historyCompletenessOf } from '../../historyCompleteness';
import { CallableMethodsInWorker, WORKER_FORWARDED_METHODS, WORKER_OPTIONAL_METHODS } from '../../exchanges/base';
import { encodeWorkerOrderIds } from '../../exchanges/clientOrderId';
import { runWithWriteTiming, WriteTiming, type WriteMarks } from '../../writeTiming';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';
import { ThrottleManager } from './ThrottleManager';

const logging = false;

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

  handleCallMethod = async (requestId: string, method: CallableMethodsInWorker, args: any[], marks?: WriteMarks) => {
    let result: any = null;
    // The caller's write timing: the exchange stamps the engine stages and the caller emits.
    const timing = marks ? new WriteTiming(method, { ...marks }) : undefined;
    timing?.mark('received');
    if (logging) {
      console.log('Calling method', method);
    }
    try {
      // Mirrors forward every metadata refresh; the worker keeps coalescing them.
      if (method === 'updateSymbolMetadata') {
        this.symbolMetadataThrottler.call(...(args as Parameters<typeof this._exchange.updateSymbolMetadata>));
        this.replySuccess(requestId, null);
        return;
      }
      if ([...WORKER_FORWARDED_METHODS, ...WORKER_OPTIONAL_METHODS].includes(method as never)) {
        const call = (this._exchange as unknown as Record<string, unknown>)[method];
        if (typeof call !== 'function') throw new Error(`${this._exchange.name} does not support ${method}`);
        const start = () => (call as (...callArgs: unknown[]) => Promise<unknown>)(...args);
        result = await (timing ? runWithWriteTiming(timing, start) : start());
        if (method === 'updateOrder') result = encodeWorkerOrderIds(result as string[]);
        this.replySuccess(requestId, result, timing);
        return;
      }
      switch (method) {
        case 'scheduleCancel':
          result = await this._exchange.scheduleCancel(...(args as Parameters<typeof this._exchange.scheduleCancel>));
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
          result = await this._exchange.fetchOHLCV(...(args as Parameters<typeof this._exchange.fetchOHLCV>));
          break;
        case 'fetchHistoricalFills':
          result = await this._exchange.fetchHistoricalFills(
            ...(args as Parameters<typeof this._exchange.fetchHistoricalFills>),
          );
          break;
        case 'fetchOrders':
          result = await this._exchange.fetchOrders(...(args as Parameters<typeof this._exchange.fetchOrders>));
          break;
        case 'fetchAndSetOrders':
          result = await this._exchange.fetchAndSetOrders(
            ...(args as Parameters<typeof this._exchange.fetchAndSetOrders>),
          );
          break;
        case 'removeOrders':
          await this._exchange.removeOrders(...(args as Parameters<typeof this._exchange.removeOrders>));
          break;
        case 'fetchHistoricalOrders':
          // Without a case here the handler falls through to `default` and returns
          // success:false, so the method exists but is unreachable from the web app.
          result = await this._exchange.fetchHistoricalOrders(
            ...(args as Parameters<typeof this._exchange.fetchHistoricalOrders>),
          );
          break;
        case 'fetchClosedPnl':
          result = await this._exchange.fetchClosedPnl(...(args as Parameters<typeof this._exchange.fetchClosedPnl>));
          break;
        case 'fetchIncomeHistory':
          result = await this._exchange.fetchIncomeHistory(
            ...(args as Parameters<typeof this._exchange.fetchIncomeHistory>),
          );
          break;
        case 'fetchIncomeHistoryRange':
          result = await this._exchange.fetchIncomeHistoryRange(
            ...(args as Parameters<typeof this._exchange.fetchIncomeHistoryRange>),
          );
          break;
        default:
          console.error('Unknown method', method);
          this._ctx.postMessage({ op: 'callMethod:done', data: { requestId, success: false } });
          return;
      }
      if (logging) {
        console.log('Method called', method, result);
      }
      this.replySuccess(requestId, result);
    } catch (e: any) {
      console.error('Error calling method', e);
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
        const {
          requestId,
          method,
          args,
          timing,
        }: { requestId: string; method: CallableMethodsInWorker; args: any[]; timing?: WriteMarks } = {
          ...payload.data,
        };
        this.handleCallMethod(requestId, method, args, timing);
        break;
    }
  }

  dispose() {
    this.symbolMetadataThrottler.clear();
    super.dispose();
  }
}
