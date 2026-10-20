import isEqual from 'lodash/isEqual';

import { serializeCommandError } from '../../commandOutcome';
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
      logging && console.error('Failed to update positions, too soon to make another request');
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
          logging && console.warn('Failed to update positions, no valid symbols to update');
          this._ctx.postMessage({ op: 'updatePositions:done', data: { requestId, success: true } });
          return;
        }
        validSymbols.forEach((symbol) => this._requestsInProgress.add(symbol));
      } else {
        if (this._requestsInProgress.has(null)) {
          logging && console.warn('Failed to update positions, another request is already in progress');
          this._ctx.postMessage({ op: 'updatePositions:done', data: { requestId, success: true } });
          return;
        } else {
          this._requestsInProgress.add(null);
        }
      }

      const isFullAccountRefresh = validSymbols === undefined;
      logging && console.log('Updating positions', symbols);
      const positions = await this._exchange.fetchPositions(symbols);
      if (this._exchange.isDisposed) return;
      if (isEqual(positions, this._exchange.store.positions)) {
        // Data unchanged - this is a success, positions are up-to-date
        logging && console.log('Positions unchanged, data is current');
        this._ctx.postMessage({ op: 'updatePositions:done', data: { requestId, success: true } });
      } else {
        if (!this._exchange.isDisposed) {
          if (isFullAccountRefresh) {
            this._exchange.store.addOrUpdatePositionsFull(positions);
          } else if (positions.length > 0) {
            this._exchange.store.addOrUpdatePositions(positions);
          }
        }
        logging && console.log('Updated positions', positions);
        logging && console.log('Full positions', this._exchange.store.positions);
        this._ctx.postMessage({ op: 'updatePositions:done', data: { requestId, success: true } });
      }
    } catch (e) {
      console.error('Error updating positions', e);
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
