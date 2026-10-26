import type { Order, Position, StoreData } from '../../types';

// Independent fixture encoder for the documented format-1 contract. This is
// deliberately separate from the Rust writer and JS reader under test.
const align8 = (n: number) => Math.ceil(n / 8) * 8;
const none = 0xffffffff;

export function consumerFrame(
  data: Partial<StoreData>,
  delta?: number[],
  events = false,
  marketSymbols: readonly string[] = data.markets?.map((market) => market.symbol) ?? ['BTCUSDT'],
  /** The venue's quantity-units code, written into market flag bits 8-11. */
  quantityUnits = 0,
  accountRead = false,
): ArrayBuffer {
  const strings: string[] = [];
  const ref = (value: string | undefined) => {
    if (value === undefined) return none;
    let index = strings.indexOf(value);
    if (index < 0) index = strings.push(value) - 1;
    return index;
  };
  const sections: Array<{ kind: number; bytes: Uint8Array }> = [];
  const marketIndex = (symbol: string) => {
    const index = marketSymbols.indexOf(symbol);
    if (index < 0) throw new Error(`Fixture market index missing for ${symbol}`);
    return index;
  };
  const table = (kind: number, words: number[], columns: number[][], tail: number[] = []) => {
    const size = align8(words.length * 4) + columns.reduce((n, col) => n + col.length * 8, 0);
    const bytes = new Uint8Array(align8(size + tail.length * 4));
    const view = new DataView(bytes.buffer);
    words.forEach((word, i) => view.setUint32(i * 4, word, true));
    let at = align8(words.length * 4);
    for (const col of columns)
      for (const value of col) {
        view.setFloat64(at, value, true);
        at += 8;
      }
    tail.forEach((word, i) => view.setUint32(at + i * 4, word, true));
    sections.push({ kind, bytes });
  };
  if (data.markets) {
    const rows = data.markets;
    table(
      2,
      [
        rows.length,
        ...rows.flatMap((m) => [ref(m.id), ref(m.symbol), ref(m.base), ref(m.quote)]),
        ...rows.map(
          (m) =>
            Number(m.active) |
            (m.isInverse ? 2 : 0) |
            (m.isSpot ? 4 : 0) |
            (m.isPerpetual ? 8 : 0) |
            (m.isExpiring ? 16 : 0) |
            (m.isReadOnly ? 32 : 0) |
            (quantityUnits << 8),
        ),
      ],
      [
        rows.map((m) => m.precision.amount),
        rows.map((m) => m.precision.price),
        rows.map((m) => m.limits.amount.min),
        rows.map((m) => m.limits.amount.max),
        rows.map((m) => m.limits.leverage.min),
        rows.map((m) => m.limits.leverage.max),
        rows.map((m) => m.limits.price?.min ?? NaN),
        rows.map((m) => m.contractSize ?? NaN),
        rows.map((m) => m.takerFee ?? NaN),
        rows.map((m) => m.makerFee ?? NaN),
      ],
    );
    const classifications = rows
      .map((market, index) => ({ market, index }))
      .filter(({ market }) =>
        ['symbolType', 'marketRegion', 'underlyingTicker'].some((field) => market.info?.[field] !== undefined),
      );
    if (classifications.length)
      table(
        16,
        [
          classifications.length,
          ...classifications.map(({ index }) => index),
          ...classifications.flatMap(({ market }) => [
            ref(market.info?.symbolType),
            ref(market.info?.marketRegion),
            ref(market.info?.underlyingTicker),
          ]),
        ],
        [],
      );
    const underlying = rows
      .map((market, index) => ({ market, index }))
      .filter(({ market }) => market.info?.hlTokenName !== undefined);
    if (underlying.length)
      table(
        20,
        [
          underlying.length,
          ...underlying.map(({ index }) => index),
          ...underlying.flatMap(({ market }) => [ref(market.info.hlTokenName), ref(market.info.hlTokenFullName)]),
        ],
        [],
      );
    const products = rows
      .map((market, index) => ({ market, index }))
      .filter(({ market }) => market.option || ['FUTURE', 'CALL', 'PUT'].includes(market.info?.instrumentKind));
    if (products.length)
      table(
        17,
        [
          products.length,
          ...products.map(({ index }) => index),
          ...products.map(({ market }) =>
            market.option
              ? market.option.right === 'call' ? 2 : 3
              : ['FUTURE', 'CALL', 'PUT'].indexOf(market.info?.instrumentKind) + 1,
          ),
          ...products.flatMap(({ market }) => [
            ref(market.option?.underlying ?? market.info?.underlying),
            ref(market.option?.settleAsset),
            ref(market.option?.premiumAsset),
            ref(market.option?.style),
            ref(market.option?.settlement),
          ]),
        ],
        [
          products.map(({ market }) => market.option?.expiry ?? market.info?.expiry ?? NaN),
          products.map(({ market }) => market.option?.strike ?? market.info?.strike ?? NaN),
        ],
      );
  }
  if (data.tickers)
    table(
      3,
      [data.tickers.length, ...data.tickers.map((row) => marketIndex(row.symbol))],
      [
        'bid',
        'ask',
        'last',
        'mark',
        'index',
        'percentage',
        'openInterest',
        'fundingRate',
        'fundingTime',
        'volume',
        'quoteVolume',
      ].map((field) => data.tickers!.map((row) => row[field as keyof typeof row] as number)),
    );
  if (delta) table(4, [], [delta]);
  if (data.orders) {
    const rows = data.orders;
    const status = ['open', 'closed', 'canceled', 'creating', 'updating', 'canceling'];
    const type = ['market', 'limit', 'conditional', 'stop_market', 'take_profit_market', 'trailing_stop_market'];
    const tif = [undefined, 'GoodTillCancel', 'ImmediateOrCancel', 'FillOrKill', 'PostOnly'];
    const trigger = [undefined, 'mark_price', 'last_price', 'index_price'];
    const rejection = [undefined, 'PostOnly', 'InsufficientBalance', 'ReduceOnly'];
    const enums = (row: Order) =>
      status.indexOf(row.status) |
      (type.indexOf(row.type) << 8) |
      (row.side === 'sell' ? 1 << 16 : 0) |
      (row.reduceOnly ? 2 << 16 : 0) |
      (row.close === undefined ? 0 : (8 | (row.close ? 4 : 0)) << 16) |
      (rejection.indexOf(row.extra?.rejectReason) << 20) |
      (tif.indexOf(row.timeInForce) << 24) |
      (trigger.indexOf(row.triggerType ?? (row.type === 'take_profit_market' ? row.tpTriggerType : row.slTriggerType)) << 28);
    table(
      7,
      [
        rows.length,
        ...rows.flatMap((row) => [ref(row.id), ref(row.clientOrderId), ref(row.parentId)]),
        ...rows.map((row) => marketIndex(row.symbol)),
        ...rows.map(enums),
      ],
      ['price', 'amount', 'filled', 'remaining', 'lastUpdate', 'takeProfit', 'stopLoss']
        .map((field) => rows.map((row) => (row[field as keyof Order] as number) ?? NaN))
        .concat([rows.map((row) => (row.extra?.remainingNative as number) ?? NaN)]),
    );
  }
  if (data.positions) {
    const rows = data.positions;
    const enums = (row: Position) =>
      Number(row.side === 'short') |
      ([undefined, 'isolated', 'cross'].indexOf(row.marginType) << 8) |
      ([undefined, 'hedged', 'oneway'].indexOf(row.positionMode) << 16);
    table(
      8,
      [rows.length, ...rows.map((row) => marketIndex(row.symbol)), ...rows.map(enums)],
      [
        'entryPrice',
        'notional',
        'leverage',
        'unrealizedPnl',
        'realizedPnl',
        'contracts',
        'liquidationPrice',
        'marginSize',
        'lastUpdate',
      ].map((field) => rows.map((row) => (row[field as keyof Position] as number) ?? NaN)),
      rows.map((row) => ref(row.id)),
    );
  }
  if (data.balance) {
    const balance = data.balance;
    const coins = Object.entries(balance.coins ?? {});
    const currencies = Object.entries(balance.currencies ?? {});
    const coinData = align8(36 + coins.length * 4);
    const currencyTail = coinData + coins.length * 16;
    const currencyData = align8(currencyTail + 8 + currencies.length * 4);
    const size =
      balance.currencies !== undefined
        ? currencyData + currencies.length * 32
        : balance.coins !== undefined
          ? currencyTail
          : 32;
    const bytes = new Uint8Array(size);
    const view = new DataView(bytes.buffer);
    [balance.used, balance.free, balance.total, balance.upnl].forEach((value, i) =>
      view.setFloat64(i * 8, value, true),
    );
    if (size > 32) {
      view.setUint32(32, coins.length, true);
      coins.forEach(([name, coin], i) => {
        view.setUint32(36 + i * 4, ref(name), true);
        view.setFloat64(coinData + i * 8, coin.amount, true);
        view.setFloat64(coinData + coins.length * 8 + i * 8, coin.notional, true);
      });
      if (balance.currencies !== undefined) {
        view.setUint32(currencyTail, currencies.length, true);
        view.setUint32(currencyTail + 4, Number(balance.coins === undefined), true);
        currencies.forEach(([name, value], i) => {
          view.setUint32(currencyTail + 8 + i * 4, ref(name), true);
          [value.total, value.free, value.used, value.upnl].forEach((amount, col) =>
            view.setFloat64(currencyData + (col * currencies.length + i) * 8, amount, true),
          );
        });
      }
    }
    sections.push({ kind: 9, bytes });
  }
  if (data.loaded)
    table(
      10,
      [
        Number(data.loaded.balance) |
          (Number(data.loaded.orders) << 1) |
          (Number(data.loaded.markets) << 2) |
          (Number(data.loaded.tickers) << 3) |
          (Number(data.loaded.positions) << 4),
        Number(data.options?.isHedged),
      ],
      [[data.latency ?? 0]],
    );
  if (data.fetchedLeverage) {
    const entries = Object.entries(data.fetchedLeverage);
    const rows = entries.map(([, value]) => value);
    table(
      12,
      [
        rows.length,
        ...entries.map(([symbol]) => marketIndex(symbol)),
        ...rows.map((row) => [undefined, 'isolated', 'cross'].indexOf(row.marginType)),
      ],
      [rows.map((row) => row.buyLeverage), rows.map((row) => row.buyLeverage), rows.map((row) => row.sellLeverage)],
    );
  }
  if (events) {
    table(5, [0, 1, 1, 0], [[99, 2, 2, 101, 3, 3]]);
    table(6, [1, 0, ref('1m')], [[1_700_000_000, 100, 102, 99, 101, 5]]);
    table(11, [1, ref('order-1'), 0, 0], [[100, 0.3]]);
  }
  const encoded = strings.map((s) => new TextEncoder().encode(s));
  const bytes = new Uint8Array(align8(4 + (strings.length + 1) * 4 + encoded.reduce((n, s) => n + s.length, 0)));
  const view = new DataView(bytes.buffer);
  view.setUint32(0, strings.length, true);
  let end = 0;
  encoded.forEach((s, i) => {
    view.setUint32(4 + i * 4, end, true);
    bytes.set(s, 4 + (strings.length + 1) * 4 + end);
    end += s.length;
  });
  view.setUint32(4 + strings.length * 4, end, true);
  sections.push({ kind: 1, bytes });
  // ACCOUNT_READ: a current-state read that must not advance freshness IDs.
  if (accountRead) sections.push({ kind: 21, bytes: new Uint8Array(0) });
  const buffer = new ArrayBuffer(16 + sections.reduce((n, s) => n + 8 + s.bytes.length, 0));
  const header = new DataView(buffer);
  header.setUint32(0, 0x46584353, true);
  header.setUint16(4, 1, true);
  header.setUint16(6, sections.length, true);
  header.setFloat64(8, 1_700_000_000_000, true);
  let at = 16;
  for (const section of sections) {
    header.setUint16(at, section.kind, true);
    header.setUint32(at + 4, section.bytes.length, true);
    new Uint8Array(buffer, at + 8, section.bytes.length).set(section.bytes);
    at += 8 + section.bytes.length;
  }
  return buffer;
}
