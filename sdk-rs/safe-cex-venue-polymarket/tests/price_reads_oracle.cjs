// Executes original methods at an injected, already-started Axios boundary.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'isolated compiler required');const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/polymarket');
const text=fs.readFileSync(path.join(root,'polymarket.api.ts'),'utf8'),ast=ts.createSourceFile('api.ts',text,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='PolymarketAPI');
const names=['getMidpoint','getMidpoints','getPrice','getLastTradePrice','getPriceHistory','getOrder','getBalanceAllowance','deriveApiKey'];
const methods=names.map(name=>{const nodes=klass.members.filter(n=>n.name?.getText(ast)===name);assert.equal(nodes.length,1,name);return text.slice(nodes[0].getFullStart(),nodes[0].end);});
const constants=fs.readFileSync(path.join(root,'polymarket.constants.ts'),'utf8'),cAst=ts.createSourceFile('constants.ts',constants,ts.ScriptTarget.Latest,true);
const table=cAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(cAst)==='ENDPOINTS'));assert.equal(table.length,1,'original endpoints');
const context={exports:{}};vm.runInNewContext(ts.transpileModule(`${table[0].getText(cAst)}\nclass API { ${methods.join('\n')} }\nexports.API=API; exports.prototype=Object.prototype;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const numeric=n=>Number.isNaN(n)?{kind:'nan'}:{kind:'number',bits:bits(n)};
const strings=['id/1','','🦀 λ,a?b#c%20','\ud800','\udfff','a/b/c?x=1&y=2','__proto__','constructor','a\0b','quote"\\end','x\u2028y','pair😀','01','-0','\ud800\udfff','final'];
const prices=['0.375tail','','\ufeff -0','Infinityrest','-Infinity','1e999','1e-999','-1e-999','0x10','1e+oops','.25suffix','\u0085 9','12\ud80099','\udfff12','\u2028+6.02e23ignore','5e-324suffix'];
const numbers=[0,-0,1.5,-1,NaN,Infinity,-Infinity,1e21,1e-7,1e-6,Number.MAX_VALUE,Number.MIN_VALUE,9007199254740992,0.9999999999999999,5e-324,-0];
const row=(marker,index)=>({marker,timestamp:-0,price:strings[index],fromReply:0,fromInput:0});
function value(index){switch(index){case 0:return [row(73,index)];case 1:return undefined;case 2:return null;case 3:return false;case 4:return 0;case 5:return -0;case 6:return NaN;case 7:return '';case 8:return [];case 9:return true;case 10:return Infinity;case 11:return '\ud800raw';case 12:return row(73,index);case 13:return -Infinity;case 14:return '-0';case 15:return [row(73,index),row(79,index)];}}
function project(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='boolean')return{kind:'bool',value:v};if(typeof v==='number')return numeric(v);if(typeof v==='string')return{kind:'string',units:units(v)};if(Array.isArray(v))return{kind:'array',rows:v.map(project)};return{kind:'object',marker:v.marker,timestamp:numeric(v.timestamp),price:units(v.price),fromReply:v.fromReply,fromInput:v.fromInput};}
const param=v=>v===undefined?{kind:'undefined'}:typeof v==='number'?numeric(v):{kind:'string',units:units(v)};
function snapshot(method,url,config,body){return{client:'xhr',method,url:units(url),params:config?.params?Object.entries(config.params).map(([k,v])=>[k,param(v)]):null,headers:config?.headers?Object.entries(config.headers).map(([k,v])=>[k,units(v)]):null,data:body?body.map(v=>Object.entries(v).map(([k,s])=>[k,units(s)])):null};}
function args(name,index,ids){const s=strings[index];switch(name){case'getMidpoints':return[ids];case'getPrice':return[s,index%2?'SELL':'BUY'];case'getPriceHistory':return[s,index%3===0?undefined:numbers[index],index%4===0?undefined:numbers[(index+5)%16],index%5===0?undefined:s,index%2===0?undefined:numbers[(index+7)%16]];case'getBalanceAllowance':return[numbers[index]];case'deriveApiKey':return[s,numbers[index],'sig-'+s,numbers[(index+5)%16]];default:return[s];}}
(async()=>{const cases=[];
for(const name of names)for(let index=0;index<16;index++)for(const fails of [false,true]){
 const api=new context.exports.API(),ids=index===1?[]:[strings[index],'second',strings[index]],input=args(name,index,ids),original=value(index);
 const priceData={mid:'-55',price:'66'};const midpointData=Object.create(null);
 for(const [key,price]of[['10','-0'],['2',prices[index]],['01','Infinity'],['__proto__','bad'],['constructor','0x99'],['key-'+strings[index],'1e+bad']])Object.defineProperty(midpointData,key,{value:price,enumerable:true,writable:true});
 const historyData=index===1?{}:{history:original};const data=['getMidpoint','getPrice','getLastTradePrice'].includes(name)?priceData:name==='getMidpoints'?midpointData:name==='getPriceHistory'?historyData:original;
 const calls=[],error={marker:47,message:'original read failure'};let finish;
 api.xhr={};for(const method of ['get','post'])api.xhr[method]=(url,bodyOrConfig,postConfig)=>{const body=method==='post'?bodyOrConfig:undefined,config=method==='post'?postConfig:bodyOrConfig;calls.push(snapshot(method,url,config,body));if(name==='getMidpoints'){assert.notStrictEqual(body,ids,'source fresh mapped array');assert.equal(body.length,index===1?0:3,'independent mapped rows');if(body.length){body[0].token_id='from-host';assert.equal(ids[0],strings[index],'source fresh row');ids[0]='from-input';assert.equal(body[0].token_id,'from-host','source input detached');}}return new Promise((resolve,reject)=>{finish=()=>fails?reject(error):resolve({data});});};
 let settled=false;const pending=api[name](...input);pending.then(()=>settled=true,()=>settled=true);assert.equal(calls.length,1,'source eager dispatch');await Promise.resolve();assert.equal(settled,false,'source pending observation');priceData.mid=prices[index];priceData.price=prices[(index+3)%16];finish();
 let result;if(fails){try{await pending;assert.fail('source rejection');}catch(e){assert.strictEqual(e,error,'source error identity');}result={kind:'error',marker:47,message:error.message};}else{const out=await pending;if(name==='getMidpoint'||name==='getPrice'||name==='getLastTradePrice')result=numeric(out);else if(name==='getMidpoints'){assert.notStrictEqual(out,data,'source transformed record fresh');assert.equal(Object.getPrototypeOf(out),context.exports.prototype,'source ordinary fromEntries prototype');assert.equal(Object.keys(out).length,6,'independent midpoint entry count');result={kind:'entries',values:Object.entries(out).map(([k,v])=>[units(k),numeric(v)])};}else{
  const fallback=name==='getPriceHistory'&&!original;
  assert.equal(Object.is(out,original),!fallback,'source returned identity/truthiness');if(fallback){assert(Array.isArray(out));assert.equal(out.length,0,'source fresh fallback array');}
  result=project(out);let replyToInput=false,inputToReply=false;
  if(out&&typeof out==='object'){const target=Array.isArray(out)?out[0]:out,source=Array.isArray(original)?original?.[0]:original;if(target&&source){target.fromReply=17;replyToInput=source.fromReply===17;source.fromInput=23;inputToReply=target.fromInput===23;assert(replyToInput&&inputToReply,'source two-way response alias');}}
  result={...result,same:Object.is(out,original),replyToInput,inputToReply,fallback};
 }
 }
 cases.push({name,index,fails,request:calls[0],result});
}
let detachedCompleted=0,finish;const detached=new context.exports.API();detached.xhr={get(){return new Promise(resolve=>{finish=()=>{detachedCompleted++;resolve({data:{mid:'1'}});};});}};detached.getMidpoint('ignored');finish();await Promise.resolve();assert.equal(detachedCompleted,1,'source abandoned observer retains operation');
const fresh=new context.exports.API(); fresh.xhr={get(){return Promise.resolve({data:{history:false}});}}; const first=await fresh.getPriceHistory('fresh'),second=await fresh.getPriceHistory('fresh'); assert.notStrictEqual(first,second,'source independently fresh fallback arrays'); first.push('marker'); assert.equal(second.length,0,'source fallback arrays do not alias');
assert.equal(cases.length,256,'independent eight-method sixteen-case two-outcome floor');assert.equal(cases[255].name,'deriveApiKey');assert.equal(cases[255].index,15);assert.equal(cases[255].fails,true,'last fixture');process.stdout.write(JSON.stringify({cases,detachedCompleted,freshFallbacks:true}));
})().catch(e=>{console.error(e);process.exitCode=1;});
