// E_BTCUSDT -> E-BTC-USDT
// E_BTCUSDC -> E-BTC-USDC
const QUOTES = ['USDT', 'USDC', 'USD'];

export const reverseWsSymbol = (symbol: string) => {
  const raw = symbol.toUpperCase().replace(/^E[-_]?/, '').replace(/[-_]/g, '');
  const quote = QUOTES.find((q) => raw.endsWith(q));
  if (!quote) {
    return `E-${raw}`;
  }
  const base = raw.slice(0, -quote.length);
  return `E-${base}-${quote}`;
};
//
// export const createPositionStub = (symbol: string, leverage: number, marginType?: MarginType): Position => ({
//   id: createPositionId(symbol, PositionSide.Long, PositionMode.OneWay),
//   symbol: symbol,
//   side: PositionSide.Long,
//   positionMode: PositionMode.OneWay,
//   marginType: marginType || MarginType.Cross,
//   entryPrice: 0,
//   notional: 0,
//   leverage: m.limits.leverage.max,
//   unrealizedPnl: 0,
//   contracts: 0,
//   liquidationPrice: 0,
//   info: {},
// });
