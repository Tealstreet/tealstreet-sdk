// Execute maintained control and mapper bodies. Only I/O and outer publication are scripted.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = file => fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
const compilerOptions = {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS};
function moduleFrom(file, dependencies = {}) {
  const exports = {};
  vm.runInNewContext(ts.transpileModule(read(file), {compilerOptions}).outputText, {
    exports, require(name) {assert(Object.hasOwn(dependencies, name), `${file}: ${name}`); return dependencies[name];},
  });
  return exports;
}
const ast = file => ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true);
const context = vm.createContext({});
const evaluate = source => vm.runInContext(ts.transpileModule(source, {compilerOptions}).outputText, context);
const types = ast('types.ts');
for (const node of types.statements.filter(ts.isEnumDeclaration)) evaluate(node.getText(types).replace(/^export\s+/, ''));
const enums = vm.runInContext('({PositionSide,PositionMode,MarginType})', context);
const {v} = moduleFrom('utils/get-key.ts', Object.fromEntries(['camelCase','memoize','snakeCase'].map(n => [`lodash/${n}`, {__esModule:true,default:require(`lodash/${n}`)}])));
const inverse = moduleFrom('exchanges/bybit/bybit.inverse.ts');
const option = moduleFrom('exchanges/bybit/bybit.options.ts');
const {createPositionId} = moduleFrom('utils/createPositionId.ts', {'../types':enums});
Object.assign(context, {v, ...inverse, ...option, createPositionId,
  ...moduleFrom('exchanges/bybit/bybit.response.ts'), virtualClock:{getCurrentTimeMs:()=>1700000000000}});
const exchange = ast('exchanges/bybit/bybit.exchange.ts');
const tables = ast('exchanges/bybit/bybit.types.ts');
function declaration(tree, name) {
  const nodes = tree.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations]).filter(n => n.name.getText(tree) === name);
  assert.equal(nodes.length,1,name); return `const ${nodes[0].getText(tree)};`;
}
for (const name of ['ENDPOINTS','POSITION_SIDE']) evaluate(declaration(tables,name));
for (const name of ['BYBIT_LEVERAGE_NOT_MODIFIED_CODE','isBybitLeverageNotModified']) evaluate(declaration(exchange,name));
const cls = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BybitExchange');
const selected = ['setLeverage','fetchPositionMode','mapPosition','refuseOptionSymbol','getCategoryForSymbol','optionsEnabled','inverseEnabled'];
evaluate(`globalThis.Subject = class {${selected.map(name => {
  const nodes=cls.members.filter(n=>n.name?.getText(exchange)===name); assert.equal(nodes.length,1,name); return nodes[0].getText(exchange);
}).join('\n')}};`);
const number = n => Number.isNaN(n) ? 'NaN' : Object.is(n,-0) ? '-0' : String(n);
(async () => {
  const cases = JSON.parse(fs.readFileSync(0,'utf8'));
  assert.equal(cases.length,32,'fixed native control case count');
  const outputs=[];
  for (const input of cases) {
    const target=new context.Subject(), requests=[], errors=[];
    target.name=input.variant; target.options={}; target.marginType=enums.MarginType.Cross;
    target.store={marketsBySymbol:{BTCUSDT:input.market},tickersBySymbol:{},positions:[],addOrUpdatePositions(rows) {
      for(const row of rows) {const index=this.positions.findIndex(p=>p.id===row.id); if(index<0)this.positions.push(row);else this.positions[index]=row;}
    }};
    if(input.cached) target.store.addOrUpdatePositions([target.mapPosition(input.row)]);
    target.fetchPositions=async()=>[target.mapPosition(input.row)];
    target.emitAndLogError=message=>errors.push(message);
    target.xhr={get:async()=>({data:{retCode:0,retMsg:'OK',result:{list:[input.row]}}}),post:async(endpoint,payload)=>{
      requests.push({endpoint,payload}); return {data:{retCode:0,retMsg:'OK'}};
    }};
    let mode;
    if(input.kind==='leverage') await target.setLeverage('BTCUSDT',1);
    else mode=await target.fetchPositionMode('BTCUSDT');
    outputs.push({posts:requests,errors,mode:mode??null,positions:target.store.positions.map(p=>({symbol:p.symbol,mode:p.positionMode,leverage:number(p.leverage)}))});
  }
  process.stdout.write(JSON.stringify(outputs));
})().catch(error=>{console.error(error);process.exitCode=1});
