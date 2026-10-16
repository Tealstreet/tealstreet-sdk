import {loadSource, ts} from './source_loader.mjs';
import fs from 'node:fs';
import vm from 'node:vm';
const sourceUrl = new URL('../../../packages/safe-cex/src/utils/sequencedDeltaOrderBook.ts', import.meta.url);
let source = fs.readFileSync(sourceUrl, 'utf8');
// Negative control changes maintained TS in memory, never the source checkout.
if (process.env.SEQUENCED_BOOK_TS_CONTROL === 'amount') {
  const before = 'updateOrderBookByPrice(orderBook, side, price, amount);';
  if (!source.includes(before)) throw Error('TS mutation anchor missing');
  source = source.replace(before, 'updateOrderBookByPrice(orderBook, side, price, amount + 1);');
}
const module = {exports:{}};
const compiled = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInThisContext('(function(require,module,exports){'+compiled+'\n})')(
  specifier => {
    if (specifier !== './updateOrderBook') throw Error('unexpected dependency '+specifier);
    return loadSource(new URL('../../../packages/safe-cex/src/utils/updateOrderBook.ts',import.meta.url));
  }, module, module.exports);
const {SequencedDeltaOrderBook} = module.exports;
const encode = value => {
  if (typeof value === 'bigint') return {bigint: value.toString()};
  if (typeof value === 'number' && (!Number.isFinite(value) || Object.is(value, -0))) return {number: Object.is(value, -0) ? '-0' : String(value)};
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k, encode(v)]));
  return value;
};
const cases = [];
const initial = {bids:[{price:5,amount:7,total:99}], asks:[{price:8,amount:9,total:Infinity}]};
const snapshot = (sequence='100', bids=[[10,2],[12,3],[10,4],[9,0]], asks=[[15,2],[14,3]]) => ({op:'snapshot', sequence,bids,asks});
const delta = (sequence='101', previousSequence='100', bids=[[12,0],[11,5],[10,6]], asks=[[14,0],[13,8]]) => ({op:'delta',sequence,previousSequence,bids,asks});
function run(name, actions) {
  const book=structuredClone(initial), machine=new SequencedDeltaOrderBook(book), steps=[];
  const record = (action, result) => steps.push(encode({action,result:result ?? null,sequence:machine.currentSequence ?? null,ready:machine.isReady,recovering:machine.isRecovering,book}));
  record({op:'inspect'}, null);
  for(const action of actions) {
    let result;
    if(action.op === 'reset') machine.reset();
    else if(action.op === 'total') book.bids[0].total = action.total;
    else result=action.op==='snapshot' ? machine.applySnapshot(action) : machine.applyDelta(action);
    record(action,result);
  }
  cases.push({name,steps});
}
run('lifecycle', [delta(),delta(),snapshot(),{op:'total',total:Infinity},delta(),delta('101','9',[[0,1]]),delta('99','0',[[0,1]]),delta('103','102'),delta(),snapshot('bad'),snapshot('900719925474099312345678901234567890'),delta('900719925474099312345678901234567891','900719925474099312345678901234567890'),{op:'reset'},delta(),snapshot('1'),snapshot('0'),delta('8','0'),{op:'reset'},snapshot()]);
const sequences=['0','000100','\uFEFF00100\uFEFF','\u0085100','', ' ', '-1','+1','1.0','1e2','0x10','１２','1\n2', '100\n', '9'.repeat(400),0,-0,100,9007199254740991,9007199254740992,-1,1.5,Infinity,-Infinity,NaN,0n,100n,-1n,10n**400n];
for (const [i, sequence] of sequences.entries()) {
  run(`sequence-${i}`, [snapshot(sequence),delta('101',sequence),snapshot(),delta(sequence,'100'),snapshot(),delta('101',sequence)]);
}
const badLevels=[[[0,1]],[[-1,1]],[[Infinity,1]],[[NaN,1]],[[1,-1]],[[1,Infinity]],[[1,NaN]],[[1]], [[]]];
for(const [i,levels] of badLevels.entries())for(const side of ['bids','asks']) {
  const snap={...snapshot(),[side]:[[20,2],...levels]};
  const change={...delta(),[side]:[[20,2],...levels]};
  run(`invalid-${side}-${i}`, [snap,snap,snapshot(),change,change,snapshot(),{...change,sequence:'100'}, {...change,sequence:'99'}, {...change,sequence:'102',previousSequence:'9'},snapshot()]);
}
run('validation-precedence', [snapshot(),delta('100','bad'),snapshot(),delta('99','bad'),snapshot('bad',[[0,0]]),snapshot(),delta('102','9',[[0,0]]),snapshot(),delta('101','100',[[1,2,NaN],[2,-0]],[])]);
// Deterministic mixed valid/invalid transitions, with both sides and repeated prices.
let seed=1837;
const random=()=> (seed=(Math.imul(seed,1664525)+1013904223)>>>0);
const actions=[];
for(let i=0;i<250;i++) {
  const sequence=String(random()%30), previous=String(random()%30);
  const bids=Array.from({length:random()%5},()=>[random()%8,random()%4]);
  const asks=Array.from({length:random()%5},()=>[random()%8,random()%4]);
  const op=random()%7;
  actions.push(op===0?{op:'reset'}:op<3?snapshot(sequence,bids,asks):delta(sequence,previous,bids,asks));
}
run('mixed-transitions',actions);
process.stdout.write(JSON.stringify(cases));
