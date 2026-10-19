import {loadSource} from './source_loader.mjs';
if (process.versions.node !== '24.20.0' || !process.versions.v8.startsWith('13.6.')) {
  throw Error('Use CI Node 24.20.0 / V8 13.6 for inconsistent comparator ordering');
}
const {sortOrderBook} = loadSource(new URL('../../../packages/safe-cex/src/utils/orderbook.ts', import.meta.url));
const bits = value => {
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const inputs = [[3,NaN,2,1], [0,-0,0,-0], [Infinity,-Infinity,NaN,Infinity,0,-0], []];
for (const n of [1,2,3,7,8,16,22,31,32,33,63,64,65,95,96,127,128,129,255,256,257,511,512,513,1024,2048,4096]) {
  for (let pattern = 0; pattern < 4; ++pattern) {
    inputs.push(Array.from({length:n}, (_,i) => pattern === 0
      ? (i % 7 === 0 || i % 11 === 0 ? NaN : (i*197+558)%997)
      : pattern === 1 ? [NaN,Infinity,-Infinity,-0,0,1][i%6]
      : pattern === 2 ? n-i : i%17));
  }
}
const rows = inputs.map(prices => {
  const levels = () => prices.map((price,id) => ({price,amount:id+1,total:-id}));
  const book = {bids:levels(),asks:levels()};
  sortOrderBook(book);
  return {input:prices.map(bits),bids:book.bids.map(x=>x.amount-1),asks:book.asks.map(x=>x.amount-1)};
});
process.stdout.write(JSON.stringify(rows));
