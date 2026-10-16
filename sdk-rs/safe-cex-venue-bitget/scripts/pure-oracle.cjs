// Execute actual family/API predicate bodies; imports are replaced only with source protocol values.
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
const dir = path.join(root, 'packages/safe-cex/src/exchanges/bitget');
function load(file, names, bindings = {}) {
  const source=fs.readFileSync(path.join(dir,file),'utf8');
  const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
  const nodes=ast.statements.filter(n=>ts.isVariableStatement(n) && n.declarationList.declarations.some(d=>names.includes(d.name.text)));
  const found=nodes.flatMap(n=>n.declarationList.declarations.map(d=>d.name.text));
  require('node:assert/strict').deepEqual([...found].sort(),[...names].sort(),`${file} oracle declaration inventory`);
  const body=nodes.map(n=>n.getText(ast).replace(/^export\s+/, '')).join('\n');
  return new Function(...Object.keys(bindings), ts.transpileModule(body,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+`\nreturn {${names.join(',')}};`)(...Object.values(bindings));
}
const familyNames=['BITGET_LIKE_EXCHANGES','isBitgetLikeExchange','isBitgetVirtualAttachedTpSlOrderId','isBitgetVirtualAttachedTpSlOrder'];
const family=load('bitget.family.ts',familyNames);
const protocol=load('bitget.types.ts',['ENDPOINTS']);
const api=load('bitget.api.ts',['SILENT_ERROR_CODES','isBitgetUtaStrategyOrderNotFound','shouldSilenceBitgetResponseError','shouldSuppressValidationErrorEmission'],protocol);
const exchanges=[undefined,'','bitget','bitgetusdc','bitgetinverse','bitgetspot','BITGET','bitgetuta',' bitget','bitget\uFEFF','bitget\u0085','constructor','__proto__'];
const ids=[undefined,'','123','123__stop_loss','123__take_profit','__stop_loss','__take_profit','123__stop_loss_extra','123__STOP_LOSS','123__take_profit\n','📈__stop_loss'];
const scalarPrefixes=['İ','ſ','ΟΣ','𐐀','\0','\uFEFF','\u0085'];
ids.push(...scalarPrefixes.map(prefix=>prefix+'__stop_loss'));
const familyCases=[];
for(const exchange of exchanges)for(const id of ids)familyCases.push({exchange,id,member:family.isBitgetLikeExchange(exchange),virtual:family.isBitgetVirtualAttachedTpSlOrderId(id),attached:family.isBitgetVirtualAttachedTpSlOrder({id},exchange)});
const messages=[undefined,null,false,0,42,[],{},['corresponding strategy id could not be found'],'','ok','Corresponding strategy ID could not be found','Failed to stop the strategy because the corresponding strategy ID could not be found','CORRESPONDING STRATEGY ID COULD NOT BE FOUND','environment is incorrect','ENVIRONMENT IS INCORRECT: key','Environment is\uFEFFincorrect','corresponding strategy id could not be found\u0085','environment is incorrect\uFEFF'];
messages.push(...scalarPrefixes.flatMap(prefix=>[prefix+'environment is incorrect',prefix+'corresponding strategy id could not be found']));
const codes=['','0','00000','40084','40085','400840',' 40085','40086'];
const urls=[undefined,'',protocol.ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER,'https://api.bitget.com'+protocol.ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER+'?id=1',protocol.ENDPOINTS.CANCEL_ALGO_ORDER,protocol.ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,'prefix'+protocol.ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER+'suffix'];
const apiCases=[];
for(const message of messages)for(const code of codes)for(const url of urls)apiCases.push({message,code,url,notFound:api.isBitgetUtaStrategyOrderNotFound(message),silence:api.shouldSilenceBitgetResponseError(code,message,url)});
const validationCases=[{},...messages.map(suppressBitgetValidationErrorEmission=>({suppressBitgetValidationErrorEmission})),{suppressBitgetValidationErrorEmission:true},{suppressBitgetValidationErrorEmission:'true'},{suppressBitgetValidationErrorEmission:1}].map(exchange=>({exchange,suppress:api.shouldSuppressValidationErrorEmission(exchange)}));
const utf16Cases=[];
for(const prefix of ['\uD800','\uDC00']) {
  const id=prefix+'__stop_loss';
  const message=prefix+'corresponding strategy id could not be found';
  utf16Cases.push({idJson:JSON.stringify(id),messageJson:JSON.stringify(message),exchangeJson:JSON.stringify(prefix+'bitget'),member:family.isBitgetLikeExchange(prefix+'bitget'),virtual:family.isBitgetVirtualAttachedTpSlOrderId(id),attached:family.isBitgetVirtualAttachedTpSlOrder({id},'bitget'),notFound:api.isBitgetUtaStrategyOrderNotFound(message),silence:api.shouldSilenceBitgetResponseError('40086',message,protocol.ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER)});
}
const account=load('bitget.exchange.ts',['BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE','isBitgetUtaClassicUnsupported','getBitgetApiErrorMessage','isBitgetEnvironmentMismatch']);
const accountCases=[];
const accountMessages=[undefined,'',' ','ok','UNIFIED ACCOUNT MODE: CLASSIC ACCOUNT API IS NOT SUPPORTED','unified account mode','classic account api is not supported','environment is incorrect',...scalarPrefixes.flatMap(p=>[p+'ENVIRONMENT IS INCORRECT',p+'unified account mode classic account api is not supported'])];
for(const code of [undefined,'','40085',40085,40084,'040085','40085.0',' 40085','0x9c95',0,-0,1e21,1e-7])for(const message of accountMessages)for(const fallback of ['','fallback','fallback\uFEFF']) {
  const payload={code,msg:message};
  accountCases.push({code,message,fallback,unsupported:account.isBitgetUtaClassicUnsupported(payload),error:account.getBitgetApiErrorMessage(payload,fallback),environment:account.isBitgetEnvironmentMismatch(message)});
}
// Pin absence of the whole payload separately from missing members.
for(const fallback of ['','fallback']) accountCases.push({fallback,unsupported:account.isBitgetUtaClassicUnsupported(),error:account.getBitgetApiErrorMessage(undefined,fallback),environment:account.isBitgetEnvironmentMismatch()});
const accountUtf16Cases=['\uD800','\uDC00'].map(prefix=>{
  const message=prefix+'environment is incorrect unified account mode classic account api is not supported';
  return {messageJson:JSON.stringify(message),unsupported:account.isBitgetUtaClassicUnsupported({msg:message}),errorJson:JSON.stringify(account.getBitgetApiErrorMessage({msg:message},'fallback')),environment:account.isBitgetEnvironmentMismatch(message)};
});
const result=JSON.stringify({members:[...family.BITGET_LIKE_EXCHANGES],familyCases,apiCases,validationCases,utf16Cases,accountCases,accountUtf16Cases,unsupportedMessage:account.BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE})+'\n';
const fixture=path.join(__dirname,'../tests/pure-oracle.json');
if(process.argv.includes('--write'))fs.writeFileSync(fixture,result);
else if(process.argv.includes('--check')) {
  require('node:assert/strict').equal(fs.readFileSync(fixture,'utf8'),result,'Bitget TS pure oracle snapshot drift');
  console.log(`Bitget pure oracle: ${familyCases.length} family, ${apiCases.length} API, ${validationCases.length} validation, ${accountCases.length} account cases`);
} else process.stdout.write(result);
