import type { Frame, QuantityUnits } from '@tealstreet/safe-cex-wasm/wire';
import type {
  Candle,
  FetchedLeverage,
  Market,
  Order,
  OrderBook,
  OrderFillEvent,
  Position,
  Ticker,
  Writable,
} from '../types';
import type { PackedTickerDelta, Store, StoreUpdate } from './store.interface';

import {
  LOADED_FLAG,
  MARKET_FLAG,
  marketQuantityUnits,
  orderEnums,
  positionEnums,
  TICKER_FIELDS,
} from '@tealstreet/safe-cex-wasm/wire';
import { MarginType, OrderSide, PositionMode } from '../types';
import { divide, subtract } from '../utils/safe-math';

export type FrameBook = { symbol: string; orderbook: OrderBook };
type BookLevels = Map<number, number> | Float64Array;
type BookState = { id: string; bids: BookLevels; asks: BookLevels };
export type FrameCandle = { symbol: string; interval: string; candle: Candle };

export class FrameApplyError extends Error {}

const finite = (value: number) => (Number.isFinite(value) ? value : undefined);
/**
 * Display-only position numbers read a non-finite engine value as the venue's 0 (none/unknown).
 * Sizing inputs (`contracts`, `notional`) stay raw: a NaN there makes nuke refuse the close.
 */
const displayNumber = (value: number) => (Number.isFinite(value) ? value : 0);

/** The sizing rules each `QuantityUnits` convention implies for order and position extras. */
type UnitProfile = {
  nativePlacementAmounts: boolean;
  inverseNativeAmounts: boolean;
  contractCountAmounts: boolean;
  inverseContractSized: boolean;
};
const unitProfile = (units: QuantityUnits): UnitProfile =>
  Object.freeze({
    nativePlacementAmounts: units === 'nativePlacement',
    inverseNativeAmounts: units === 'inverseNative',
    contractCountAmounts: units === 'contractCount',
    inverseContractSized: units === 'inverseContractSized',
  });
const UNIT_PROFILES: Record<QuantityUnits, UnitProfile> = {
  base: unitProfile('base'),
  nativePlacement: unitProfile('nativePlacement'),
  inverseNative: unitProfile('inverseNative'),
  contractCount: unitProfile('contractCount'),
  inverseContractSized: unitProfile('inverseContractSized'),
};

/**
 * Applies the Rust engine's flush frames to a `DefaultStore`, so every reader
 * of the store keeps working: one `store.update` per frame, tickers patched in
 * place from deltas, everything else replaced by section. Frame row indices
 * refer to the latest `MARKETS` section, so the applier keeps that table.
 */
export class FrameApplier {
  private marketSymbols: string[] = [];
  private marketIds: string[] = [];
  private bookStates = new Map<string, BookState>();
  private readonly frameBooks = new WeakMap<Frame, FrameBook[]>();
  private readonly frameMarkets = new WeakMap<Frame, { symbols: string[]; ids: string[] }>();
  private contractSizes: number[] = [];
  private readonly invalidMarketFrames = new WeakSet<Frame>();
  private marketsNeedSnapshot = false;
  private appliedSections = 0;

  /** STATUS flags cannot prove that a section survived frame decoding. */
  get hasMarketIdentity(): boolean {
    return !this.marketsNeedSnapshot && (this.appliedSections & LOADED_FLAG.MARKETS) !== 0;
  }

  get hasAccountState(): boolean {
    const required = LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS;
    return (this.appliedSections & required) === required;
  }

  get hasTickerState(): boolean {
    return (this.appliedSections & LOADED_FLAG.TICKERS) !== 0;
  }

  // Each market's quantity convention, from its MARKETS flags; the venue's
  // convention survives an empty table for rows outside it.
  private marketUnits: QuantityUnits[] = [];
  private venueUnits: QuantityUnits = 'base';

  constructor(private readonly store: Store) {}

  /** Forwarding workers skip book reconstruction before transferring the frame. */
  apply(frame: Frame, applyBooks = true): void {
    const update: Writable<StoreUpdate> = {};

    const errors: unknown[] = [];
    const attempt = (read: () => void, section = 0) => {
      try {
        read();
      } catch (error) {
        this.appliedSections &= ~section;
        errors.push(error);
      }
    };
    let markets: ReturnType<Frame['markets']>;
    let tickers: ReturnType<Frame['tickers']>;
    let symbols = this.marketSymbols;
    let ids = this.marketIds;
    let contractSizes = this.contractSizes;
    let marketUnits = this.marketUnits;
    // Rows outside the market table keep the venue's convention, which every market shares.
    const unitsOf = (market: number) => UNIT_PROFILES[marketUnits[market] ?? marketUnits[0] ?? this.venueUnits];
    const resolveSymbol = (market: number, fallback?: string) => symbols[market] ?? fallback ?? '';
    attempt(() => {
      markets = frame.markets();
    }, LOADED_FLAG.MARKETS);
    attempt(() => {
      if (markets) {
        const predictionMarkets = frame.predictionMarkets?.();
        const rows: Writable<Market>[] = [];
        const nextSymbols: string[] = new Array(markets.count);
        const nextContractSizes = Array.from(markets.contractSize, (size) => (size > 0 ? size : 1));
        const nextUnits = Array.from(markets.flags, marketQuantityUnits);
        for (let i = 0; i < markets.count; i++) {
          const flags = markets.flags[i];
          const flag = (bit: number) => ((flags & bit) !== 0 ? true : undefined);
          const minPrice = finite(markets.minPrice[i]);
          rows.push({
            id: markets.id(i),
            symbol: markets.symbol(i),
            ...(markets.prettySymbol?.(i) !== undefined
              ? { prettySymbol: markets.prettySymbol!(i) }
              : nextUnits[i] === 'contractCount' &&
                !markets.id(i).endsWith('-SWAP') && {
                  prettySymbol: `${markets.base(i)}/${markets.quote(i)} XPERP`,
                }),
            base: markets.base(i),
            quote: markets.quote(i),
            active: (flags & MARKET_FLAG.ACTIVE) !== 0,
            precision: { amount: markets.precisionAmount[i], price: markets.precisionPrice[i] },
            limits: {
              amount: { min: markets.minAmount[i], max: markets.maxAmount[i] },
              leverage: { min: markets.minLeverage[i], max: markets.maxLeverage[i] },
              ...(minPrice !== undefined && { price: { min: minPrice } }),
            },
            contractSize: finite(markets.contractSize[i]),
            isInverse: flag(MARKET_FLAG.INVERSE),
            isSpot: flag(MARKET_FLAG.SPOT),
            isPerpetual: flag(MARKET_FLAG.PERPETUAL),
            isExpiring: flag(MARKET_FLAG.EXPIRING),
            isReadOnly: flag(MARKET_FLAG.READ_ONLY),
            takerFee: finite(markets.takerFee[i]),
            makerFee: finite(markets.makerFee[i]),
          });
          nextSymbols[i] = rows[i].symbol;
        }
        if (predictionMarkets) {
          for (let i = 0; i < predictionMarkets.count; i++) {
            const market = rows[predictionMarkets.market[i]];
            if (!market) continue;
            market.info = {
              conditionId: predictionMarkets.conditionId(i),
              tokenId: market.id,
              outcome: predictionMarkets.outcome(i),
              tickSize: predictionMarkets.tickSize[i],
              negRisk: (predictionMarkets.flags[i] & 1) !== 0,
              question: predictionMarkets.question(i),
              description: predictionMarkets.description(i),
              endDate: predictionMarkets.endDate(i),
              slug: predictionMarkets.slug(i),
            };
          }
        }
        const classifications = frame.instrumentClassifications?.();
        if (classifications) {
          for (let i = 0; i < classifications.count; i++) {
            const market = rows[classifications.market[i]];
            if (!market) throw new Error('Instrument classification has no market');
            market.info = {
              ...market.info,
              symbolType: classifications.symbolType(i),
              marketRegion: classifications.marketRegion(i),
              underlyingTicker: classifications.underlyingTicker(i),
            };
          }
        }
        // TS Hyperliquid's merged spot disclosure: the wrapped token behind the pair.
        const underlying = frame.underlyingTokens?.();
        if (underlying) {
          for (let i = 0; i < underlying.count; i++) {
            const market = rows[underlying.market[i]];
            if (!market) throw new Error('Underlying token has no market');
            market.info = {
              ...market.info,
              hlTokenName: underlying.name(i),
              hlTokenFullName: underlying.fullName(i),
            };
          }
        }
        const products = frame.derivativeProducts?.();
        if (products) {
          for (let i = 0; i < products.count; i++) {
            const market = rows[products.market[i]];
            if (!market) throw new Error('Derivative product has no market');
            const kind = products.kind[i];
            const expiry = finite(products.expiry[i]);
            const strike = finite(products.strike[i]);
            const style = products.style(i);
            const settlement = products.settlement(i);
            const settleAsset = products.settleAsset(i);
            const premiumAsset = products.premiumAsset(i);
            market.info = {
              ...market.info,
              ...(({ 1: 'FUTURE', 2: 'CALL', 3: 'PUT' } as Record<number, string>)[kind] !== undefined && {
                instrumentKind: ({ 1: 'FUTURE', 2: 'CALL', 3: 'PUT' } as Record<number, string>)[kind],
              }),
              underlying: products.underlying(i),
              expiry,
              strike,
            };
            if (
              (kind === 2 || kind === 3) &&
              expiry !== undefined &&
              strike !== undefined &&
              style === 'european' &&
              settlement === 'cash' &&
              settleAsset &&
              premiumAsset
            ) {
              market.option = {
                underlying: products.underlying(i),
                expiry,
                strike,
                right: kind === 2 ? 'call' : 'put',
                style,
                settlement,
                settleAsset,
                premiumAsset,
              };
            }
          }
        }
        update.markets = rows;
        symbols = nextSymbols;
        ids = rows.map((row) => row.id);
        contractSizes = nextContractSizes;
        marketUnits = nextUnits;
      }
    }, LOADED_FLAG.MARKETS);

    const marketsFailed = errors.length > 0 || (this.marketsNeedSnapshot && !update.markets);
    this.marketsNeedSnapshot = marketsFailed;
    if (marketsFailed) {
      this.invalidateMarketIdentity();
      this.invalidMarketFrames.add(frame);
      if (!errors.length) errors.push(new Error('Market identity requires a snapshot'));
    }
    attempt(() => {
      if (marketsFailed) return;
      tickers = frame.tickers();
      if (tickers) {
        const rows: Ticker[] = [];
        for (let i = 0; i < tickers.count; i++) {
          const market = update.markets
            ? update.markets[tickers.market[i]]
            : this.store.marketsBySymbol[resolveSymbol(tickers.market[i])];
          const identity = tickers.identity?.(i);
          const ticker: Record<string, number | string> = {
            id: market?.id ?? identity?.id ?? '',
            symbol: market?.symbol ?? identity?.symbol ?? '',
          };
          for (const field of TICKER_FIELDS) {
            const value = tickers[field]?.[i];
            if (value !== undefined && (field !== 'midPrice' || !Number.isNaN(value))) ticker[field] = value;
          }
          rows.push(ticker as unknown as Ticker);
        }
        update.tickers = rows;
      }
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const deltas = frame.tickerDeltas();
      if (deltas && !tickers) {
        // The engine's ticker field order is the store's packed delta order.
        const packed: PackedTickerDelta[] = [];
        for (let i = 0; i < deltas.length;) {
          const mask = deltas[i + 1];
          if (
            !Number.isInteger(deltas[i]) ||
            deltas[i] < 0 ||
            deltas[i] >= (update.tickers ?? this.store.tickers).length ||
            !Number.isInteger(mask) ||
            mask <= 0 ||
            mask >= 1 << TICKER_FIELDS.length
          ) {
            throw new Error('Invalid ticker delta row or mask');
          }
          let width = 0;
          for (let field = 0; field < TICKER_FIELDS.length; field++) if (mask & (1 << field)) width++;
          if (i + 2 + width > deltas.length) throw new Error('Truncated ticker delta');
          packed.push(Array.from(deltas.subarray(i, i + 2 + width)) as PackedTickerDelta);
          i += 2 + width;
        }
        update.tickerDeltas = packed;
      }
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const orders = frame.orders();
      if (orders) {
        const rows: Order[] = [];
        for (let i = 0; i < orders.count; i++) {
          const enums = orderEnums(orders.enums[i]);
          const trigger = enums.triggerType as Order['tpTriggerType'];
          const symbol = resolveSymbol(orders.market[i], orders.symbol?.(i));
          const market = update.markets ? update.markets[orders.market[i]] : this.store.marketsBySymbol[symbol];
          const native = orders.remainingNative?.[i];
          const { nativePlacementAmounts, inverseNativeAmounts, contractCountAmounts, inverseContractSized } = unitsOf(
            orders.market[i],
          );
          const canDeriveNative = contractSizes[orders.market[i]] > 0;
          const regular = enums.type === 'limit' || enums.type === 'market';
          const placementFill = orders.filledInPlacementUnits?.[i];
          const exactPlacementFill =
            placementFill !== undefined && Number.isFinite(placementFill) ? placementFill : undefined;
          const nativeExtra = contractCountAmounts
            ? native !== undefined && Number.isFinite(native)
              ? {
                  remainingNative: native,
                  ...(market?.isInverse && { remainingNotional: native * contractSizes[orders.market[i]] }),
                  ...(exactPlacementFill !== undefined
                    ? { filledInPlacementUnits: exactPlacementFill }
                    : regular &&
                      orders.amount[i] >= native && { filledInPlacementUnits: subtract(orders.amount[i], native) }),
                }
              : !market?.isInverse && canDeriveNative
                ? {
                    remainingNative: divide(orders.remaining[i], contractSizes[orders.market[i]]),
                    ...(exactPlacementFill !== undefined
                      ? { filledInPlacementUnits: exactPlacementFill }
                      : regular && {
                          filledInPlacementUnits: divide(orders.filled[i], contractSizes[orders.market[i]]),
                        }),
                  }
                : exactPlacementFill !== undefined
                  ? { filledInPlacementUnits: exactPlacementFill }
                  : undefined
            : market?.isInverse && native !== undefined && Number.isFinite(native)
              ? {
                  remainingNative: native,
                  remainingNotional:
                    native *
                    (inverseContractSized || nativePlacementAmounts ? contractSizes[orders.market[i]] : 1),
                }
              : inverseNativeAmounts
                ? { remainingNative: Math.max(0, subtract(orders.amount[i], orders.filled[i])) }
                : undefined;
          const exactNative = finite(native ?? NaN);
          const remainingNotional = finite(orders.remainingNotional?.[i] ?? NaN);
          const conditionalOrderPrice = finite(orders.conditionalOrderPrice?.[i] ?? NaN);
          const extra =
            nativeExtra ||
            enums.rejectReason ||
            nativePlacementAmounts ||
            exactNative !== undefined ||
            exactPlacementFill !== undefined ||
            remainingNotional !== undefined ||
            conditionalOrderPrice !== undefined
              ? {
                  ...nativeExtra,
                  ...(exactNative !== undefined && { remainingNative: exactNative }),
                  ...(exactPlacementFill !== undefined && { filledInPlacementUnits: exactPlacementFill }),
                  ...(remainingNotional !== undefined && { remainingNotional }),
                  ...(conditionalOrderPrice !== undefined && { conditionalOrderPrice }),
                  ...(enums.rejectReason && { rejectReason: enums.rejectReason as never }),
                  ...(nativePlacementAmounts &&
                    canDeriveNative && {
                      remainingNative:
                        native !== undefined && Number.isFinite(native)
                          ? native
                          : divide(orders.remaining[i], contractSizes[orders.market[i]]),
                      filledInPlacementUnits:
                        exactPlacementFill !== undefined
                          ? exactPlacementFill
                          : market?.isInverse
                            ? undefined
                            : divide(orders.filled[i], contractSizes[orders.market[i]]),
                    }),
                }
              : undefined;
          rows.push({
            id: orders.id(i),
            status: enums.status as Order['status'],
            symbol,
            type: enums.type as Order['type'],
            side: enums.side as Order['side'],
            price: orders.price[i],
            amount: orders.amount[i],
            filled: orders.filled[i],
            remaining: orders.remaining[i],
            reduceOnly: enums.reduceOnly,
            lastUpdate: orders.lastUpdate[i],
            ...(enums.close !== undefined && { close: enums.close }),
            ...(orders.clientOrderId(i) !== undefined && { clientOrderId: orders.clientOrderId(i) }),
            ...(orders.parentId(i) !== undefined && { parentId: orders.parentId(i) }),
            ...(enums.timeInForce !== undefined && { timeInForce: enums.timeInForce as Order['timeInForce'] }),
            ...(trigger && { triggerType: trigger }),
            ...(enums.type === 'take_profit_market' && trigger && { tpTriggerType: trigger }),
            ...(orders.tpTriggerType?.(i) !== undefined && {
              tpTriggerType: orders.tpTriggerType!(i) as Order['tpTriggerType'],
            }),
            ...((enums.type === 'stop_market' || enums.type === 'trailing_stop_market') &&
              trigger && { slTriggerType: trigger }),
            ...(orders.slTriggerType?.(i) !== undefined && {
              slTriggerType: orders.slTriggerType!(i) as Order['slTriggerType'],
            }),
            ...(finite(orders.takeProfit[i]) !== undefined && { takeProfit: orders.takeProfit[i] }),
            ...(finite(orders.stopLoss[i]) !== undefined && { stopLoss: orders.stopLoss[i] }),
            ...(extra && { extra }),
          } as Order);
        }
        update.orders = rows;
      }
    }, LOADED_FLAG.ORDERS);

    attempt(() => {
      if (marketsFailed) return;
      const positions = frame.positions();
      if (positions) {
        const rows: Position[] = [];
        const greekTable = frame.positionGreeks?.();
        const greeksById = new Map<string, NonNullable<Position['option']>>();
        if (greekTable) {
          for (let i = 0; i < greekTable.count; i++) {
            greeksById.set(greekTable.positionId(i), {
              greeks: {
                delta: greekTable.delta[i],
                gamma: greekTable.gamma[i],
                vega: greekTable.vega[i],
                theta: greekTable.theta[i],
                units: greekTable.units(i),
              },
            });
          }
        }
        const predictionHoldings = frame.predictionHoldings?.();
        const holdingFlags = predictionHoldings && new Map<number, number>();
        if (predictionHoldings) {
          for (let i = 0; i < predictionHoldings.count; i++)
            holdingFlags!.set(predictionHoldings.market[i], predictionHoldings.flags[i]);
        }
        for (let i = 0; i < positions.count; i++) {
          const enums = positionEnums(positions.enums[i]);
          const symbol = resolveSymbol(positions.market[i], positions.symbol?.(i));
          const market = update.markets ? update.markets[positions.market[i]] : this.store.marketsBySymbol[symbol];
          const mode = enums.positionMode as PositionMode | undefined;
          const { nativePlacementAmounts, inverseNativeAmounts, contractCountAmounts, inverseContractSized } = unitsOf(
            positions.market[i],
          );
          const id = positions.id(i) ?? (mode === PositionMode.Hedged ? `${symbol}:${enums.side}` : symbol);
          rows.push({
            id,
            ...(greeksById.has(id) && { option: greeksById.get(id) }),
            symbol,
            side: enums.side as Position['side'],
            ...(market?.info?.conditionId && {
              info: {
                tokenId: market.id,
                conditionId: market.info.conditionId,
                redeemable: ((holdingFlags?.get(positions.market[i]) ?? 0) & 1) !== 0,
                mergeable: ((holdingFlags?.get(positions.market[i]) ?? 0) & 2) !== 0,
              },
            }),
            entryPrice: positions.entryPrice[i],
            notional: positions.notional[i],
            leverage: displayNumber(positions.leverage[i]),
            unrealizedPnl: displayNumber(positions.unrealizedPnl[i]),
            contracts: positions.contracts[i],
            ...(market &&
              (nativePlacementAmounts ||
                contractCountAmounts ||
                market?.isInverse ||
                inverseNativeAmounts) && {
                extra: {
                  ...(nativePlacementAmounts && {
                    contractsNative: divide(positions.contracts[i], contractSizes[positions.market[i]]),
                  }),
                  ...(contractCountAmounts
                    ? {
                        contractsNative: divide(
                          market?.isInverse ? positions.notional[i] : positions.contracts[i],
                          contractSizes[positions.market[i]],
                        ),
                      }
                    : (market?.isInverse || inverseNativeAmounts) && {
                        contractsNative: divide(
                          positions.notional[i],
                          inverseContractSized || nativePlacementAmounts
                            ? contractSizes[positions.market[i]]
                            : 1,
                        ),
                        ...(!nativePlacementAmounts && {
                          unrealizedPnlNative: displayNumber(positions.unrealizedPnl[i]),
                        }),
                      }),
                },
              }),
            liquidationPrice: displayNumber(positions.liquidationPrice[i]),
            ...(mode !== undefined && { positionMode: mode }),
            ...(enums.marginType !== undefined && { marginType: enums.marginType as MarginType }),
            ...(finite(positions.realizedPnl[i]) !== undefined && { realizedPnl: positions.realizedPnl[i] }),
            ...(finite(positions.marginSize[i]) !== undefined && { marginSize: positions.marginSize[i] }),
            ...(finite(positions.lastUpdate[i]) !== undefined && { lastUpdate: positions.lastUpdate[i] }),
          } as Position);
        }
        update.positions = rows;
      }
    }, LOADED_FLAG.POSITIONS);

    attempt(() => {
      const balance = frame.balance();
      if (balance) update.balance = { ...this.store.balance, ...balance };
    }, LOADED_FLAG.BALANCE);

    attempt(() => {
      const status = frame.status();
      if (status) {
        const loaded = status.loaded;
        update.loaded = {
          ...this.store.loaded,
          balance: (loaded & LOADED_FLAG.BALANCE) !== 0,
          orders: (loaded & LOADED_FLAG.ORDERS) !== 0,
          markets: (loaded & LOADED_FLAG.MARKETS) !== 0,
          tickers: (loaded & LOADED_FLAG.TICKERS) !== 0,
          positions: (loaded & LOADED_FLAG.POSITIONS) !== 0,
        };
        update.latency = status.latency;
        update.options = { ...this.store.options, isHedged: status.hedged };
      }
    });

    attempt(() => {
      if (marketsFailed) return;
      // Per-market leverage and margin type: the order form falls back to
      // `fetchedLeverage` when a market has no position.
      const settings = frame.settings();
      if (settings) {
        const hedged = (update.options ?? this.store.options).isHedged;
        const leverage: Record<string, FetchedLeverage> = {};
        for (let i = 0; i < settings.count; i++) {
          const value = finite(settings.leverage[i]);
          const buyLeverage = finite(settings.buyLeverage?.[i] ?? NaN) ?? value;
          const sellLeverage = finite(settings.sellLeverage?.[i] ?? NaN) ?? value;
          // An unknown side omits the symbol: UiStateHandler copies these onto positions.
          if (buyLeverage === undefined || sellLeverage === undefined) continue;
          leverage[resolveSymbol(settings.market[i], settings.symbol?.(i))] = {
            buyLeverage,
            sellLeverage,
            marginType: settings.marginType(i) as MarginType | undefined,
            positionMode: hedged ? PositionMode.Hedged : PositionMode.OneWay,
          };
        }
        this.writeFetchedLeverage(update, leverage);
      }
    });

    this.frameMarkets.set(frame, { symbols, ids });
    if (applyBooks && !marketsFailed) {
      try {
        this.ingestBooks(frame, symbols, ids);
      } catch (error) {
        // A rejected batch may discard valid deltas too; require fresh baselines.
        this.bookStates.clear();
        this.frameBooks.set(frame, []);
        errors.push(error);
      }
    } else if (marketsFailed) this.frameBooks.set(frame, []);
    else this.bookStates.clear();

    this.marketSymbols = symbols;
    this.marketIds = ids;
    this.contractSizes = contractSizes;
    this.marketUnits = marketUnits;
    if (marketUnits.length) this.venueUnits = marketUnits[0];
    // Install proof before notifying startup waiters. Empty validated tables
    // count; rejected sections leave independent accepted peers usable.
    if (update.markets && !marketsFailed) this.appliedSections |= LOADED_FLAG.MARKETS;
    if (update.orders) this.appliedSections |= LOADED_FLAG.ORDERS;
    if (update.positions) this.appliedSections |= LOADED_FLAG.POSITIONS;
    if (update.balance) this.appliedSections |= LOADED_FLAG.BALANCE;
    if (update.tickers) this.appliedSections |= LOADED_FLAG.TICKERS;

    let changed = false;
    for (const _key in update) {
      changed = true;
      break;
    }
    // A current-state read is not another independent venue observation. Its
    // pending stream frame still owns order/position freshness when it arrives.
    if (changed) {
      try {
        if (frame.accountRead?.()) this.store.update(update, false, 'account-read');
        else this.store.update(update);
      } catch (error) {
        if (!errors.length) throw error;
        errors.push(error);
      }
    }
    if (errors.length) throw new FrameApplyError(errors.map(String).join('; '));
  }

  invalidateMarketIdentity(): void {
    this.appliedSections &= ~(LOADED_FLAG.MARKETS | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS | LOADED_FLAG.TICKERS);
    this.marketsNeedSnapshot = true;
    this.bookStates.clear();
  }

  applyLeverage(symbol: string, leverage: FetchedLeverage): void {
    this.applyLeverageSnapshot({ ...this.store.fetchedLeverage, [symbol]: leverage });
  }

  applyLeverageSnapshot(leverage: Record<string, FetchedLeverage>): void {
    const update: Writable<StoreUpdate> = {};
    this.writeFetchedLeverage(update, leverage);
    this.store.update(update);
  }

  private writeFetchedLeverage(update: Writable<StoreUpdate>, leverage: Record<string, FetchedLeverage>): void {
    update.fetchedLeverage = leverage;
  }

  private symbol(market: number, fallback?: string): string {
    return this.marketSymbols[market] ?? fallback ?? '';
  }

  books(frame: Frame): FrameBook[] {
    const captured = this.frameMarkets.get(frame);
    const markets = captured ? undefined : frame.markets();
    return this.ingestBooks(
      frame,
      captured?.symbols ??
        (markets ? Array.from({ length: markets.count }, (_, i) => markets.symbol(i)) : this.marketSymbols),
      captured?.ids ?? (markets ? Array.from({ length: markets.count }, (_, i) => markets.id(i)) : this.marketIds),
    );
  }

  private ingestBooks(frame: Frame, symbols: string[], ids: string[]): FrameBook[] {
    const cached = this.frameBooks.get(frame);
    if (cached) return cached;
    const fullBooks = [...(frame.books?.() ?? [])];
    const deltaBooks = [...(frame.bookDeltas?.() ?? [])];
    if (!fullBooks.length && !deltaBooks.length && symbols === this.marketSymbols) {
      const empty: FrameBook[] = [];
      this.frameBooks.set(frame, empty);
      return empty;
    }
    // Stage every change: one invalid delta must not partially advance a baseline.
    const states = new Map(this.bookStates);
    if (symbols !== this.marketSymbols) {
      const identities = new Map(symbols.map((symbol, i) => [symbol, ids[i]]));
      for (const [symbol, state] of states) if (identities.get(symbol) !== state.id) states.delete(symbol);
    }
    const changed = new Set<string>();
    const identity = (market: number) => {
      const symbol = symbols[market];
      const id = ids[market];
      if (!symbol || id === undefined) throw new Error('Order book snapshot required: unknown market');
      return { symbol, id };
    };
    const applyLevels = (levels: Map<number, number>, rows: Float64Array, width: number) => {
      for (let i = 0; i < rows.length; i += width) {
        const price = rows[i];
        const amount = rows[i + 1];
        if (!Number.isFinite(price) || !Number.isFinite(amount) || amount < 0) {
          throw new Error('Order book snapshot required: invalid level');
        }
        if (amount === 0) levels.delete(price);
        else levels.set(price, amount);
      }
    };
    for (const book of fullBooks) {
      const { symbol, id } = identity(book.market);
      // A worker can transfer the source frame after reading it. Keep compact
      // numeric baselines, without allocating price maps or sorting full books.
      states.set(symbol, { id, bids: book.bids.slice(), asks: book.asks.slice() });
      changed.add(symbol);
    }
    const levelMap = (levels: BookLevels) => {
      if (levels instanceof Map) return new Map(levels);
      const result = new Map<number, number>();
      applyLevels(result, levels, 3);
      return result;
    };
    for (const book of deltaBooks) {
      const { symbol, id } = identity(book.market);
      const previous = states.get(symbol);
      if (!previous || previous.id !== id) throw new Error(`Order book snapshot required for ${symbol}`);
      const state = { id, bids: levelMap(previous.bids), asks: levelMap(previous.asks) };
      applyLevels(state.bids, book.bids, 2);
      applyLevels(state.asks, book.asks, 2);
      states.set(symbol, state);
      changed.add(symbol);
    }
    const side = (levels: BookLevels, descending: boolean): OrderBook['bids'] => {
      let total = 0;
      if (levels instanceof Float64Array) {
        const rows: OrderBook['bids'] = [];
        let previous = descending ? Infinity : -Infinity;
        for (let i = 0; i < levels.length; i += 3) {
          const price = levels[i];
          const amount = levels[i + 1];
          if (!Number.isFinite(price) || !Number.isFinite(amount) || amount < 0) {
            throw new Error('Order book snapshot required: invalid level');
          }
          if (descending ? price >= previous : price <= previous) return side(levelMap(levels), descending);
          previous = price;
          if (amount === 0) continue;
          total += amount;
          if (!Number.isFinite(total)) throw new Error('Order book snapshot required: invalid total');
          rows.push({ price, amount, total });
        }
        return rows;
      }
      return [...levels]
        .sort(([a], [b]) => (descending ? b - a : a - b))
        .map(([price, amount]) => {
          total += amount;
          if (!Number.isFinite(total)) throw new Error('Order book snapshot required: invalid total');
          return { price, amount, total };
        });
    };
    const books = [...changed].map((symbol) => {
      const state = states.get(symbol)!;
      return { symbol, orderbook: { bids: side(state.bids, true), asks: side(state.asks, false) } };
    });
    this.bookStates = states;
    this.frameBooks.set(frame, books);
    return books;
  }

  candles(frame: Frame): FrameCandle[] {
    if (this.invalidMarketFrames.has(frame)) throw new FrameApplyError('Candle identity requires a market snapshot');
    const rows = frame.candles();
    const candles: FrameCandle[] = [];
    for (let i = 0; i < (rows?.count ?? 0); i++) {
      const at = i * 6;
      candles.push({
        symbol: this.symbol(rows!.market[i]),
        interval: rows!.interval(i),
        candle: {
          timestamp: rows!.rows[at],
          open: rows!.rows[at + 1],
          high: rows!.rows[at + 2],
          low: rows!.rows[at + 3],
          close: rows!.rows[at + 4],
          volume: rows!.rows[at + 5],
        },
      });
    }
    return candles;
  }

  fills(frame: Frame): OrderFillEvent[] {
    const rows = frame.fills();
    const fills: OrderFillEvent[] = [];
    for (let i = 0; i < (rows?.count ?? 0); i++) {
      const fallback = rows!.symbol?.(i);
      const symbol = this.invalidMarketFrames.has(frame)
        ? fallback
        : (this.frameMarkets.get(frame)?.symbols[rows!.market[i]] ?? this.symbol(rows!.market[i], fallback));
      if (symbol === undefined) throw new FrameApplyError('Fill identity requires a market snapshot');
      const realizedPnl = rows!.realizedPnl?.[i];
      const fee = rows!.fee?.[i];
      fills.push({
        id: rows!.fillId?.(i) ?? rows!.orderId(i),
        orderId: rows!.orderId(i),
        symbol,
        ...([OrderSide.Buy, OrderSide.Sell][rows!.side[i]] !== undefined && {
          side: [OrderSide.Buy, OrderSide.Sell][rows!.side[i]],
        }),
        price: rows!.rows[i * 2],
        amount: rows!.rows[i * 2 + 1],
        ...(realizedPnl !== undefined && Number.isFinite(realizedPnl) && { realizedPnl }),
        ...(fee !== undefined && Number.isFinite(fee) && { fee }),
      } as OrderFillEvent);
    }
    return fills;
  }
}
