// Float-noise frames through the actual incrementalFills.ts; bits are exact.
import {loadSource} from './source_loader.mjs';
const {createIncrementalFills, cleanFloat} = loadSource(new URL('../../../packages/safe-cex/src/utils/incrementalFills.ts', import.meta.url));
const bits = (n) => {const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex');};
const read = (h) => Buffer.from(h, 'hex').readDoubleBE(0);
const {sequences, cleans} = JSON.parse(process.argv[2]);
const out = {
  sequences: sequences.map((frames) => {
    const fills = createIncrementalFills(500);
    return frames.map(([qty, value]) => {
      const r = fills('A', read(qty), value === null ? undefined : read(value));
      return {qty: bits(r.qty), value: r.value === undefined ? null : bits(r.value)};
    });
  }),
  cleans: cleans.map((h) => bits(cleanFloat(read(h)))),
};
process.stdout.write(JSON.stringify(out));
