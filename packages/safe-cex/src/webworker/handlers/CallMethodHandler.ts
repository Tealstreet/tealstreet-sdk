import { CallableMethodsInWorker } from '../../exchanges/base';
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

  handleCallMethod = async (requestId: string, method: CallableMethodsInWorker, args: any[]) => {
    let result: any = null;
    if (logging) {
      console.log('Calling method', method);
    }
    try {
      switch (method) {
        case 'setLeverage':
          result = await this._exchange.setLeverage(...(args as Parameters<typeof this._exchange.setLeverage>));
          break;
        case 'scheduleCancel':
          result = await this._exchange.scheduleCancel(...(args as Parameters<typeof this._exchange.scheduleCancel>));
          break;
        case 'updateSymbolMetadata':
          this.symbolMetadataThrottler.call(...(args as Parameters<typeof this._exchange.updateSymbolMetadata>));
          break;
        case 'changeMarginType':
          result = await this._exchange.changeMarginType(
            ...(args as Parameters<typeof this._exchange.changeMarginType>),
          );
          break;
        case 'changePositionMode':
          result = await this._exchange.changePositionMode(
            ...(args as Parameters<typeof this._exchange.changePositionMode>),
          );
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
      this._ctx.postMessage({ op: 'callMethod:done', data: { requestId, result, success: true } });
    } catch (e: any) {
      console.error('Error calling method', e);
      // Serialize error with all properties including custom ones like 'code'
      const serializedError =
        e instanceof Error
          ? {
              message: e.message,
              name: e.name,
              stack: e.stack,
              // Preserve custom properties like 'code' from exchange errors
              ...('code' in e && { code: (e as Error & { code: unknown }).code }),
            }
          : {
              message: String(e),
            };
      this._ctx.postMessage({
        op: 'callMethod:done',
        data: { requestId, success: false, result: serializedError },
      });
    }
  };

  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'callMethod':
        const { requestId, method, args }: { requestId: string; method: CallableMethodsInWorker; args: any[] } = {
          ...payload.data,
        };
        this.handleCallMethod(requestId, method, args);
        break;
    }
  }

  dispose() {
    this.symbolMetadataThrottler.clear();
    super.dispose();
  }
}
