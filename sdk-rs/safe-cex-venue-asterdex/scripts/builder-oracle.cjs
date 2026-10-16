const fs=require('node:fs'),path=require('node:path');
const {ts,root,values}=require('./source.cjs'), load=require('./load-module.cjs');
const compilerRoot=path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root,'sdk-rs/target/asterdex-oracle-deps/node_modules/typescript'))),'..');
if(require(path.join(compilerRoot,'package.json')).name!=='typescript')throw Error('Unrecognized TypeScript installation');
const ethersPath=path.join(path.dirname(compilerRoot),...(process.argv.includes('--missing-package-control')?['asterdex-missing-ethers-control']:[]),'ethers');
const ethers=require(ethersPath);
const unavailable=new Proxy(function(){throw Error('Unselected dependency invoked');},{get(_,key){if(key==='__esModule')return false;return ()=>{throw Error('Unselected dependency invoked '+String(key));};}});
const shared=ts.createSourceFile('types.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const printer=ts.createPrinter();const enumSource=shared.statements.filter(ts.isEnumDeclaration).map(n=>printer.printNode(ts.EmitHint.Unspecified,n,shared)).join('\n');
const enums={exports:{}};new Function('exports',ts.transpileModule(enumSource,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText)(enums.exports);
const fee=load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts',['parseScaledDecimal','normalizeDecimalString','assertFeeRate','resolveBuilderAddress'],{ethers,'../../types':enums.exports});
const approval=load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts',['getBaseUrl','extractBuilderRows','parseFeeRate','getProviderErrorCode','getProviderErrorMessage','getAsterBuilderApprovalConfig'],{ethers:unavailable,'../../transport/registry':unavailable,'./asterdex.api':require('./api-source.cjs'),'./asterdex.builder-fee':fee,'./asterdex.types':values});
const {schemas,shape,witnesses,accepts}=require('./oracle.cjs');
const records={};
for(const file of ['asterdex.builder-fee.ts','asterdex.builder-approval.ts']){
 const text=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/asterdex',file),'utf8'),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
 for(const n of ast.statements)if(ts.isTypeAliasDeclaration(n)&&n.modifiers?.some(x=>x.kind===ts.SyntaxKind.ExportKeyword)){
  schemas[n.name.text]=shape(n.type);records[n.name.text]=witnesses(schemas[n.name.text]).map(input=>({input,accept:accepts(schemas[n.name.text],input)}));
 }
}
const result=fn=>{try{return {value:fn()??null}}catch(e){return {error:{name:e.name,message:e.message}}}};
const parsing=[undefined,null,false,true,0,1,'',' ','\ufeff','\u0085','0x10','12abc',[],[1],{}, {toString:null},'1e309',-5, {data:[null,1,{builderAddress:'a'}]},[null,1,{maxFeeRate:true}],{code:4902,message:''},{code:'4902',message:false},'  text  '].map(input=>({...(input===undefined?{}:{input}),rows:approval.extractBuilderRows(input),rate:result(()=>approval.parseFeeRate(input)),code:approval.getProviderErrorCode(input)??null,message:approval.getProviderErrorMessage(input,'fallback')}));
const urls=[];for(const base of [undefined,'','  ','\ufeffhttps://host/// \ufeff','\u0085','https://host/'])for(const testnet of [false,true])urls.push({base:base??null,testnet,output:approval.getBaseUrl(testnet,base)});
const orders=[];for(const type of ['limit','market','stop_market'])for(const timeInForce of [undefined,'PostOnly','GoodTillCancel'])for(const postOnly of [undefined,false,true])for(const apiOrderType of ['LIMIT','STOP','TAKE_PROFIT','MARKET','stop'])for(const feeRateMaker of ['0','0.001','0.0'])for(const feeRateTaker of ['0','0.0001','0.0']){
 const opts={symbol:'BTCUSDT',side:'buy',amount:1,type,...(timeInForce?{timeInForce}:{}),...(postOnly===undefined?{}:{postOnly})},config={builderAddress:'builder',feeRateMaker,feeRateTaker};
 orders.push({opts,apiOrderType,config,maker:fee.isAsterMakerOnlyOrder(opts,apiOrderType),rate:fee.getAsterBuilderFeeRateForOrder(opts,apiOrderType,config)??null});
}
const typed=[];for(const testnet of [false,true])for(const builderName of [undefined,'','Builder']){const input={builderAddress:'0x'+'1'.repeat(40),maxFeeRate:'0.0001',nonce:'9007199254740993',testnet,userAddress:'0x'+'2'.repeat(40),...(builderName===undefined?{}:{builderName})};typed.push({input,output:approval.buildAsterApproveBuilderTypedData(input)});}
const constants=Object.fromEntries(Object.entries(fee).filter(([,v])=>typeof v==='string'));
const decimals=['','0','1','01','000','0.00',' 0.0010 ','\ufeff0.001\ufeff','\u00850.001','0.001000000000000001','0.000000000000000001','0.0000000000000000001','0.','0.1.2','.1','+1','-0','1e-3','0x10','1\n','0.1\r\n','0.1\u2028','١','9'.repeat(400)];
const decimalCases=decimals.map(input=>({input,scaled:result(()=>fee.parseScaledDecimal(input)?.toString()),normalized:result(()=>fee.normalizeDecimalString(input)),checked:result(()=>fee.assertFeeRate(input,'maker fee rate'))}));
const caps=[];for(const feeRateMaker of decimals)for(const feeRateTaker of ['0','0.0001','bad','0.0010']){const config={builderAddress:'a',feeRateMaker,feeRateTaker};caps.push({config,output:fee.getAsterBuilderApprovalFeeRate(config)});}
const nativeErrors=['boom','','  ','\ufeff'].map(message=>{const error=new Error(message);return {json:JSON.parse(JSON.stringify(error)),message:error.message,output:approval.getProviderErrorMessage(error,'fallback')};});
const api=require('./api-source.cjs');
const nonces=[['a',0],['a',1],['a',1],['b',-1],['a',0],['a',0.5],['a',NaN],['a',Infinity],['a',1],['a',1e30],['a',1],['b',0]].map(([key,time],index)=>{api.setOracleClock(time);return {key,time:Number.isFinite(time)?time:String(time),wrapper:index%2===0,output:result(()=>index%2===0?api.buildAsterDEXV3Nonce(key):api.buildV3Nonce(key))}});
const builderAddresses=[undefined,'',' ','\ufeff','\u0085','REPLACE_WITH_X','0x'+'0'.repeat(40),'0x52908400098527886e0f7030069857d2e4169ee7','0x52908400098527886E0F7030069857D2E4169Ee7',' 0xde709f2102306220921060314715629080e2fb77\ufeff','de709f2102306220921060314715629080e2fb77','XE65GB6LDNXYOFTX0NSV3FUWKOWIXAMJK36','XE54ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ','not an address','😀','é'];
const configs=[];
for(const testnet of [false,true])for(const address of builderAddresses)for(const maker of [undefined,'','0','0.0','0.0001','0.001','0.002','bad'])for(const taker of [undefined,'0','0.0001','bad']){
 const options={key:'api-key',secret:'api-secret',testnet,extra:{...(address===undefined?{}:{asterBuilderAddress:address}),...(maker===undefined?{}:{asterBuilderFeeRateMaker:maker}),...(taker===undefined?{}:{asterBuilderFeeRateTaker:taker})}};
 configs.push({options,address:result(()=>fee.resolveBuilderAddress(options)),config:result(()=>fee.resolveAsterBuilderFeeConfig(options)),approvalConfig:result(()=>approval.getAsterBuilderApprovalConfig(options))});
}
const builderSurrogates = ['{"key":"api-key","secret":"api-secret","extra":{"asterBuilderAddress":"\\ud800"}}','{"key":"api-key","secret":"api-secret","extra":{"asterBuilderAddress":"\\udfff","asterBuilderFeeRateMaker":"0","asterBuilderFeeRateTaker":"0"}}'].map(inputJson=>{const input=JSON.parse(inputJson);return {inputJson,address:result(()=>fee.resolveBuilderAddress(input)),config:result(()=>fee.resolveAsterBuilderFeeConfig(input))}});
process.stdout.write(JSON.stringify({builderSurrogates,configs,records,parsing,urls,orders,typed,constants,nativeErrors,decimalCases,caps,nonces}));
