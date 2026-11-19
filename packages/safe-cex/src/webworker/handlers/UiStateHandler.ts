import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

export class UiStateHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'ui:state':
        this.handleActiveSymbolsChange(payload.data);
        this.handleActiveMarginSymbolChange(payload.data);
        break;
    }
  }

  private handleActiveSymbolsChange(data: any) {
    if (!Array.isArray(data?.activeSymbols)) {
      return;
    }

    const activeSymbols = data.activeSymbols.filter((symbol: unknown) => typeof symbol === 'string' && symbol.trim());
    const existingSymbols = this._exchange.getActiveTickerSymbols?.() || [];
    const mergedSymbols = [...new Set([...existingSymbols, ...activeSymbols])];
    this._exchange.setActiveTickerSymbols(mergedSymbols);
  }

  private readonly fetchingLeverage = new Set<string>();

  private handleActiveMarginSymbolChange(data: { activeMarginSymbol?: string }) {
    if (!this._metadata.fetchLeverageOnMarginSymbolChange) return;
    const symbol = data?.activeMarginSymbol;
    if (!symbol || this._exchange.store.fetchedLeverage[symbol] || this.fetchingLeverage.has(symbol)) return;
    this.fetchingLeverage.add(symbol);
    // The native read publishes SETTINGS. Pending UI requests do not fabricate state.
    void this._exchange.fetchLeverageForSymbol(symbol)
      .catch(() => undefined)
      .finally(() => this.fetchingLeverage.delete(symbol));
  }
}
