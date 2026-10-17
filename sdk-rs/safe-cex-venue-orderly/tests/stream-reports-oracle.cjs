'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),src=path.join(root,'packages/safe-cex/src'),hashes={};
function source(file){const s=fs.readFileSync(path.join(src,file),'utf8');hashes[file]=crypto.createHash('sha256').update(s).digest('hex');return ts.createSourceFile(file,s,ts.ScriptTarget.Latest,true);}
const asts={public:source('exchanges/orderly/orderly.ws-public.ts'),private:source('exchanges/orderly/orderly.ws-private.ts')};
function evaluate(code,bindings={}){const r=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal((r.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);return vm.runInNewContext(r.outputText+';make',{exports:{},...bindings});}
function constant(ast,name,bindings){const d=ast.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(ast)===name);assert.ok(d);return evaluate('function make(){return ('+d.initializer.getText(ast)+')}',bindings)();}
const lodash=name=>require('lodash/'+name);
const v=constant(source('utils/get-key.ts'),'v',{s:lodash('memoize')(lodash('snakeCase')),c:lodash('memoize')(lodash('camelCase'))});
const normalizeSymbol=constant(source('exchanges/orderly/orderly.utils.ts'),'normalizeSymbol',{memoize:lodash('memoize')});
const types=source('types.ts');const enums={};for(const name of ['OrderStatus','OrderSide']){const d=types.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text===name);assert.ok(d);enums[name]=evaluate(d.getText(types)+';function make(){return '+name+';}')();}
const ORDER_SIDE=constant(source('exchanges/orderly/orderly.types.ts'),'ORDER_SIDE',enums);
function body(side,name,bindings,receiver){const ast=asts[side],cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===(side==='public'?'OrderlyPublicWebsocket':'OrderlyPrivateWebSocket'));assert.ok(cls);const member=cls.members.filter(m=>m.name?.getText(ast)===name);assert.equal(member.length,1);assert.ok(ts.isArrowFunction(member[0].initializer));return evaluate('function make(){return ('+member[0].initializer.getText(ast)+')}',bindings).call(receiver);}
function capture(v){if(v===undefined)return {undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {number:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}if(Array.isArray(v))return v.map(capture);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,capture(x)]));return v;}
const {defaults,cases}=require('./stream-reports-fixtures.cjs'),rows={};
for(const [name,configs]of Object.entries(cases)){rows[name]=[];for(const raw of configs){const cfg=structuredClone({...defaults,...raw}),input=capture(cfg),events=[],original=new Error('controlled failure');if(cfg.removeCamel)for(const row of cfg.data){delete row.fundingRate;delete row.fundingTs;}const mappedOrders=cfg.mappedOrders??[cfg.mapped,{...cfg.mapped,id:'SECOND'}];let appended=false;
 const effect=(kind,details={})=>{events.push({kind,...capture(details)});if(cfg.fail===kind)throw original;};
 const rawRows=()=>Array.isArray(cfg.data)?cfg.data:cfg.data?.positions??[cfg.data,cfg.parsed?.data];
 const mutate=()=>{if(cfg.append&&!appended){appended=true;if(Array.isArray(cfg.data))cfg.data.push(cfg.data[0]);else if(cfg.data?.positions)cfg.data.positions.push({id:'appended'});}};
 const lookup={tickersBySymbol:cfg.tickers};
 const store={updateTicker(ticker,patch){effect('updateTicker',{store:this===store,identity:Object.values(lookup.tickersBySymbol).includes(ticker),patch,keys:Object.keys(patch)});Object.assign(ticker,patch);mutate();},addOrUpdatePosition(position){effect('addPosition',{store:this===store,identity:position===cfg.mapped,position});},addOrUpdateOrder(order){effect('addOrder',{store:this===store,identity:order===cfg.mapped||mappedOrders.includes(order),order});},updateOrderStatus(selector,status){effect('status',{store:this===store,selector,status});}};
 const emitter={emit(name,payload){effect('fill',{emitter:this===emitter,name,payload,keys:Object.keys(payload)});}};
 const parent={store:lookup,emitter,mapPosition(raw){effect('mapPosition',{parent:this===parent,identity:rawRows().includes(raw),raw});mutate();return cfg.mapped;},mapLimitOrder(raw){effect('mapLimitOrder',{parent:this===parent,identity:rawRows().includes(raw),raw});if(cfg.mutateStatus)raw.status=cfg.mutateStatus;return cfg.mapped;},mapAlgoOrder(raw){effect('mapAlgoOrder',{parent:this===parent,identity:rawRows().includes(raw),raw});mutate();return mappedOrders;}};
 const receiver={parent,store,isDisposed:cfg.disposed||false,trackLastReceived(){effect('received');},emitWsDiagnosticReceive(data){effect('diagnostic',{identity:data===cfg.wire});},sendWithLog(payload){effect('send',{payload});},handlePongEvent(){effect('pong');}};
 cfg.wire='original wire';
 const bindings={...enums,ORDER_SIDE,v,normalizeSymbol,parseFloat,jsonParse(data){effect('parse',{identity:data===cfg.wire});return cfg.parsed;},JSON:{stringify(payload){effect('encode',{payload,keys:Object.keys(payload)});return payload;}},performance:{now(){effect('clock');return 19.25;}}};
 for(const method of ['onMessage','handlePositionReport','handleExecutionReport','handleAlgoExecutionReport','subscribe','ping'])receiver[method]=body('private',method,bindings,receiver);
 for(const method of ['handlePositionReport','handleExecutionReport','handleAlgoExecutionReport']){const actual=receiver[method];receiver[method]=data=>{effect('route',{method,identity:data===cfg.parsed?.data});if(cfg.integrate)actual(data);};}
 if(name!=='onMessage')receiver[name]=body(name.startsWith('handle')&&['handleTickersStreamEvents','handleBBOStreamEvents','handleMarkPricesStreamEvents','handleFundingRateStreamEvents'].includes(name)?'public':'private',name,bindings,receiver);
 let error;try{receiver[name](name==='onMessage'?{data:cfg.wire}:cfg.data);}catch(e){error={identity:e===original,message:e.message};}
 rows[name].push({cfg:input,events,error,tickers:capture(cfg.tickers)});
}}
process.stdout.write(JSON.stringify({hashes,rows}));
