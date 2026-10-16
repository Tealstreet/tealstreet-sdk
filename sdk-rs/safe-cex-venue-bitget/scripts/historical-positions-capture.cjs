// Live delegation and shared-fold consumer capture; provenance accompanies each run.
const fs = require('node:fs'), path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
function source(file) { return ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true); }
function member(file, name, field) {
  const klass = file.statements.find(n => ts.isClassDeclaration(n) && n.name.text === name);
  const result = klass?.members.find(n => n.name?.getText(file) === field);
  if (!result) throw Error(`missing source ${name}.${field}`);
  return result.getText(file);
}
const venue = source('packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const base = source('packages/safe-cex/src/exchanges/base.ts');
const names = ['baseCapabilityFallback', 'isBaseCapabilityFallback', 'isEmptyStubCapability', 'isInertCapability'];
const helpers = names.map(name => {
  const node = base.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(base) === name));
  if (!node) throw Error(`missing capability ${name}`);
  return node.getText(base);
}).join('\n');
const code = helpers + '\nexport class BitgetExchange{' + member(base, 'BaseExchange', 'foldClosedPositionRecords') + '\n' + member(venue, 'BitgetExchange', 'fetchHistoricalPositions') + '}';
function number(n) { const b = Buffer.alloc(8); b.writeDoubleBE(n); return {bits: Number.isNaN(n) ? '7ff8000000000000' : b.toString('hex')}; }
const exportsObject = {};
let builder;
new Function('exports', 'buildFoldedPositionRecords', ts.transpileModule(code, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText)(exportsObject, (...args) => builder(...args));
(async () => {
  const cases = [], numbers = [undefined, -0, 0, 12.5, -7.25, Infinity, -Infinity, NaN];
  for (let i = 0; i < numbers.length; i++) for (let j = 0; j < numbers.length; j++) {
    const opts = {account: `account:${i}:${j}`, from: numbers[i], to: numbers[j]};
    const input = {account:opts.account, from:i ? number(opts.from) : null, to:j ? number(opts.to) : null};
    const exchange = new exportsObject.BitgetExchange();
    exchange.fetchHistoricalFills = async () => [{id: 'implemented'}];
    const records = [{id:'record-A', amount:17}, {id:'record-B', amount:29}];
    let args, sameVenue;
    builder = async (passedVenue, passed) => { sameVenue = passedVenue === exchange; args = {account:passed.account, from:number(passed.from), to:number(passed.to), symbolScoped:passed.symbolScoped}; return {records}; };
    const previous = Date.now; Date.now = () => 123000;
    let result; try { result = await exchange.fetchHistoricalPositions(opts); } finally { Date.now = previous; }
    cases.push({name:`${i}:${j}`, ...input, args, sameVenue, sameRecords:result === records, result});
  }
  const exchange = new exportsObject.BitgetExchange();
  const options = {account:'forwarded', from:17, to:29}; let forwarded;
  exchange.foldClosedPositionRecords = async (opts, config) => {forwarded = {sameOptions:opts === options, symbolScoped:config.symbolScoped}; return [];};
  await exchange.fetchHistoricalPositions(options);
  const failure = new Error('original folder failure');
  exchange.foldClosedPositionRecords = async () => {throw failure;};
  let sameError = false; try { await exchange.fetchHistoricalPositions(options); } catch (e) {sameError = e === failure;}
  const records = [{id:'alias'}]; exchange.foldClosedPositionRecords = async () => records;
  const returned = await exchange.fetchHistoricalPositions(options); records[0].id='mutated'; returned.push({id:'added'});
  const aliases = {same:returned === records, callerSees:returned[0].id, producerSees:records.length};
  const openRecords = [{info:() => 17, missing:undefined}];
  exchange.foldClosedPositionRecords = async () => openRecords;
  const openResult = await exchange.fetchHistoricalPositions(options);
  const openOutput = {same:openResult === openRecords, callable:typeof openResult[0].info, ownsMissing:Object.hasOwn(openResult[0], 'missing'), serialized:JSON.stringify(openResult)};
  const surrogate = '\ud800'; let accepted;
  exchange.foldClosedPositionRecords = async opts => {accepted=opts.account;return [];}; await exchange.fetchHistoricalPositions({account:surrogate});
  process.stdout.write(JSON.stringify({capture:{platform:process.platform,architecture:process.arch,node:process.version,nodeMajor:process.versions.node.split('.')[0]}, cases, forwarded, sameError, aliases, openOutput, utf16:{input:JSON.stringify(surrogate),output:JSON.stringify(accepted)}})+'\n');
})().catch(e => {console.error(e);process.exitCode=1;});
