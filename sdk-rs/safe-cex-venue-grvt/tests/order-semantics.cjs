'use strict';
const assert=require('node:assert/strict'), fs=require('node:fs'), path=require('node:path'), vm=require('node:vm');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler),'absolute isolated compiler required');
const ts=require(compiler);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/grvt/grvt.order-semantics.ts');
const source=fs.readFileSync(file,'utf8');
const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const sharedFile=path.resolve(path.dirname(file),'../../types.ts');
const shared=ts.createSourceFile(sharedFile,fs.readFileSync(sharedFile,'utf8'),ts.ScriptTarget.Latest,true);
const enumDecl=shared.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text==='OrderTimeInForce');
const fnDecl=tree.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(tree)==='getGrvtOrderTimeInForce'));
assert(enumDecl&&fnDecl,'actual TIF source declarations');
let text=fnDecl.getText(tree);
if(process.env.GRVT_TIF_MUTATION){
 assert.equal(process.env.GRVT_TIF_MUTATION,'nullish');
 assert(text.includes('order.post_only ?? order.po'));
 text=text.replace('order.post_only ?? order.po','order.post_only || order.po');
}
const triggerEnum=shared.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text==='OrderStopTriggerType');
const triggerDecl=tree.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(tree)==='getGrvtTriggerMetadata'));
assert(triggerEnum&&triggerDecl);
let triggerText=triggerDecl.getText(tree);
if(process.env.GRVT_TRIGGER_MUTATION){
 assert.equal(process.env.GRVT_TRIGGER_MUTATION,'number');
 assert(triggerText.includes('Number(rawTriggerPrice)'));
 triggerText=triggerText.replace('Number(rawTriggerPrice)','parseFloat(rawTriggerPrice)');
}
const output=ts.transpileModule(enumDecl.getText(shared)+'\n'+triggerEnum.getText(shared)+'\n'+text+'\n'+triggerText,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(output.diagnostics.length,0);
const exportsObject={};vm.runInNewContext(output.outputText,{exports:exportsObject},{filename:file});
const post=[undefined,'null','false','true','0','-0','1e400','"false"','[]','{}'];
const tifs=[undefined,'null','"GOOD_TILL_TIME"','"IMMEDIATE_OR_CANCEL"','"FILL_OR_KILL"','"ALL_OR_NONE"','"RETAIL_PRICE_IMPROVEMENT"','""','"unknown"','0','[]','{}'];
const rows=[];
for(const a of post)for(const b of post)for(const c of tifs)for(const d of tifs){
 const entries=[['post_only',a],['po',b],['time_in_force',c],['ti',d]].filter(([,value])=>value!==undefined);
 const raw='{'+entries.map(([key,value])=>JSON.stringify(key)+':'+value).join(',')+'}';
 const result=exportsObject.getGrvtOrderTimeInForce(JSON.parse(raw));
 rows.push({raw,result:result??null});
}
const refused=['{"post_only":"\\ud800","time_in_force":"GOOD_TILL_TIME"}','{"time_in_force":"\\udc00"}'].map(raw=>({raw,result:exportsObject.getGrvtOrderTimeInForce(JSON.parse(raw))??null}));
const triggerRows=[];
const bits=x=>{const b=Buffer.alloc(8);b.writeDoubleBE(x);return b.toString('hex');};
function addTrigger(raw){
 try{
  const result=exportsObject.getGrvtTriggerMetadata(JSON.parse(raw));
  triggerRows.push({raw,result:result===undefined ? null : {...result,triggerPrice:undefined,priceBits:result.triggerPrice===undefined ? null : bits(result.triggerPrice)}});
 }catch(error){triggerRows.push({raw,error:{name:error.name,message:error.message}});}
}
const tokens=[undefined,'null','false','true','0','-0','-0.0','1.25','1e3','1e400','-1e400','-1e-400','""','"0x10"','"12abc"','"-0"','"1e400"','[]','[null]','["-0"]','[1,2]','{}','{"toString":1}','"LAST"','"MARK"','"INDEX"'];
const object=(entries)=>'{'+entries.filter(([,v])=>v!==undefined).map(([k,v])=>JSON.stringify(k)+':'+v).join(',')+'}';
for(const kind of ['STOP_LOSS','TAKE_PROFIT'])for(const lite of [false,true])for(const field of ['trigger_price','trigger_by','close_position','is_split_position'])for(const a of tokens)for(const b of tokens){
 const aliases={trigger_price:'tp',trigger_by:'tb',close_position:'cp',is_split_position:'is'};
 const tpsl=object([[field,a],[aliases[field],b]]);
 const trigger=object([[lite?'tt':'trigger_type',JSON.stringify(kind)],[lite?'t':'tpsl',tpsl]]);
 addTrigger(object([[lite?'m':'metadata',object([[lite?'t':'trigger',trigger]])]]));
}
for(const field of ['metadata','m'])for(const value of tokens)addTrigger(object([[field,value]]));
for(const trigger of tokens)addTrigger(object([['metadata',object([['trigger',trigger]])]]));
const validTpsl='{"tp":"-0","tb":"MARK","cp":true}';
const validTrigger='{"tt":"STOP_LOSS","t":'+validTpsl+'}';
const validMetadata='{"t":'+validTrigger+'}';
for(const primary of tokens){
 addTrigger(object([['metadata',primary],['m',validMetadata]]));
 addTrigger(object([['metadata',object([['trigger',primary],['t',validTrigger]])]]));
 addTrigger(object([['metadata',object([['trigger',object([['trigger_type',primary],['tt','"STOP_LOSS"'],['t',validTpsl]])]])]]));
 addTrigger(object([['metadata',object([['trigger',object([['tt','"STOP_LOSS"'],['tpsl',primary],['t',validTpsl]])]])]]));
}
const triggerRefused=['{"metadata":{"trigger":{"trigger_type":"STOP_LOSS","tpsl":{"tp":"\\ud800"}}}}'];
const triggerBoundary=triggerRefused.map(raw=>{
 const result=exportsObject.getGrvtTriggerMetadata(JSON.parse(raw));
 return {raw,type:result.type,priceType:typeof result.triggerPrice};
});
const callable=()=>true;
const symbol=Symbol('price');
const dynamic={postType:typeof callable,postResult:exportsObject.getGrvtOrderTimeInForce({post_only:callable,time_in_force:'GOOD_TILL_TIME'})??null,priceType:typeof symbol};
try{exportsObject.getGrvtTriggerMetadata({metadata:{trigger:{tt:'STOP_LOSS',t:{tp:symbol}}}});assert.fail('symbol must throw');}
catch(error){dynamic.priceError={name:error.name,message:error.message};}
process.stdout.write(JSON.stringify({rows,refused,triggerRows,triggerBoundary,dynamic}));
