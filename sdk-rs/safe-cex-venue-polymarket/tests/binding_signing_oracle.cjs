// Actual maintained signing helper + pinned official SDK; no signing replacements.
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const assert=require('node:assert/strict');
const {createRequire}=require('node:module');
const root=path.resolve(__dirname,'../../..');
const deps=createRequire(path.join(root,'package.json'));
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root,'node_modules/typescript'));
const directory=path.join(root,'packages/safe-cex/src/exchanges/polymarket');
const compiler={module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022};
function execute(text,file,require){const exports={};const code=ts.transpileModule(text,{compilerOptions:compiler}).outputText;vm.runInNewContext('(function(require,exports){'+code+'\n})',{Date,Math,Uint8Array,Buffer},{filename:file})(require,exports);return exports;}
async function main(){
  const sdk=await import(deps.resolve('@polymarket/clob-client-v2'));
  const typesFile=path.join(root,'packages/safe-cex/src/types.ts');
  const ast=ts.createSourceFile(typesFile,fs.readFileSync(typesFile,'utf8'),ts.ScriptTarget.Latest,true);
  const declarations=ast.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderType','OrderSide'].includes(n.name.text));
  assert.equal(declarations.length,2,'actual source enums');
  const enums=execute(declarations.map(n=>n.getText(ast)).join('\n'),typesFile,()=>{throw Error('enum import');});
  const cache=new Map();
  function load(name){if(cache.has(name))return cache.get(name);const file=path.join(directory,name+'.ts');const result=execute(fs.readFileSync(file,'utf8'),file,id=>{
    if(id.startsWith('./polymarket.'))return load(id.slice(2));
    if(id==='../../types')return enums;
    if(id==='@polymarket/clob-client-v2')return sdk;
    if(id==='ethers')return deps('ethers');
    throw Error('Unexpected import '+id);
  });cache.set(name,result);return result;}
  const signing=load('polymarket.signing');
  const key='0x'+'01'.repeat(32);
  const wallet=signing.createWallet(key);
  const now=1700000000123;
  const oldNow=Date.now,oldRandom=Math.random;
  Date.now=()=>now;Math.random=()=>0.5;
  try{
    const cases=[];
    for(const tickSize of ['0.1','0.01','0.005','0.0025','0.001','0.0001'])for(const signatureType of [0,1,2,3])for(const side of ['BUY','SELL'])for(const negRisk of [false,true]){
      const params={tokenId:'123456789012345678901234567890123456789012345678901234567890',price:0.53765,size:[5.123455,7.995,10.02,123456.789][signatureType],side,tickSize,negRisk,signatureType};
      if(signatureType!==0)params.funderAddress='0x1111111111111111111111111111111111111111';
      const order=await signing.createSignedOrder(wallet,params);
      assert.equal(order.signatureType,signatureType,'actual SDK signature type');
      const payload=sdk.orderToJsonV2(order,'account-key','GTC');
      cases.push({params,order,payload,payloadString:JSON.stringify(payload),keys:Object.keys(order)});
    }
    const auth=await signing.signApiKeyDerivationMessage(wallet,0,1700000000);
    const l2=signing.buildL2Headers({apiKey:'account-key',secret:'c2VjcmV0',passphrase:'account-passphrase'},wallet.address,'post','/order','{"orderID":"abc"}',1700000000);
    process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),key,now,cases,auth,l2}));
  }finally{Date.now=oldNow;Math.random=oldRandom;}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
