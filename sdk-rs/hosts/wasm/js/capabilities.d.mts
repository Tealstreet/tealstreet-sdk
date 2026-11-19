/** Shared native capability types; keep host declarations independent of SDK store/browser sources. */
export type QuantityUnits = 'base' | 'nativePlacement' | 'inverseNative' | 'contractCount' | 'inverseContractSized';

/** What a venue answers beyond the common commands (`safe_cex_engine::Capabilities`). */
export interface EngineCapabilities {
  account: 'unsupported' | 'query' | 'afterLoad';
  clientOrderLookup: boolean;
  fillHistory: boolean;
  orderHistory: boolean;
  incomeHistory: boolean;
  fillsByOrderId: boolean;
  cancelAll: 'eachOpenSymbol' | 'eachPlacedSymbol' | 'native';
  leverageQuery: 'settings' | 'onRead' | 'onMetadata' | 'onReadAndMetadata';
  maxLeverage: 'market' | 'query' | 'rails' | 'unsupported';
  accountLeverage: boolean;
  accountMarginType: boolean;
  cancelWithSymbols: boolean;
  amendWithSymbol: boolean;
  orderBookQuery: boolean;
  options: boolean;
  quantityUnits: QuantityUnits;
  accountTier: boolean;
  closedPositions: boolean;
  balanceInfo: boolean;
  marketRefresh: boolean;
  positionHistory: 'unsupported' | 'fillsAccount' | 'fillsSymbol' | 'bybit' | 'okx' | 'blofin' | 'wooOrders' | 'bitunix';
  closedPositionFormat: 'unsupported' | 'bybit' | 'okx' | 'blofin' | 'bitunix';
  fillHistoryTime: 'asProvided' | 'secondsOrMilliseconds' | 'nonnegativeSecondsOrMilliseconds';
  stpControls: boolean;
  bybitExecutionControls: boolean;
  rejectClientSlippage: boolean;
  finiteCommandNumbers: boolean;
  finiteHistoryBounds: boolean;
  rejectOrderHistoryFilters: boolean;
  publicCatalogueValidation: boolean;
  isolatedBookSubscriptions: boolean;
  inverseClosedPositions: boolean;
  scheduleCancel: boolean;
  orderHistoryNeedsSymbol: boolean;
  freshPositions: boolean;
  /** The `cancelAccountOrders` op, which only a nuke sends. */
  nukeAccountCancel: boolean;
  /** A symbol cancel can miss an order the store has not seen yet. */
  symbolCancelMissesUnseen: boolean;
  /** The source writes read covers; a frame-fed page lifts fences on them (Rust ingest only). */
  orderReadCovers: boolean;
}
