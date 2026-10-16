const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependency = createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH,'package.json'));
const root = path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/apexomni/zklink/signer');
function declarations(file) {
  const code=fs.readFileSync(path.join(root,file),'utf8');
  const tree=ts.createSourceFile(file,code,ts.ScriptTarget.Latest,true);
  return tree.statements.filter(n=>ts.isFunctionDeclaration(n) || ts.isVariableStatement(n)).map(n=>n.getText(tree)).join('\n');
}
const source=['utils.ts',...['contract','order','transfer','withdraw','changePubKey'].map(n=>`transactions/${n}.ts`)].map(declarations).join('\n');
let operations=[];
const record=(kind,args)=>{operations.push([kind,...args]);return {kind,args};};
const sdk={
  ContractBuilder:class {constructor(...args){Object.assign(this,record('builder:Contract',args));}},
  Order:class {constructor(...args){Object.assign(this,record('builder:Order',args));}},
  TransferBuilder:class {constructor(...args){Object.assign(this,record('builder:Transfer',args));}},
  WithdrawBuilder:class {constructor(...args){Object.assign(this,record('builder:Withdraw',args));}},
  ChangePubKeyBuilder:class {constructor(...args){Object.assign(this,record('builder:ChangePubKey',args));}},
  closestPackableTransactionAmount:value=>{record('pack:amount',[value]);return `amount:${value}`;},
  closestPackableTransactionFee:value=>{record('pack:fee',[value]);return `fee:${value}`;},
};
for(const [method,kind] of [['newContract','Contract'],['newTransfer','Transfer'],['newWithdraw','Withdraw'],['newChangePubkey','ChangePubKey']]) {
  sdk[method]=builder=>{assert.equal(builder.kind,`builder:${kind}`);record(`new:${kind}`,[]);return builder;};
}
const signed={nativeSignature:'signature-é-😀',nonce:97.25};
const signer={
  createSignedContract:tx=>{assert.equal(tx.kind,'builder:Contract');record('sign:Contract',[]);return signed;},
  createSignedOrder:tx=>{assert.equal(tx.kind,'builder:Order');record('sign:Order',[]);return signed;},
  signTransfer:async(tx,symbol)=>{assert.equal(tx.kind,'builder:Transfer');record('sign:Transfer',[symbol]);return signed;},
  signWithdraw:async(tx,symbol)=>{assert.equal(tx.kind,'builder:Withdraw');record('sign:Withdraw',[symbol]);return signed;},
  signChangePubkeyWithEthEcdsaAuth:async tx=>{assert.equal(tx.kind,'builder:ChangePubKey');record('sign:ChangePubKey',[true]);return signed;},
  signChangePubkeyWithOnchain:async tx=>{assert.equal(tx.kind,'builder:ChangePubKey');record('sign:ChangePubKey',[false]);return signed;},
  pubkeyHash:()=>{record('pubkeyHash',[]);return '0x'+'1'.repeat(40);},
};
const hostDate=class extends Date {static now(){record('clock',[]);return 1700001234567;}};
const compiled=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const api=new vm.Script(compiled+'\n({signContract,signOrder,signTransfer,signWithdraw,signChangePubKey,initSdk})').runInNewContext({exports:{},Array,sdk,Date:hostDate,BigNumber:dependency('@ethersproject/bignumber').BigNumber,isAddress:dependency('ethers').isAddress,init:async()=>{record('init',[]);}});
const exchangeFile=path.resolve(root,'../../apexomni.exchange.ts');const exchangeText=fs.readFileSync(exchangeFile,'utf8');const exchangeTree=ts.createSourceFile(exchangeFile,exchangeText,ts.ScriptTarget.Latest,true);const exchangeClass=exchangeTree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='ApexOmniExchange');const wrapperText=exchangeClass.members.find(n=>n.name?.getText(exchangeTree)==='signOrder').initializer.getText(exchangeTree);const wrapperCode=ts.transpileModule('const run='+wrapperText+';run;',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const exchangeSignOrder=new vm.Script(wrapperCode).runInNewContext({signContract:api.signContract});
function bits(value){const b=Buffer.alloc(8);b.writeDoubleBE(value);return Number.isNaN(value)?'7ff8000000000000':b.toString('hex');}
function native(value){if(value===undefined)return {kind:'undefined'};if(value===null)return {kind:'null'};if(typeof value==='number')return {kind:'number',bits:bits(value)};if(Array.isArray(value))return {kind:'array',values:value.map(native)};if(typeof value==='object')return {kind:'object',values:Object.entries(value).map(([k,v])=>[k,native(v)])};return {kind:typeof value,value};}
const address='0x0000000000000000000000000000000000000001';
const bases={
  signContract:{accountId:11.25,subAccountId:22.25,slotId:33.25,nonce:44.25,pairId:55.25,size:'size',price:'price',direction:true,feeRates:[-12.25,34.25]},
  signOrder:{accountId:11.25,subAccountId:22.25,slotId:33.25,nonce:44.25,baseTokenId:55.25,quoteTokenId:66.25,amount:'amount',price:'price',isSell:false,feeRates:[-12.25,34.25]},
  signTransfer:{accountId:11.25,to:address,fromSubAccountId:22.25,toSubAccountId:33.25,tokenId:44.25,tokenSymbol:'TOKEN',amount:'amount',fee:'fee',nonce:55.25},
  signWithdraw:{toChainId:11.25,withdrawToL1:true,subAccountId:22.25,to:address,l2SourceTokenId:33.25,l2SourceTokenSymbol:'TOKEN',l1TargetTokenId:44.25,tokenDecimals:18,amount:'amount',withdrawFeeRatio:55.25,accountId:66.25,fee:'fee',nonce:77.25},
  signChangePubKey:{chainId:11.25,accountId:22.25,subAccountId:33.25,feeTokenId:44.25,ethAuthType:'Onchain',fee:'fee',nonce:55.25},
};
async function main(){
  const rows=[];
  for(const [name,base] of Object.entries(bases)){
    const changes=[{}, {accountId:0}, {nonce:-1}];
    if(name==='signContract'||name==='signOrder')changes.push({hasSubsidy:false},{feeRates:[12.25,34.25]},{feeRates:[-1,256]});
    else if(name==='signChangePubKey')changes.push({ethAuthType:'EthECDSA',newPkHash:'0x'+'2'.repeat(40),ts:1700000001},{ethAuthType:'EthCreate2',ts:0},{newPkHash:'bad'});
    else changes.push({ts:1700000001},{ts:0},{to:'bad'});
    for(const change of changes){const payload={...base,...change};operations=[];let result;try{result={ok:true,value:native(await api[name](signer,payload))};}catch(error){result={ok:false,message:error.message};}rows.push({name,raw:JSON.stringify(payload),operations:native(operations),result});}
  }
  const wrapperRows=[];
  const wrapperSigner={...signer,createSignedContract:tx=>{assert.equal(tx.kind,'builder:Contract');record('sign:Contract',[]);return {...signed,signature:{signature:'wrapper-signature-é-😀'}};}};
  for(const change of [{},{accountId:0},{nonce:-1},{hasSubsidy:false},{feeRates:[12.25,34.25]},{feeRates:[-1,256]}]){const payload={...bases.signContract,...change};operations=[];let result;try{result={ok:true,value:native(await exchangeSignOrder(wrapperSigner,payload,'ignored-prefix'))};}catch(error){result={ok:false,message:error.message,name:error.name};}wrapperRows.push({raw:JSON.stringify(payload),operations:native(operations),result});}
  operations=[];let missing;try{missing={ok:true,value:native(await exchangeSignOrder(signer,bases.signContract))};}catch(error){missing={ok:false,message:error.message,name:error.name};}wrapperRows.push({raw:JSON.stringify(bases.signContract),operations:native(operations),result:missing});
  assert.equal(wrapperRows.length,7,'exchange signOrder floor');assert(wrapperRows[0].result.ok,'exchange signOrder baseline succeeds');
  operations=[];await api.initSdk();await api.initSdk();const initialization=native(operations);
  assert.equal(rows.length,30,'signing case floor');
  for(const name of Object.keys(bases))assert(rows.find(row=>row.name===name).result.ok,`${name} source baseline succeeds`);
  console.log(JSON.stringify({rows,wrapperRows,initialization,header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH}}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
