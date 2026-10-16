// Execute maintained message, mapping, fill, category and Store bodies; only host I/O is scripted.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = file => fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
const compilerOptions = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS };
function moduleFrom(file, dependencies = {}) {
  const exports = {};
  vm.runInNewContext(ts.transpileModule(read(file), { compilerOptions }).outputText, {
    exports, require(name) { assert(Object.hasOwn(dependencies, name), `${file}: ${name}`); return dependencies[name]; },
  });
  return exports;
}
const clock = {getCurrentTimeMs: () => 1700000000000};
const context = vm.createContext({});
function evaluate(source) { vm.runInContext(ts.transpileModule(source, {compilerOptions}).outputText, context); }
const types = ts.createSourceFile('types.ts', read('types.ts'), ts.ScriptTarget.Latest, true);
for (const node of types.statements.filter(ts.isEnumDeclaration)) evaluate(node.getText(types).replace(/^export\s+/, ''));
const enums = vm.runInContext('({OrderType,OrderSide,OrderStatus,PositionSide,PositionMode,MarginType})', context);
const {v} = moduleFrom('utils/get-key.ts', Object.fromEntries(['camelCase','memoize','snakeCase'].map(n => [`lodash/${n}`, {__esModule:true,default:require(`lodash/${n}`)}])));
const inverse = moduleFrom('exchanges/bybit/bybit.inverse.ts');
const option = moduleFrom('exchanges/bybit/bybit.options.ts');
const {createPositionId} = moduleFrom('utils/createPositionId.ts', {'../types':enums});
const {syncUnifiedBalanceUpnlFromPositions} = moduleFrom('exchanges/bybit/syncUnifiedBalanceUpnlFromPositions.ts', {'./bybit.inverse': inverse});
const fills = moduleFrom('utils/incrementalFills.ts');
const {resolveBybitExecution} = moduleFrom('exchanges/bybit/bybit.fill-event.ts', {'../../utils/get-key':{v}});
const {clone} = moduleFrom('utils/clone.ts');
const {DefaultStore} = moduleFrom('store/store.base.ts', {
  'lodash/uniqBy':{__esModule:true,default:require('lodash/uniqBy')}, '../types':enums,
  '../utils/clone':{clone}, '../utils/virtualClock':{virtualClock:clock},
});
Object.assign(context, {v, ...inverse, ...option, ...fills, createPositionId, syncUnifiedBalanceUpnlFromPositions,
  resolveBybitExecution, virtualClock:clock, subtract:require('lodash/subtract'), jsonParse(s) {try{return JSON.parse(s)}catch{return null}} });
function ast(file) { return ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true); }
const tables = ast('exchanges/bybit/bybit.types.ts');
const declarations = a => a.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations]);
for (const name of ['ORDER_TYPE','ORDER_STATUS','ORDER_SIDE','ORDER_TIME_IN_FORCE','ORDER_TRIGGER_TYPE','POSITION_SIDE','POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE']) {
  const node = declarations(tables).find(n => n.name.getText(tables) === name); assert(node,name); evaluate(`const ${node.getText(tables)};`);
}
const exchange = ast('exchanges/bybit/bybit.exchange.ts');
for (const name of ['BYBIT_POSITION_TPSL_MODES','isBybitPositionTpslMode']) {
  const node = declarations(exchange).find(n => n.name.getText(exchange) === name); assert(node,name); evaluate(`const ${node.getText(exchange)};`);
}
function selected(a, className, names) {
  const cls = a.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === className); assert(cls,className);
  return names.map(name => { const rows = cls.members.filter(n => n.name?.getText(a) === name); assert.equal(rows.length,1,name); return rows[0].getText(a); }).join('\n');
}
evaluate(`globalThis.Parent = class {${selected(exchange,'BybitExchange',[
  'getCategoryForSymbol','tradesCategoryOf','optionsEnabled','inverseEnabled','remainingInStoreUnits',
  'inferProtectiveOrderSide','mapOrder','mapPosition','syncBalanceUpnlFromPositions','accountType'
])}};`);
const socket = ast('exchanges/bybit/bybit.ws-private.ts');
evaluate(`globalThis.Subject = class {${selected(socket,'BybitPrivateWebsocket',[
  'incrementalFills','onMessage','rowsThisConnectionTrades','handleOrderTopic','handlePositionTopic'
])}};`);
const spotExchange = ast('exchanges/bybit/bybitspot.exchange.ts');
evaluate(`globalThis.SpotParent = class {${selected(spotExchange,'BybitSpotExchange',['mapOrder'])}};`);
const spotSocket = ast('exchanges/bybit/bybitspot.ws-private.ts');
evaluate(`globalThis.SpotSubject = class {${selected(spotSocket,'BybitPrivateWebsocket',['incrementalFills','onMessage','handleOrderTopic'])}};`);
const orders = rows => rows.map(({id,status,symbol,type,side,filled,remaining,amount}) => ({id,status,symbol,type,side,filled,remaining,amount}));
const positions = rows => rows.map(({id,symbol,contracts,notional,unrealizedPnl,realizedPnl,leverage,marginSize,marginType,positionMode,option}) => ({id,symbol,contracts,notional,unrealizedPnl,realizedPnl,leverage,marginSize,marginType,positionMode,option}));
const output = JSON.parse(fs.readFileSync(0,'utf8')).map(input => {
  const parent = input.variant === "spot" ? new context.SpotParent() : new context.Parent();
  const owner = input.variant === "spot" ? new context.SpotSubject() : new context.Subject();
  const store = new DefaultStore();
  parent.options = input.options; parent.unifiedMarginStatus=6; parent.marginType=enums.MarginType.Cross; parent.store=store;
  store.update({markets:input.markets, balance:{used:0,free:1000,total:1000,upnl:0}});
  owner.parent=parent; owner.store=store; owner.isDisposed=false; owner.authRetries=0;
  const fillEvents=[];
  owner.emitter={emit:(_,event) => fillEvents.push(event)};
  owner.directFallback={onMessage(){}};
  owner.trackLastReceived=owner.emitWsDiagnosticReceive=owner.recordMessageHandlerStats=owner.emitAndLogError=owner.handlePongEvent=owner.sendReconnectSequence=()=>{};
  parent.markOrderSeen=()=>{};
  const snapshots=[];
  for (const frame of input.frames) {
    owner.onMessage({data:typeof frame === 'string' ? frame : JSON.stringify(frame)});
    snapshots.push({orders:orders(store.orders), positions:positions(store.positions), upnl:store.balance.upnl, fills:[...fillEvents]});
  }
  return snapshots;
});
process.stdout.write(JSON.stringify(output));
