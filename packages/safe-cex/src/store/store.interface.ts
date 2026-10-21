import type {
  Market,
  Order,
  OrderRiskProfile,
  OrderStatus,
  Position,
  RequestOutcome,
  StoreData,
  Ticker,
} from '../types';

export type PackedTickerDelta = [index: number, mask: number, ...values: number[]];
export type StoreUpdate = Partial<StoreData> & {
  tickerDeltas?: PackedTickerDelta[];
};

export interface Store {
  update: (changes: StoreUpdate, fromWorker?: boolean, source?: 'account-read') => void;
  reset: () => void;
  subscribe: (cb: (data: StoreData, fromWorker?: boolean) => void) => () => void;

  setSetting: (key: 'isHedged', value: boolean) => void;

  removeOrder: (order: Pick<Order, 'id'>) => void;
  updateOrderStatus: (order: Pick<Order, 'id'>, newStatus: OrderStatus) => void;
  removeOrders: (ids: Array<Pick<Order, 'id'>>) => void;
  updateOrdersStatus: (ids: Array<Pick<Order, 'id'>>, newStatus: OrderStatus) => void;
  /**
   * Like updateOrdersStatus, for a venue event that is FINAL (filled, cancelled):
   * for a while after, no snapshot or upsert may reopen these ids.
   */
  markOrdersTerminal: (ids: Array<Pick<Order, 'id'>>, status: OrderStatus.Closed | OrderStatus.Canceled) => void;
  updateOrder: (id: Pick<Order, 'id'>, changes: Partial<Order>) => void;

  updatePosition: (position: Pick<Position, 'id'>, changes: Partial<Position>) => void;
  updatePositions: (updates: Array<[Pick<Position, 'id'>, Partial<Position>]>) => void;
  addOrUpdatePosition: (position: Position) => void;
  addOrUpdatePositions: (positions: Position[]) => void;
  addOrUpdatePositionsFull: (positions: Position[]) => void;

  addOrder: (order: Order) => void;
  addOrUpdateOrder: (order: Order) => void;
  addOrUpdateOrders: (orders: Order[]) => void;
  addOrUpdateOrdersFull: (orders: Order[]) => void;
  setOrdersForSymbol: (symbol: string, orders: Order[]) => void;

  updateTicker: (ticker: Pick<Ticker, 'id'>, changes: Partial<Ticker>) => void;
  applyPackedTickerDeltas: (deltas: PackedTickerDelta[], fromWorker?: boolean) => void;

  updateMarket: (market: Pick<Market, 'id'>, changes: Partial<Market>) => void;

  updateStarkConfig(data: StoreData['starkConfig']): void;

  updateZkLinkConfig(data: StoreData['zkLinkConfig']): void;

  updateUserData(data: StoreData['userData']): void;

  get latency(): StoreData['latency'];

  get balance(): StoreData['balance'];

  get markets(): StoreData['markets'];

  get tickers(): StoreData['tickers'];

  get orders(): StoreData['orders'];

  get ordersRisk(): Record<string, OrderRiskProfile[]>;

  get positions(): StoreData['positions'];

  get displayedPositions(): StoreData['displayedPositions'];

  get displayedMargin(): StoreData['displayedMargin'];

  get activeChasers(): StoreData['activeChasers'];

  get activeTWAPs(): StoreData['activeTWAPs'];

  get loaded(): StoreData['loaded'];

  get updateIds(): StoreData['updateIds'];

  get options(): StoreData['options'];

  get messageHandlerStats(): StoreData['messageHandlerStats'];

  get starkConfig(): StoreData['starkConfig'];

  get zkLinkConfig(): StoreData['zkLinkConfig'];

  get userData(): StoreData['userData'];

  get tickersById(): Record<string, Ticker>;

  get marketsById(): Record<string, Market>;

  get tickersBySymbol(): Record<string, Ticker>;

  get marketsBySymbol(): Record<string, Market>;

  getMarketsVersion(): number;

  getTickersVersion(): number;

  get fetchedLeverage(): StoreData['fetchedLeverage'];

  get socketHealth(): StoreData['socketHealth'];

  updateSocketHealth(data: Partial<NonNullable<StoreData['socketHealth']>>): void;

  get requestStats(): StoreData['requestStats'];

  /**
   * Increment one REST outcome counter. Runs on EVERY request, so it must stay
   * O(1) and must NOT notify — the throttled store ship already carries the
   * field, and notifying here would manufacture a store delta per request.
   */
  recordRequestOutcome(outcome: RequestOutcome): void;

  serialize(): StoreData;
}
