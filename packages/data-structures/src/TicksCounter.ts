export type Entry = {
  [key: string]: {
    total: number;
    buys: number;
    sells: number;
  };
};

const EMPTY_ENTRY: Entry = {};

export class TicksCounter {
  private ticksToStore: number;
  private lastSixtySecondsTicks: Entry[] = [];
  private currentSecondTicks: Entry = {
    ...EMPTY_ENTRY,
  };

  constructor(ticksToStore: number = 60) {
    this.ticksToStore = ticksToStore;

    setInterval(() => {
      this.recordCurrentSecond();
    }, 1_000);

    this.recordCurrentSecond();
  }

  recordCurrentSecond() {
    this.lastSixtySecondsTicks.push({ ...this.currentSecondTicks });
    this.currentSecondTicks = {
      ...EMPTY_ENTRY,
    };
    if (this.lastSixtySecondsTicks.length > this.ticksToStore) {
      this.lastSixtySecondsTicks.shift();
    }
  }

  getLastN(seconds: number): Entry {
    const stats: Entry = {};
    let startPointTotal = this.lastSixtySecondsTicks.length - seconds;
    if (this.lastSixtySecondsTicks.length < seconds) {
      startPointTotal = 0;
    }
    for (let i = startPointTotal; i < this.lastSixtySecondsTicks.length; i++) {
      for (const [symbol, trades] of Object.entries(this.lastSixtySecondsTicks[i])) {
        if (!stats[symbol]) {
          stats[symbol] = {
            total: 0,
            buys: 0,
            sells: 0,
          };
        }
        stats[symbol].total += trades.total;
        stats[symbol].buys += trades.buys;
        stats[symbol].sells += trades.sells;
      }
    }
    return stats;
  }

  getTopN(seconds: number, top: number) {
    const result = Object.entries(this.getLastN(seconds));
    return result.sort((a, b) => b[1].total - a[1].total).slice(0, top);
  }

  add(symbol: string, side: 'buy' | 'sell' | unknown) {
    if (!this.currentSecondTicks[symbol]) {
      this.currentSecondTicks[symbol] = {
        buys: 0,
        total: 0,
        sells: 0,
      };
    }
    this.currentSecondTicks[symbol].total++;
    this.currentSecondTicks[symbol][side === 'buy' ? 'buys' : 'sells']++;
  }
}
