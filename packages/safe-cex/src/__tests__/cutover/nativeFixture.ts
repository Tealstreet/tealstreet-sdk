import type { StoreData } from '../../generated/StoreData';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { consumerFrame } from './consumerFrame';

/** Test encoder for native replacement sections 26–32, including IEEE non-finite values. */
function nativeValue(value: unknown): Uint8Array {
  const bytes: number[] = [];
  const u32 = (n: number) => {
    bytes.push(n & 255, (n >>> 8) & 255, (n >>> 16) & 255, n >>> 24);
  };
  const text = (value: string) => {
    u32(value.length);
    for (let i = 0; i < value.length; i++) bytes.push(value.charCodeAt(i) & 255, value.charCodeAt(i) >>> 8);
  };
  const write = (value: unknown): void => {
    if (value === undefined) bytes.push(0);
    else if (value === null) bytes.push(1);
    else if (value === false) bytes.push(2);
    else if (value === true) bytes.push(3);
    else if (typeof value === 'number') {
      bytes.push(4);
      const number = new Uint8Array(8);
      new DataView(number.buffer).setFloat64(0, value, true);
      bytes.push(...number);
    } else if (typeof value === 'string') {
      bytes.push(5);
      text(value);
    } else if (Array.isArray(value)) {
      bytes.push(6);
      u32(value.length);
      value.forEach(write);
    } else if (typeof value === 'object') {
      const entries = Object.entries(value);
      bytes.push(7);
      u32(entries.length);
      for (const [key, item] of entries) {
        text(key);
        write(item);
      }
    } else throw new Error('Unsupported native fixture value');
  };
  write(value);
  const result = new Uint8Array(Math.ceil((4 + bytes.length) / 8) * 8);
  new DataView(result.buffer).setUint32(0, bytes.length, true);
  result.set(bytes, 4);
  return result;
}

/** Build real wire bytes; fixtures never restore the removed mutable store API. */
export function nativeStoreFrame(data: Partial<StoreData>, symbols?: readonly string[]): ArrayBuffer {
  const identities = symbols ?? [
    ...new Set([
      ...(data.markets?.map((row) => row.symbol) ?? []),
      ...(data.tickers?.map((row) => row.symbol) ?? []),
      ...(data.orders?.map((row) => row.symbol) ?? []),
      ...(data.positions?.map((row) => row.symbol) ?? []),
      ...Object.keys(data.fetchedLeverage ?? {}),
    ]),
  ];
  const base = consumerFrame(data, undefined, false, identities);
  const sections = [
    [26, data.socketHealth],
    [27, data.activeChasers],
    [28, data.activeTWAPs],
    [29, data.ordersRisk],
    [30, data.displayedPositions],
    [31, data.displayedMargin],
    [32, data.balance?.info],
  ] as const;
  const extra = sections.flatMap(([kind, value]) => (value === undefined ? [] : [{ kind, bytes: nativeValue(value) }]));
  const result = new ArrayBuffer(base.byteLength + extra.reduce((size, section) => size + 8 + section.bytes.length, 0));
  const bytes = new Uint8Array(result);
  bytes.set(new Uint8Array(base));
  const view = new DataView(result);
  view.setUint16(6, view.getUint16(6, true) + extra.length, true);
  let offset = base.byteLength;
  for (const section of extra) {
    view.setUint16(offset, section.kind, true);
    view.setUint32(offset + 4, section.bytes.length, true);
    bytes.set(section.bytes, offset + 8);
    offset += 8 + section.bytes.length;
  }
  return result;
}

/** Test producer: encode generated section data into real SCXF bytes. */
export function nativeFixture(data: Partial<StoreData>, baselineSymbols: readonly string[] = []): Frame {
  const symbols = [
    ...new Set([
      ...(data.markets?.map((row) => row.symbol) ?? baselineSymbols),
      ...(data.orders?.map((row) => row.symbol) ?? []),
      ...(data.positions?.map((row) => row.symbol) ?? []),
      ...(data.tickers?.map((row) => row.symbol) ?? []),
      ...Object.keys(data.fetchedLeverage ?? {}),
    ]),
  ];
  return new Frame(nativeStoreFrame(data, symbols));
}
