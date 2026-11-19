import { serializeCommandError } from '../../commandOutcome';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

const logging = false;

// TODO: simplify to single fetch, set, then migrate this to CallMethodInWebWorker and test
export class UpdatePositionsHandler extends BaseHandler {
  private _lastFailedRequest: number = 0;
  private _requestsInProgress = new Set<string | null>();
  handleUpdatePositions = async (requestId: string, symbols?: string[]) => {
    const now = Date.now();
    if (now - this._lastFailedRequest < 5000) {
      logging && diagnosticConsole.error('Failed to update positions, too soon to make another request');
      this._ctx.postMessage({
        op: 'updatePositions:done',
        data: { requestId, success: false, reason: 'retry_too_soon' },
      });
      return;
    }
    let validSymbols = symbols;
    try {
      if (validSymbols) {
        validSymbols = validSymbols.filter((symbol) => !this._requestsInProgress.has(symbol));
        if (validSymbols.length === 0) {
          logging && diagnosticConsole.warn('Failed to update positions, no valid symbols to update');
          this._ctx.postMessage({ op: 'updatePositions:done', data: { requestId, success: true } });
          return;
        }
        validSymbols.forEach((symbol) => this._requestsInProgress.add(symbol));
      } else {
        if (this._requestsInProgress.has(null)) {
          logging && diagnosticConsole.warn('Failed to update positions, another request is already in progress');
          this._ctx.postMessage({ op: 'updatePositions:done', data: { requestId, success: true } });
          return;
        } else {
          this._requestsInProgress.add(null);
        }
      }

      // fetchPositions applies the native ACCOUNT_READ frame before resolving.
      await this._exchange.fetchPositions(symbols);
      if (this._exchange.isDisposed) return;
      this._ctx.postMessage({ op: 'updatePositions:done', data: { requestId, success: true } });
    } catch (e) {
      diagnosticConsole.error('Error updating positions', e);
      this._lastFailedRequest = now;
      this._ctx.postMessage({
        op: 'updatePositions:done',
        data: {
          requestId,
          success: false,
          reason: String(e),
          ...(e instanceof Error && { errorDetails: serializeCommandError(e) }),
        },
      });
    } finally {
      if (validSymbols) {
        validSymbols.forEach((symbol) => this._requestsInProgress.delete(symbol));
      } else {
        this._requestsInProgress.delete(null);
      }
    }
  };

  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'updatePositions':
        const { symbols, requestId }: { symbols?: string[]; requestId: string } = { ...payload.data };
        this.handleUpdatePositions(requestId, symbols);
        break;
    }
  }
}
