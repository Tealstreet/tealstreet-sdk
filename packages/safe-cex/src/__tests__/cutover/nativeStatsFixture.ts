import { Frame } from '@tealstreet/safe-cex-wasm/wire';

type Row = { group: number; topic: string; flags: number; age: number };
const align = (n: number) => Math.ceil(n / 8) * 8;
export function statsFrame(rows: Row[], replace = true, kind = 25): Frame {
  const labels = rows.map((row) => new TextEncoder().encode(row.topic));
  const stringsSize = align(4 + (labels.length + 1) * 4 + labels.reduce((sum, bytes) => sum + bytes.length, 0));
  const statsSize = 8 + rows.length * 24;
  const buffer = new ArrayBuffer(16 + 8 + statsSize + 8 + stringsSize);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 2, true);
  view.setFloat64(8, 1234, true);
  view.setUint16(16, kind, true);
  view.setUint32(20, statsSize, true);
  view.setUint32(24, rows.length, true);
  view.setUint32(28, Number(replace), true);
  rows.forEach((row, i) => {
    const at = 32 + i * 24;
    view.setUint32(at, row.group, true);
    view.setUint32(at + 4, i, true);
    view.setUint32(at + 8, row.flags, true);
    view.setFloat64(at + 16, row.age, true);
  });
  const strings = 24 + statsSize;
  view.setUint16(strings, 1, true);
  view.setUint32(strings + 4, stringsSize, true);
  const payload = strings + 8;
  view.setUint32(payload, labels.length, true);
  let offset = 0;
  labels.forEach((label, i) => {
    view.setUint32(payload + 4 + i * 4, offset, true);
    new Uint8Array(buffer, payload + 4 + (labels.length + 1) * 4 + offset, label.length).set(label);
    offset += label.length;
  });
  view.setUint32(payload + 4 + labels.length * 4, offset, true);
  return new Frame(buffer);
}
