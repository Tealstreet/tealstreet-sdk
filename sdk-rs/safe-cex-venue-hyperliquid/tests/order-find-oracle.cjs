const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),dir='packages/safe-cex/src/exchanges/hyperliquid',file=dir+'/hyperliquid.exchange.ts',text=fs.readFileSync(path.join(root,file),'utf8'),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true),cls=ast.statements.find(x=>ts.isClassDeclaration(x)&&x.name.text==='HyperliquidExchange');
const member=cls.members.find(x=>x.name?.getText(ast)==='findOrderByClientOrderId');assert(member?.initializer);
// The actual source pattern, read from hyperliquid.cloid.ts rather than restated here.
const cloidFile=dir+'/hyperliquid.cloid.ts',cloidAst=ts.createSourceFile(cloidFile,fs.readFileSync(path.join(root,cloidFile),'utf8'),ts.ScriptTarget.Latest,true);
const patternDecl=cloidAst.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.getText(cloidAst)==='HYPERLIQUID_CLOID_PATTERN');assert(patternDecl?.initializer);
const pattern=vm.runInNewContext(patternDecl.initializer.getText(cloidAst));
let effects;const record=(...v)=>effects.push(v);
const context={exports:{},OrderStatus:{Open:'Open',Closed:'Closed',Canceled:'Canceled'},INFO_TYPES:{ORDER_STATUS:'orderStatus'},ENDPOINTS:{INFO:'info'},
 HYPERLIQUID_CLOID_PATTERN:{test(value){record('test',value);return pattern.test(value)}},
 orderCarriesClientOrderId(order,id){record('carries',order.id,id);return order.clientOrderId===id}};
vm.runInNewContext(ts.transpileModule('export class E {findOrderByClientOrderId = '+member.initializer.getText(ast)+';}',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
const scenes=['store-hit','store-other-symbol','no-user','application-id','invalid-cloid','venue-open','venue-triggered','venue-filled','venue-canceled','venue-unknown','venue-null-data','venue-no-row','mapped-other-symbol','mapped-undefined','post-reject'];
const CLOID='0x0123456789abcdef0123456789abcdef';
function tag(v){if(v===undefined)return['u'];if(v===null)return['null'];if(typeof v==='boolean')return['bool',v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',b.toString('hex')]}if(typeof v==='string')return['string',Array.from({length:v.length},(_,i)=>v.charCodeAt(i))];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.keys(v).map(k=>[k,tag(v[k])])]}
function venueStatus(scene){return{'venue-triggered':'triggered','venue-filled':'filled','venue-canceled':'marginCanceled'}[scene]||'open'}
(async()=>{const cases=[];let completed=0,last_index=-1;for(const scene of scenes){
effects=[];const e=new context.exports.E(),marker=new Error('find-error');
const id=scene==='invalid-cloid'?CLOID.toUpperCase():CLOID;
e.store={orders:[{id:'s1',symbol:'BTC',clientOrderId:scene==='store-hit'?CLOID:'0xother'},{id:'s2',symbol:'ETH',clientOrderId:CLOID}]};
e.options=scene==='no-user'?{vault:'',applicationId:undefined}:scene==='application-id'?{vault:'',applicationId:'app'}:{vault:'vault',applicationId:'app'};
const data=scene==='venue-null-data'?null:scene==='venue-unknown'?{status:'unknownOid'}:{status:'order',order:{order:scene==='venue-no-row'?null:{oid:9,coin:'BTC'},status:venueStatus(scene)}};
e.infoXhr={post(endpoint,body,config){record('post',endpoint,body.type,body.user,body.oid,config.headers['Content-Type']);return scene==='post-reject'?Promise.reject(marker):Promise.resolve({data})}};
e.mapOpenOrderRow=function(row,status){assert.equal(this,e,'source mapOpenOrderRow receiver');record('map',row.oid,status);return scene==='mapped-undefined'?undefined:{id:'mapped',symbol:scene==='mapped-other-symbol'?'ETH':'BTC',status}};
let result,rejected=false;try{result=await e.findOrderByClientOrderId('BTC',id)}catch(error){assert.equal(error,marker,'source original post error identity');rejected=true}
assert.equal(rejected,scene==='post-reject','source independent rejection floor');
if(!rejected)assert.equal(result.found,['store-hit','store-other-symbol','application-id','venue-open','venue-triggered','venue-filled','venue-canceled'].includes(scene),'source independent found floor '+scene);
if(scene==='store-hit')assert.equal(result.order,e.store.orders[0],'source store order alias');
cases.push({scene,state:tag({result,rejected,effects})});last_index=completed++;}
assert.equal(completed,15,'independent completed find floor');assert.equal(last_index,14,'independent last find index');process.stdout.write(JSON.stringify({cases,last_index}));})().catch(error=>{console.error(error);process.exitCode=1});
