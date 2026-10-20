import { FetchedLeverage, PositionMode, PositionSide } from '../../types';
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

  private handleActiveMarginSymbolChange(data: any) {
    // Check if exchange has fetchLeverageOnMarginSymbolChange enabled
    if (!this._metadata.fetchLeverageOnMarginSymbolChange) {
      return;
    }

    const activeMarginSymbol = data?.activeMarginSymbol;
    const existing = this._exchange.store.fetchedLeverage[activeMarginSymbol];
    const isFetching = this._exchange.store.fetchedLeverage[activeMarginSymbol]?.isFetching;

    // Only fetch if we don't already have leverage for this symbol and it's not currently being fetched
    if (activeMarginSymbol && !existing && !isFetching) {
      const placeholder: FetchedLeverage = { buyLeverage: 0, sellLeverage: 0, isFetching: true };
      this._exchange.store.update(
        {
          fetchedLeverage: {
            ...this._exchange.store.fetchedLeverage,
            [activeMarginSymbol]: placeholder,
          },
        },
        true,
      );

      // A frame may confirm leverage while the fetch is in flight; clear only a still-pending
      // placeholder (frame recovery may have copied it, so identity cannot be relied on).
      const clearPlaceholder = () => {
        const { [activeMarginSymbol]: current, ...rest } = this._exchange.store.fetchedLeverage;
        if (current?.isFetching) this._exchange.store.update({ fetchedLeverage: rest }, true);
      };

      this._exchange
        .fetchLeverageForSymbol(activeMarginSymbol)
        .then((leverageObj) => {
          if (leverageObj) {
            this.updatePositionsWithLeverage(activeMarginSymbol, leverageObj);
          } else {
            clearPlaceholder();
          }
        })
        .catch(clearPlaceholder);
    }
  }

  private updatePositionsWithLeverage(symbol: string, leverageObj: FetchedLeverage) {
    // Find all positions for this symbol
    const positions = this._exchange.store.positions.filter((pos) => pos.symbol === symbol);

    if (positions.length === 0) {
      return;
    }

    // Update each position with the appropriate leverage
    const updates: Array<[{ id: string }, { leverage: number }]> = [];

    positions.forEach((pos) => {
      let leverage: number;

      // If position mode is OneWay, use either buy or sell leverage (they should be the same)
      if (pos.positionMode === PositionMode.OneWay) {
        leverage = leverageObj.buyLeverage;
      } else {
        // For hedge mode, use the appropriate leverage based on side
        leverage = pos.side === PositionSide.Long ? leverageObj.buyLeverage : leverageObj.sellLeverage;
      }

      // Only update if leverage actually changed
      if (pos.leverage !== leverage) {
        updates.push([{ id: pos.id }, { leverage }]);
      }
    });

    if (updates.length > 0) {
      this._exchange.store.updatePositions(updates);
    }
  }
}
