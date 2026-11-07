import type { Order, Position } from '../types';
import type { ContextOrder, ContextPosition, SymbolSettings, WriteContext, WriterCommand } from './writerRpc';

/** The store a write's caller keeps current from its frames; the writer has none. */
export type WriteContextStore = {
  readonly orders: readonly Order[];
  readonly positions: readonly Position[];
  readonly options: { readonly isHedged: boolean };
};

const contextOrder = (order: Order): ContextOrder => ({
  id: order.id,
  symbol: order.symbol,
  side: order.side,
  type: order.type,
  price: order.price,
  amount: order.amount,
  remaining: order.remaining,
  reduceOnly: order.reduceOnly,
  ...(order.clientOrderId !== undefined && { clientOrderId: order.clientOrderId }),
});

const contextPosition = (position: Position): ContextPosition => ({
  symbol: position.symbol,
  side: position.side,
  contracts: position.contracts,
});

const symbolSettings = (store: WriteContextStore, symbol: string): SymbolSettings => {
  const marginType = store.positions.find((position) => position.symbol === symbol)?.marginType;
  return { hedged: store.options.isHedged, ...(marginType !== undefined && { marginMode: marginType }) };
};

const strings = (value: unknown): string[] =>
  Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];

const symbolsOf = (command: WriterCommand): string[] => {
  const symbols = new Set<string>();
  if (typeof command.symbol === 'string' && command.symbol) symbols.add(command.symbol);
  for (const field of ['orders'] as const) {
    const rows = command[field];
    if (!Array.isArray(rows)) continue;
    for (const row of rows) {
      const symbol = (row as { symbol?: unknown } | null)?.symbol;
      if (typeof symbol === 'string' && symbol) symbols.add(symbol);
    }
  }
  return [...symbols];
};

const orderIdsOf = (command: WriterCommand): string[] => {
  const ids = new Set(strings(command.ids));
  if (typeof command.id === 'string') ids.add(command.id);
  if (Array.isArray(command.orders) && command.op !== 'placeOrders') {
    for (const row of command.orders) {
      const id = (row as { id?: unknown } | null)?.id;
      if (typeof id === 'string') ids.add(id);
    }
  }
  return [...ids];
};

/**
 * The store fields one write reads: the orders it names, and the positions and
 * settings of the symbols it touches. An account-wide write (an empty symbol)
 * carries every order and position, because the venue path may walk them.
 */
export const buildWriteContext = (store: WriteContextStore, command: WriterCommand): WriteContext => {
  if (command.op === 'refreshMarkets') return {};
  const accountWide = command.symbol === '' && command.op !== 'placeOrders';
  const ids = new Set(orderIdsOf(command));
  // An amend by id names no symbol; the order it names does.
  const named = store.orders.filter((order) => ids.has(order.id)).map((order) => order.symbol);
  const symbols = accountWide
    ? [...new Set([...store.positions, ...store.orders].map((row) => row.symbol))]
    : [...new Set([...symbolsOf(command), ...named])];
  const orders = store.orders.filter(
    (order) =>
      ids.has(order.id) || accountWide || (command.op === 'cancelSymbolOrders' && symbols.includes(order.symbol)),
  );
  const positions = store.positions.filter((position) => symbols.includes(position.symbol));
  const context: WriteContext = {};
  if (orders.length) context.orders = orders.map(contextOrder);
  if (positions.length) context.positions = positions.map(contextPosition);
  if (symbols.length)
    context.settings = Object.fromEntries(symbols.map((symbol) => [symbol, symbolSettings(store, symbol)]));
  return context;
};
