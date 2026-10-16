const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler), { bits } = require('../../safe-cex-num/tests/support/number-bits.cjs');
const root = path.resolve(__dirname, '../../../packages');
const file = path.join(root, 'safe-cex/src/exchanges/grvt/grvt.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'GrvtExchange');
const names = ['recordNonce', 'getPositionMode', 'calculatePositionUpnlFromLastPrice', 'calculateUpnlFromPositions', 'syncBalanceUpnlFromPositions', 'getFakePositions', 'fetchTickers', 'dispose', 'fetchOrders', 'getAccount'];
const members = names.map(name => { const m = cls.members.find(n => n.name?.getText(tree) === name); assert(m, 'actual member ' + name); return m.getText(tree); });
const maximum = cls.members.find(n => n.name?.getText(tree) === 'MAX_NONCE_MAPPINGS');
assert.equal(maximum.initializer.getText(tree), '200', 'independent nonce retention limit');
const shared = ts.createSourceFile('types.ts', fs.readFileSync(path.join(root, 'safe-cex/src/types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
const enums = ['PositionMode', 'PositionSide', 'MarginType'].map(name => shared.statements.find(n => ts.isEnumDeclaration(n) && n.name.text === name).getText(shared));
const utilFile = path.join(root, 'safe-cex/src/utils/createPositionId.ts');
const util = ts.createSourceFile(utilFile, fs.readFileSync(utilFile, 'utf8'), ts.ScriptTarget.Latest, true);
const utilDeclaration = util.statements.find(ts.isVariableStatement).getText(util);
const logging = fs.readFileSync(path.join(root, 'logging/src/LogSeverity.ts'), 'utf8');
const text = [...enums, logging, utilDeclaration, 'export class GrvtExchange {', maximum.getText(tree), ...members, '}'].join('\n');
const context = {exports: {}, ENDPOINTS: {ACCOUNT_SUMMARY:'/lite/v1/account_summary'}};
vm.runInNewContext(ts.transpileModule(text, {compilerOptions:{module:ts.ModuleKind.CommonJS, target:ts.ScriptTarget.ES2022}}).outputText, context);
const Venue = context.exports.GrvtExchange;
function tag(v) {
  if (typeof v === 'number') return {bits:bits(v)};
  if (v === undefined) return {undefined:true};
  if (Array.isArray(v)) return v.map(tag);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,tag(x)]));
  return v;
}
function p(symbol, side='long', contracts=2, entryPrice=10, unrealizedPnl=7) {
  return {id:symbol, symbol, side, contracts, entryPrice, unrealizedPnl, notional:0, leverage:10, liquidationPrice:0};
}
function market(id, symbol) { return {id,symbol,base:'B',quote:'Q',active:true,precision:{amount:0.01,price:0.5},limits:{amount:{min:0.01,max:99},leverage:{min:1,max:100}}}; }
function store(input) {
  const s = {positions:input.positions ?? [], markets:input.markets ?? [], tickers:input.tickers ?? [], fetchedLeverage:input.fetchedLeverage ?? {}, balance:{free:31,used:7,total:38,upnl:input.upnl ?? 0, ...(input.balance ?? {})}};
  s.tickersBySymbol = {};
  for (const t of s.tickers) s.tickersBySymbol[t.symbol]=t;
  const effects=[];
  s.updatePositions = updates => { effects.push(['positions', tag(updates)]); for(const [which,patch] of updates) for(const v of s.positions) if(v.id===which.id)Object.assign(v,patch); };
  s.update = update => { effects.push(['balance',tag(update.balance)]); Object.assign(s,update); };
  return {s,effects};
}
(async()=>{
  const nonce = new Venue(); nonce._nonceCreationTimes=new Map(); nonce._nonceToOrderId=new Map();
  const nonceInputs=[['same','0xabc',null],['same','0x00',100],['same','0x0',120],['same','',90],['same','0xdef',110],['unknown','0x0',0]];
  for(let i=0;i<203;i++)nonceInputs.push(['n'+i,'0x'+i,i+1]);
  const nonceRows=[];
  for(const [id,real,time] of nonceInputs) {
    nonce.recordNonce(id,real,time===null?undefined:time);
    nonceRows.push({input:[id,real,time],times:[...nonce._nonceCreationTimes].map(([k,v])=>[k,tag(v)]),ids:[...nonce._nonceToOrderId]});
  }
  assert.equal(nonceRows.length,209,'nonce floor');
  const pnlRows=[];
  const values=['0','-0','-1','1','12','1e400','-1e400','-1e-400'];
  for(const last of values) for(const side of ['long','short']) {
    const input={positions:[p('A',side)],tickers:[{id:'A',symbol:'A',bid:0,ask:0,last:JSON.parse(last),mark:0,index:0,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0}]};
    // Construct raw token separately so overflow and signed zero are not re-encoded.
    const rawInput=JSON.stringify(input).replace('"last":'+JSON.stringify(input.tickers[0].last),'"last":'+last);
    const parsed=JSON.parse(rawInput), {s,effects}=store(parsed), v=new Venue(); v.store=s;
    const derived=v.calculatePositionUpnlFromLastPrice(s.positions[0]);
    const total=v.calculateUpnlFromPositions(s.positions);
    v.syncBalanceUpnlFromPositions();
    pnlRows.push({raw:rawInput,derived:tag(derived),total:tag(total),positions:tag(s.positions),balance:tag(s.balance),effects});
  }
  for (const input of [
    {positions:[p('MISSING','short',-3,8,19)]},
    {positions:[p('A','long',0,10,9),p('B','short',2,8,13)]},
    {positions:[p('A','long',1,10,3),p('B','short',-2,8,4)],tickers:[{id:'A',symbol:'A',last:14,bid:0,ask:0,mark:0,index:0,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0},{id:'B',symbol:'B',last:9,bid:0,ask:0,mark:0,index:0,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0}]},
  ]) {
    const {s,effects}=store(input),v=new Venue();v.store=s;
    const derived=v.calculatePositionUpnlFromLastPrice(s.positions[0]),total=v.calculateUpnlFromPositions(s.positions);
    const raw=JSON.stringify(input); v.syncBalanceUpnlFromPositions();
    pnlRows.push({raw,derived:tag(derived),total:tag(total),positions:tag(s.positions),balance:tag(s.balance),effects});
  }
  assert.equal(pnlRows.length,19,'pnl floor');
  const fakeRows=[];
  for (const input of [
    {markets:[]},
    {markets:[market('a','A'),market('b','B')],positions:[{...p('A','long'),id:'A-old'},{...p('A','short'),id:'A-last'}],fetchedLeverage:{A:{buyLeverage:3,sellLeverage:7},B:{buyLeverage:5,sellLeverage:9,isFetching:true}}},
    {markets:[market('c','C')],fetchedLeverage:{C:{buyLeverage:0,sellLeverage:7,isFetching:false}}},
    {markets:[market('p','__proto__')]},
  ]) { const {s}=store(input),v=new Venue();v.store=s;v.defaultLeverage=10;fakeRows.push({raw:JSON.stringify(input),expected:tag(v.getFakePositions())}); }
  const tickerRows=[];
  for(const count of [0,2]) for(const failAt of [0,1,2]) {
    const input={markets:count===0?[]:[market('a','A'),market('b','B')]}, {s}=store(input),v=new Venue();v.store=s;
    const effects=[];v.log=(message,severity)=>{effects.push([message,severity]);if(effects.length===1&&failAt>0||effects.length===2&&failAt===2)throw Error('log-'+effects.length);};
    let result=null,error=null;try{result=tag(await v.fetchTickers());}catch(e){error={name:e.name,message:e.message};}
    tickerRows.push({raw:JSON.stringify(input),failAt,result,error,effects});
  }
  const disposeRows=[];
  for(const fail of ['', 'public', 'private']) {const v=new Venue(),effects=[];v.publicWebsocket={dispose(){effects.push(['public',v.isDisposed]);if(fail==='public')throw Error('public');}};v.privateWebsocket={dispose(){effects.push(['private',v.isDisposed]);if(fail==='private')throw Error('private');}};let error=null;try{v.dispose();}catch(e){error=e.message;}disposeRows.push({fail,disposed:v.isDisposed,effects,error});}
  const ordersRows=[];
  const order={id:'o1',symbol:'A',status:'open',type:'limit',side:'buy',price:7,amount:3,filled:1,remaining:2,reduceOnly:false,lastUpdate:123};
  for(const symbols of [undefined,[],['A'],['A','B']]) for(const fail of [false,true]) {
    const v=new Venue(),effects=[];v.fetchOpenOrders=async(...args)=>{effects.push(args);if(fail)throw TypeError('orders');return [order];};
    let result=null,error=null;try{result=tag(await v.fetchOrders(symbols));}catch(e){error={name:e.name,message:e.message};}
    ordersRows.push({symbols:symbols??null,effects,fail,result,error});
  }
  assert.equal(ordersRows.length,8,'orders floor');
  const accountRows=[];
  for(const sub of [undefined,'','s1']) for(const account of [undefined,'','user']) for(const fail of [false,true]) {const v=new Venue(),effects=[];v.apiExtras={subAccountId:sub,accountId:account};v.tradingXhr={async post(url,body){effects.push(['post',url,body]);if(fail)throw Error('request');return {data:{}};}};v.log=(message,severity)=>effects.push(['log',message,severity]);let result=null,error=null;try{result=await v.getAccount();}catch(e){error=e.message;}accountRows.push({sub:sub??null,account:account??null,fail,result,error,effects});}
  {
    const v=new Venue(),effects=[];v.apiExtras={accountId:'before'};
    v.tradingXhr={async post(url,body){effects.push(['post',url,body]);await Promise.resolve();v.apiExtras.accountId='after';return {data:{}};}};
    v.log=(message,severity)=>effects.push(['log',message,severity]);
    accountRows.push({sub:null,account:'before',lateAccount:'after',fail:false,result:await v.getAccount(),error:null,effects});
  }
  assert.equal(accountRows.length,19,'account floor');
  process.stdout.write(JSON.stringify({mode:await(new Venue()).getPositionMode(),nonceRows,pnlRows,fakeRows,tickerRows,disposeRows,ordersRows,accountRows}));
})().catch(error=>{console.error(error);process.exitCode=1;});
