const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni/apexomni.exchange.ts');
let source = fs.readFileSync(filename, 'utf8');
const mutations = {
  tpsl: ["side === 'BUY' ? 1.005 : 0.995", "side === 'BUY' ? 0.995 : 1.005"],
  d8: ["const n = '123456789'", "const n = '987654321'"],
  client: ["`apexomni-${accountId}-${Date.now()}-${d8(6)}`", "`MUTATED-${accountId}-${Date.now()}-${d8(6)}`"],
  epoch: ['Math.ceil(new Date(Number(isoTimestamp)).getTime() / ONE_HOUR_MS)', 'Math.floor(new Date(Number(isoTimestamp)).getTime() / ONE_HOUR_MS)'],
  buffer: ['expirationEpochHours + STARK_ORDER_SIGNATURE_EXPIRATION_BUFFER_HOURS', 'expirationEpochHours - STARK_ORDER_SIGNATURE_EXPIRATION_BUFFER_HOURS'],
};
if (process.env.APEXOMNI_EXCHANGE_UTILS_MUTATE_TS) {
  const [from,to] = mutations[process.env.APEXOMNI_EXCHANGE_UTILS_MUTATE_TS];
  assert.equal(source.split(from).length, 2, 'actual source mutation applies exactly once');
  source = source.replace(from,to);
}
const tree = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
const wanted = new Set(['ONE_HOUR_MS','STARK_ORDER_SIGNATURE_EXPIRATION_BUFFER_HOURS','isoTimestampToEpochHours','addOrderExpirationBufferHours','d8','generateApeXRandomClientId','calculateTpSlPrice']);
const selected = [];
for (const node of tree.statements) {
  const names = ts.isFunctionDeclaration(node) ? [node.name?.text] : ts.isVariableStatement(node) ? node.declarationList.declarations.map(d=>d.name.getText(tree)) : [];
  if (!names.some(name=>wanted.has(name))) continue;
  assert(names.every(name=>wanted.delete(name)), 'no unexpected or duplicate declaration in actual source slice');
  selected.push(node.getText(tree));
}
assert.equal(wanted.size,0,'all actual helper declarations captured');
assert.equal(selected.length,7,'helper declaration census');
const compiled=ts.transpileModule(selected.join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
assert(!compiled.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error),'actual helper transpilation');
function sourceModule(file) {
  const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  const module={exports:{}};
  new vm.Script(`(function(require,module,exports){${code}\n})`,{filename:file}).runInNewContext()(
    name=>{assert.equal(name,'@tealstreet/math/src/isNotNumber');return sourceModule(path.resolve(__dirname,'../../../packages/math/src/isNotNumber.ts'));},module,module.exports);
  return module.exports;
}
const sharedMath=sourceModule(path.resolve(__dirname,'../../../packages/safe-cex/src/utils/safe-math.ts'));
let draws = [0], drawCount = 0, now = 0, trace = [];
const hostMath = Object.create(Math);
hostMath.random = () => { trace.push('random'); return draws[drawCount++ % draws.length]; };
class HostDate extends Date { static now() { trace.push('now'); return now; } }
const api=new vm.Script(compiled.outputText+'\n({isoTimestampToEpochHours,addOrderExpirationBufferHours,d8,generateApeXRandomClientId,calculateTpSlPrice})',{filename}).runInNewContext({Math:hostMath,Date:HostDate,adjust:sharedMath.adjust});
function bits(value) { assert.equal(typeof value,'number'); const b=Buffer.alloc(8);b.writeDoubleBE(value);return Number.isNaN(value)?'7ff8000000000000':b.toString('hex'); }
const epochInputs=['',' ','\ufeff','0','-0','1','-1','0.25','-0.25','3599999','3600000','3600001','-3599999','-3600000','-3600001','1700000000123','8640000000000000','8640000000000001','-8640000000000000','-8640000000000001','NaN','Infinity','-Infinity','1e400','1e-400','-1e-400','0x10','0b11','0o17','1_000','2026-10-13T00:00:00Z','１２','\n3600001\t','1.2.3'];
const epoch=epochInputs.map(input=>({input,inputBits:bits(Number(input)),outputBits:bits(api.isoTimestampToEpochHours(input))}));
const bufferInputs=[0,-0,1,-1,0.25,-0.25,168,-168,1000000,Number.MAX_SAFE_INTEGER,Number.MAX_VALUE,Number.MIN_VALUE,NaN,-NaN,Infinity,-Infinity];
const buffer=bufferInputs.map(input=>({inputBits:bits(input),outputBits:bits(api.addOrderExpirationBufferHours(input))}));
// Separately labelled client-ID supplement; prior epoch/buffer values stay unchanged.
const patterns = [[0],[0.5],[0.9999999999999999],[0,0.1,0.2,0.3,0.4,0.9],[NaN],[1],[-0.1],[Infinity],[NaN,0],[0,NaN],[NaN,NaN,0]];
const ids = [];
for (const length of [undefined,NaN,-1,0,0.5,1,2,2.5,6,16]) {
  for (const alphabet of [undefined,'hex','HEX','D']) {
    for (const pattern of patterns) {
      draws=pattern;drawCount=0;trace=[];
      const output=api.d8(length,alphabet);
      assert(['undefined','string'].includes(typeof output) || Number.isNaN(output));
      ids.push({lengthBits:length===undefined?null:bits(length),alphabet:alphabet??null,drawBits:pattern.map(bits),output:typeof output==='string'?output:null,outputBits:typeof output==='number'?bits(output):null,type:typeof output,calls:drawCount,trace});
    }
  }
}
const clients=[];
for (const account of ['', '0','42','α😀','tenant-id','line\nbreak']) {
  for (const clock of [0,-0,1700000000123,1e21,NaN,Infinity]) {
    for (const pattern of patterns) {
      draws=pattern;drawCount=0;trace=[];now=clock;
      const output=api.generateApeXRandomClientId(account);
      assert.equal(typeof output,'string');
      clients.push({account,clockBits:bits(clock),drawBits:pattern.map(bits),output,calls:drawCount,trace});
    }
  }
}
const brackets=[];
for (const side of ['BUY','SELL','buy','','OTHER','😀']) {
  for (const price of [0,-0,1,-1,12.01,12.15,12.34,99.5,501.025,1e-7,1e20,Number.MAX_VALUE,Number.MIN_VALUE,NaN,Infinity,-Infinity]) {
    for (const step of [0.01,0.1,0.25,1,5,1e-8,0,-0,-0.1,NaN]) {
      const output=api.calculateTpSlPrice({side,marketPrice:price,priceStep:step});
      brackets.push({side,priceBits:bits(price),stepBits:bits(step),outputBits:bits(output),outputType:typeof output});
    }
  }
}
assert.equal(brackets.length,960,'source bracket corpus total');
console.log(JSON.stringify({brackets,header:{node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH},epoch,buffer,ids,clients}));
