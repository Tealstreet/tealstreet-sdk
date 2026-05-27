import { RingBuffer } from './RingBuffer';

interface ExchangeSymbol {
  symbol: string;
  [key: string]: any;
}

export class RingBufferTrackingSymbolUpdates extends RingBuffer<ExchangeSymbol> {
  private newUpdatesBySymbol: { [symbol: string]: number } = {};
  private clearUpdatesBySymbol: { [symbol: string]: boolean } = {};

  public getLimit(symbol?: string, limit?: number) {
    if (symbol === undefined) {
      symbol = 'all';
    }
    this.clearUpdatesBySymbol[symbol] = true;
    if (limit === undefined) {
      return this.newUpdatesBySymbol[symbol];
    } else if (this.newUpdatesBySymbol[symbol] === undefined) {
      return limit;
    } else {
      return this.getSize();
    }
  }

  public add(...items: ExchangeSymbol[]): void {
    for (const item of items) {
      if (this.clearUpdatesBySymbol[item.symbol]) {
        this.clearUpdatesBySymbol[item.symbol] = false;
        this.newUpdatesBySymbol[item.symbol] = 0;
      }
      if (this.clearUpdatesBySymbol['all']) {
        this.clearUpdatesBySymbol['all'] = false;
        this.newUpdatesBySymbol['all'] = 0;
      }
    }
    for (const item of items) {
      this.newUpdatesBySymbol[item.symbol] = (this.newUpdatesBySymbol[item.symbol] || 0) + 1;
      this.newUpdatesBySymbol['all'] = (this.newUpdatesBySymbol['all'] || 0) + 1;
    }
    super.add(...items);
  }

  public filterBySinceLimit(
    since: number | undefined = undefined,
    limit: number | undefined = undefined,
    key = 'timestamp',
    tail = false,
  ) {
    const sinceIsDefined = since !== undefined && since !== null;
    if (sinceIsDefined) {
      let array = this.toArray().filter((entry) => entry[key] >= since);
      if (limit !== undefined && limit !== null) {
        array = tail ? array.slice(-limit) : array.slice(0, limit);
      }
      return array;
    }
    if (limit !== undefined && limit !== null) {
      return tail ? this.getLastN(limit) : this.getFirstN(limit);
    }
    return this.toArray();
  }
}
