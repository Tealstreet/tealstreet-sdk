'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),src=path.join(root,'packages/safe-cex/src'),hashes={};
function source(file){const s=fs.readFileSync(path.join(src,file),'utf8');hashes[file]=crypto.createHash('sha256').update(s).digest('hex');return ts.createSourceFile(file,s,ts.ScriptTarget.Latest,true);}
function evaluate(code,bindings={}){const r=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal((r.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);return vm.runInNewContext(r.outputText+';make',{exports:{},...bindings});}
function constant(ast,name,bindings){const d=ast.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(ast)===name);assert.ok(d);return evaluate('function make(){return ('+d.initializer.getText(ast)+')}',bindings)();}
const ast=source('exchanges/orderly/orderly.exchange.ts'),cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='OrderlyExchange');assert.ok(cls);
function body(name,bindings,receiver){const m=cls.members.filter(m=>m.name?.getText(ast)===name);assert.equal(m.length,1);assert.ok(ts.isArrowFunction(m[0].initializer));return evaluate('function make(){return ('+m[0].initializer.getText(ast)+')}',bindings).call(receiver);}
const types=source('types.ts'),enums={};for(const name of ['PositionMode','PositionSide','OrderType','OrderSide','OrderStatus','MarginType']){const d=types.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text===name);assert.ok(d);enums[name]=evaluate(d.getText(types)+';function make(){return '+name+'}')();}
const orderlyTypes=source('exchanges/orderly/orderly.types.ts'),ENDPOINTS=constant(orderlyTypes,'ENDPOINTS'),ORDER_TYPE=constant(orderlyTypes,'ORDER_TYPE',enums),ORDER_SIDE=constant(orderlyTypes,'ORDER_SIDE',enums);
const lodash=name=>require(path.join(root,'node_modules/lodash',name));const util=source('exchanges/orderly/orderly.utils.ts'),normalizeSymbol=constant(util,'normalizeSymbol',{memoize:lodash('memoize')}),reverseSymbol=constant(util,'reverseSymbol',{memoize:lodash('memoize')});const v=constant(source('utils/get-key.ts'),'v',{s:lodash('memoize')(lodash('snakeCase')),c:lodash('memoize')(lodash('camelCase'))});
const math=source('utils/safe-math.ts');const afterDecimal=constant(math,'afterDecimal');const subtract=constant(math,'subtract',{afterDecimal,pow10:n=>10**n});
function capture(v){if(v===undefined)return {undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {number:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}if(Array.isArray(v))return v.map(capture);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,capture(x)]));return v;}
const {defaults,cases}=require('./data-mappers-fixtures.cjs'),rows={};
(async()=>{for(const [name,configs]of Object.entries(cases)){rows[name]=[];for(const raw of configs){const cfg=structuredClone({...defaults,...raw}),input=capture(cfg),events=[],original=cfg.error,cleanup={token:'cleanup'},callback={token:'callback'};let receiver;
 const effect=(kind,details={})=>{events.push({kind,...capture(details)});if(cfg.fail===kind)throw original;};
 const store={markets:cfg.markets,marketsBySymbol:cfg.marketsBySymbol,tickersBySymbol:cfg.tickersBySymbol,tickers:cfg.cachedTickers,positions:cfg.cachedPositions,balance:cfg.balance,update(patch){effect('update',{receiver:this===store,patch});Object.assign(store,patch);}};
 const xhr={get(url,config){effect('request',{receiver:this===xhr,url,config});if(cfg.syncFail)throw original;return(async()=>{await Promise.resolve();effect('await');const change=cfg.afterAwait||{};if(change.marketsBySymbol)store.marketsBySymbol=change.marketsBySymbol;if(change.balance)store.balance=change.balance;if(change.leverage)receiver.accountLeverage=change.leverage;if(cfg.reject)throw original;return {data:{data:{rows:cfg.rows,total_collateral_value:cfg.total,free_collateral:cfg.free}}};})();}};
 const socket={listenOHLCV(options,cb){effect('delegate',{receiver:this===socket,argumentIdentity:options===cfg.options,callbackIdentity:cb===callback});return cleanup;},listenOrderBook(symbol,cb){effect('delegate',{receiver:this===socket,argumentIdentity:symbol===cfg.symbol,callbackIdentity:cb===callback});return cleanup;}};
 receiver={xhr,store,publicWebsocket:socket,isDisposed:cfg.disposed||false,accountLeverage:cfg.leverage,mapMarginType(raw){effect('margin',{raw});return typeof raw==='string'&&raw.toUpperCase()==='ISOLATED'?'isolated':'cross';},getCurrentMarginType(symbol){effect('currentMargin',{symbol});return cfg.margin;},createOrderlyPositionId(symbol,side,mode,margin){effect('id',{symbol,side,mode,margin});return `${symbol}:${side}:${mode}:${margin}`;},getPositionMode(){effect('mode');return Promise.resolve(cfg.mode);},emitAndLogError(message){effect('log',{message});}};
 const bindings={...enums,ENDPOINTS,ORDER_TYPE,ORDER_SIDE,normalizeSymbol,reverseSymbol,v,subtract,uniqBy:lodash('uniqBy'),cloneDeep(value){effect('clone');return lodash('cloneDeep')(value);},virtualClock:{getCurrentTimeMs(){effect('clock');return 1723638896125.5;}}};
 const mapper=body('mapPosition',bindings,receiver);receiver.mapPosition=function(p,mode){effect('mapPosition',{rowIdentity:cfg.rows.includes(p),modeIdentity:mode===cfg.mode});return cfg.integrate?mapper(p,mode):cfg.mapped===null?null:{id:`BTCUSDC:long:${mode}:cross`,opaque:'position'};};
 const fn=body(name,bindings,receiver);let result,error;try{const args=name==='mapPosition'?[cfg.position,cfg.mode]:name==='mapLimitOrder'?[cfg.limit]:name==='mapAlgoOrder'?[cfg.algo]:name==='fetchOHLCV'?[cfg.options]:name==='listenOHLCV'?[cfg.options,callback]:name==='listenOrderBook'?[cfg.symbol,callback]:[];result=await fn(...args);}catch(e){error={identity:e===original,message:capture(e.message)};}
 const cloned=(info,original)=>info!==original&&Object.entries(original).every(([k,v])=>!v||typeof v!=='object'||info[k]!==v);
 let outputIdentity=true;
 if(!error){
  if(name.startsWith('listen'))outputIdentity=result===cleanup;
  else if(name==='mapLimitOrder'&&result)outputIdentity=result.info===cfg.limit;
  else if(name==='fetchTickers')outputIdentity=result===store.tickers||result.every(t=>cfg.rows.includes(t.info));
  else if(name==='fetchPositions'&&(cfg.reject||cfg.syncFail||cfg.fail))outputIdentity=result===store.positions;
  else if(name==='fetchMarkets')outputIdentity=result.every(m=>cfg.rows.every(r=>cloned(m.info,r)));
  else if(name==='mapPosition'&&result)outputIdentity=cloned(result.info,cfg.position);
  else if(name==='mapAlgoOrder'){const originals=cfg.algo.childOrders||cfg.algo.child_orders||[cfg.algo];outputIdentity=result.every(m=>originals.every(r=>cloned(m.info,r)));}
 }

 rows[name].push({cfg:input,result:capture(result),error,outputIdentity,events,state:{balance:capture(store.balance)}});
}}process.stdout.write(JSON.stringify({hashes,rows}));})().catch(e=>{console.error(e);process.exitCode=1});
