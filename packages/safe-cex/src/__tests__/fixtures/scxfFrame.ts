import { Frame } from '@tealstreet/safe-cex-wasm/wire';

const align8 = (n: number) => Math.ceil(n / 8) * 8;
export type OrderRow = {
  id: string;
  parentId?: string;
  amount: number;
  filled: number;
  remaining: number;
  native?: number;
  filledNative?: number;
  enums?: number;
};

// Independent encoder of the documented format-1 layout. The optional native
// tail must not change offsets in the original seven-column ORDERS table.
// `units` is the market's QUANTITY_UNITS index, carried in MARKETS flag bits 8..12.
/** The frame bytes; `inverseUnitsFrame` wraps them in a `Frame`. */
export const inverseUnitsFrameBytes = ({
  native = false,
  inverse = true,
  orders = [] as OrderRow[],
  positions = false,
  markets = true,
  contractSize = NaN,
  positionPnl = 0.002,
  units = 0,
  orderMarket = 0,
} = {}) => {
  const strings = [
    'BTCUSD',
    'BTC',
    'USD',
    ...orders.flatMap((row) => [row.id, ...(row.parentId ? [row.parentId] : [])]),
  ];
  const ref = (s: string | undefined) => (s === undefined ? 0xffffffff : strings.indexOf(s));
  const sections: Array<{ kind: number; bytes: Uint8Array }> = [];
  const table = (kind: number, words: number[], columns: number[][]) => {
    const bytes = new Uint8Array(align8(words.length * 4) + columns.reduce((n, column) => n + column.length * 8, 0));
    const view = new DataView(bytes.buffer);
    words.forEach((word, i) => view.setUint32(i * 4, word, true));
    let offset = align8(words.length * 4);
    for (const column of columns)
      for (const value of column) {
        view.setFloat64(offset, value, true);
        offset += 8;
      }
    sections.push({ kind, bytes });
  };
  if (markets)
    table(
      2,
      [1, 0, 0, 1, 2, (inverse ? 3 : 1) | (units << 8)],
      [1, 0.1, 1, 100000, 1, 100, NaN, contractSize, NaN, NaN].map((n) => [n]),
    );
  const columns = [
    orders.map(() => 54321),
    orders.map((o) => o.amount),
    orders.map((o) => o.filled),
    orders.map((o) => o.remaining),
    orders.map(() => 1234),
    orders.map(() => NaN),
    orders.map(() => NaN),
  ];
  if (native || orders.some((o) => o.filledNative !== undefined)) columns.push(orders.map((o) => o.native ?? NaN));
  if (orders.some((o) => o.filledNative !== undefined)) columns.push(orders.map((o) => o.filledNative ?? NaN));
  table(
    7,
    [
      orders.length,
      ...orders.flatMap((o) => [ref(o.id), 0xffffffff, ref(o.parentId)]),
      ...orders.map(() => orderMarket),
      ...orders.map((o) => o.enums ?? 256),
    ],
    columns,
  );
  if (positions)
    table(
      8,
      [1, 0, (2 << 8) | (2 << 16)],
      [49000, 1000, 10, positionPnl, 0.001, 0.02, 40000, 100, 1234].map((n) => [n]),
    );
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
  const buffer = new ArrayBuffer(16 + sections.reduce((n, section) => n + 8 + section.bytes.length, 0));
  const header = new DataView(buffer);
  header.setUint32(0, 0x46584353, true);
  header.setUint16(4, 1, true);
  header.setUint16(6, sections.length, true);
  let offset = 16;
  for (const section of sections) {
    header.setUint16(offset, section.kind, true);
    header.setUint32(offset + 4, section.bytes.length, true);
    new Uint8Array(buffer, offset + 8, section.bytes.length).set(section.bytes);
    offset += 8 + section.bytes.length;
  }
  return buffer;
};

export const inverseUnitsFrame = (options: Parameters<typeof inverseUnitsFrameBytes>[0] = {}) =>
  new Frame(inverseUnitsFrameBytes(options));
