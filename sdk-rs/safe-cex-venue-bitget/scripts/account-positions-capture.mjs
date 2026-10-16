import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const fields=(file,owner,names)=>{
 const p=path.join(root,'packages/safe-cex/src/exchanges/bitget/'+file);
 const ast=ts.createSourceFile(p,fs.readFileSync(p,'utf8'),ts.ScriptTarget.Latest,true);
 const c=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===owner);
 return names.map(name=>{const f=c.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)===name);assert(f?.initializer,'ACCOUNT actual source '+name);return name+'='+f.initializer.getText(ast)+';';}).join('\n');
};
const classic=fields('bitget.exchange.ts','BitgetExchange',['fetchBalance','fetchUtaBalance','fetchPositionMetadata','fetchUtaPositionMetadata','fetchPositions','fetchOpenPositions','fetchUtaOpenPositions']);
const classicFile=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const classicAst=ts.createSourceFile(classicFile,fs.readFileSync(classicFile,'utf8'),ts.ScriptTarget.Latest,true);
const finiteMethod=classicAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BitgetExchange').members.find(n=>ts.isMethodDeclaration(n)&&n.name.getText(classicAst)==='toFiniteNumber');
assert(finiteMethod,'ACCOUNT actual source finite helper');
const finiteText=finiteMethod.getText(classicAst).replace(/^private\s+/,'');
const inverse=fields('bitgetinverse.exchange.ts','BitgetInverseExchange',['fetchBalance']);
// ENDPOINTS and enums are transpiled from their real modules, not repeated fixture tables.
const moduleValue=(file,names)=>{const p=path.join(root,file);const ast=ts.createSourceFile(p,fs.readFileSync(p,'utf8'),ts.ScriptTarget.Latest,true);const statements=ast.statements.filter(n=>ts.isEnumDeclaration(n)&&names.includes(n.name.text)||ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));const code=ts.transpileModule(statements.map(n=>n.getText(ast).replace(/^export\s+/, '')).join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;return new Function(code+';return {'+names.join(',')+'};')();};
const {ENDPOINTS}=moduleValue('packages/safe-cex/src/exchanges/bitget/bitget.types.ts',['ENDPOINTS']);
const {PositionMode,PositionSide,MarginType}=moduleValue('packages/safe-cex/src/types.ts',['PositionMode','PositionSide','MarginType']);
const mathSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'),'utf8').replace(/^import .*;\n/gm,'').replace(/export\s+/g,'');
const mathCode=ts.transpileModule(mathSource,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const {subtract}=new Function('isNotNumber',mathCode+';return {subtract};')(Number.isNaN);
const {isBitgetSuccessCode,getBitgetApiErrorMessage,isBitgetEnvironmentMismatch,BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE}=moduleValue('packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts',['isBitgetSuccessCode','getBitgetApiErrorMessage','isBitgetEnvironmentMismatch','BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE']);
const idSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts'),'utf8').replace(/^import .*;\n/gm,'').replace(/export\s+/g,'');
const createPositionId=new Function('PositionMode','PositionSide',ts.transpileModule(idSource,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+';return createPositionId;')(PositionMode,PositionSide);
const {mapSeries}=require('p-iteration');const flatten=require('lodash/flatten'),uniqBy=require('lodash/uniqBy');
const code=ts.transpileModule('class Classic {'+classic+finiteText+'} class Inverse {'+inverse+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const {Classic,Inverse}=new Function('ENDPOINTS','PositionMode','PositionSide','MarginType','subtract','isBitgetSuccessCode','getBitgetApiErrorMessage','isBitgetEnvironmentMismatch','BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE','mapSeries','flatten','uniqBy','createPositionId',code+';return {Classic,Inverse};')(ENDPOINTS,PositionMode,PositionSide,MarginType,subtract,isBitgetSuccessCode,getBitgetApiErrorMessage,isBitgetEnvironmentMismatch,BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE,mapSeries,flatten,uniqBy,createPositionId);
const unpack=v=>v.u?undefined:v.null?null:'b'in v?v.b:v.n?Buffer.from(v.n).readDoubleLE():v.s?String.fromCharCode(...v.s):v.a?v.a.map(unpack):Object.fromEntries(v.o.map(([k,v])=>[String.fromCharCode(...k),unpack(v)]));
const pack=v=>v===undefined?{u:true}:v===null?{null:true}:typeof v==='boolean'?{b:v}:typeof v==='number'?{n:[...(()=>{const b=Buffer.alloc(8);b.writeDoubleLE(v);return b;})()]}:typeof v==='string'?{s:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:Array.isArray(v)?{a:v.map(pack)}:{o:Object.entries(v).map(([k,v])=>[Array.from({length:k.length},(_,i)=>k.charCodeAt(i)),pack(v)])};
const cases=JSON.parse(fs.readFileSync(0,'utf8'));const floor=process.argv.includes('--diagnostic')?7:64;assert.equal(cases.length,floor,'ACCOUNT independent input floor');
const outputs=[];
for(const c of cases){
 const trace=[],params=[],emissions=[],response=unpack(c.response),open=unpack(c.open),meta=unpack(c.meta),stored=unpack(c.stored),original={message:'rejected'};
 const target=c.op==='inverse-balance'?new Inverse():new Classic();
 target.usesUtaRouting=c.scene==='route';target.apiProductType='USDT-FUTURES';target.apiMarginCoin='USDT';target.apiUtaCategory='linear';target.isDisposed=false;let testnetReads=0;target.options={get testnet(){trace.push('testnet');const read=testnetReads++;if(c.scene==='second-testnet-failure'&&read===1)throw original;return c.scene==='toggle-testnet'&&read===1;}};
 target.store={balance:stored,marketsBySymbol:{BTC:{id:'BTCID'}},tickers:[{symbol:'BTCUSD',mark:30000}]};
 const read=client=>async (url,options)=>{const endpoint={balance:'BALANCE','uta-balance':'UTA_ACCOUNT_ASSETS',metadata:'FETCH_SINGLE_ACCOUNT','uta-metadata':'ACCOUNT_SETTINGS_V3',open:'POSITIONS_V2','uta-open':'UTA_POSITIONS','inverse-balance':'BALANCE'}[c.op];assert.equal(url,ENDPOINTS[endpoint],'ACCOUNT actual endpoint path');trace.push('read:'+client+':'+endpoint);params.push(options?.params||{});if(c.scene==='reject')throw original;return {data:response};};
 target.xhr={get:read('regular')};target.unlimitedXHR={get:read('fast')};
 target.ensureAccountTypeDetected=async()=>{trace.push('call:ensureAccountTypeDetected');};
 target.promoteUtaRoutingFromClassicUnsupported=()=>{trace.push('promote');return c.scene==='promote';};
 target.calculateAccountUpnl=f=>{trace.push('upnl');return f+2;};

 target.emitAndLogError=(e,d)=>{trace.push('emit');if(e===original||e instanceof Error)e={message:e.message};emissions.push([pack(e),d===undefined?null:pack(d)]);};
 Object.defineProperty(target,'mapPosition',{get(){trace.push('map:false');return r=>({raw:r.raw,uta:false});}});
 Object.defineProperty(target,'mapUtaPosition',{get(){trace.push('map:true');return r=>({raw:r.raw,uta:true});}});
 if(['classic-data-toggle','classic-data-failure','uta-data-toggle'].includes(c.scene)){const first=response.data,alternate=unpack(c.alternate);let reads=0;Object.defineProperty(response,'data',{enumerable:true,get(){trace.push('response-data');const read=reads++;if(read===1){if(c.scene==='classic-data-failure')throw original;return alternate;}return first;}});}
 if(['uta-list-toggle','uta-list-failure'].includes(c.scene)){const first=response.data.list,alternate=unpack(c.alternate).list;let reads=0;Object.defineProperty(response.data,'list',{enumerable:true,get(){trace.push('response-list');const read=reads++;if(read===1){if(c.scene==='uta-list-failure')throw original;return alternate;}return first;}});}
 const current={balance:'fetchBalance','uta-balance':'fetchUtaBalance',metadata:'fetchPositionMetadata','uta-metadata':'fetchUtaPositionMetadata',positions:'fetchPositions',open:'fetchOpenPositions','uta-open':'fetchUtaOpenPositions','inverse-balance':'fetchBalance'}[c.op];
 for(const name of ['fetchUtaBalance','fetchUtaOpenPositions','fetchUtaPositionMetadata'])if(name!==current)target[name]=async()=>{trace.push('call:'+name);return {delegated:name};};
 if(current!=='fetchOpenPositions')target.fetchOpenPositions=async()=>{trace.push('call:fetchOpenPositions');if(c.scene==='reject')throw original;return c.scene==='empty'?[]:open;};
 if(current!=='fetchPositionMetadata')target.fetchPositionMetadata=async symbols=>{trace.push('call:fetchPositionMetadata');params.push(symbols);return meta;};
 let result;
 try {let value;if(c.op==='uta-balance')value=await target[current](target.xhr);else if(c.op==='balance')value=await target[current]({preferFast:c.scene==='fast'});else if(c.op==='positions')value=await target[current](c.scene==='route'?undefined:['BTC','ETH','BTC']);else if(c.op.includes('metadata'))value=await target[current](['BTC','ETH']);else value=await target[current]();result={ok:pack(value)};}
 catch(e){result={error:pack(e.message),token:e===original?'original':e.name};}
 outputs.push({result,trace,params:params.map(pack),emissions});
}
assert.equal(outputs.length,floor,'ACCOUNT independent output floor');console.log(JSON.stringify(outputs));
