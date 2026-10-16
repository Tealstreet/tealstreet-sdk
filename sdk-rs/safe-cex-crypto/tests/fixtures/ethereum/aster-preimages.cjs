// Execute the actual private TS helpers, tracing their SDK hashing boundaries.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),vm=require('node:vm'),crypto=require('node:crypto');
const {oracleRequire,verifyPackage}=require('../oracle-packages.cjs');verifyPackage('ethers');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH,'SAFE_CEX_TYPESCRIPT_PATH must select the existing isolated compiler');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),ethers=oracleRequire('ethers');
const root=path.resolve(__dirname,'../../../../..'),sourcePath='packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts';
const source=fs.readFileSync(path.join(root,sourcePath),'utf8'),file=ts.createSourceFile(sourcePath,source,ts.ScriptTarget.Latest,true);
const names=new Set(['buildV3SigningPayload','generateV3Signature']),selected=[];
for(const statement of file.statements)if(ts.isVariableStatement(statement))for(const declaration of statement.declarationList.declarations)if(ts.isIdentifier(declaration.name)&&names.has(declaration.name.text)){selected.push(`const ${declaration.getText(file)};`);names.delete(declaration.name.text);}
assert.equal(names.size,0,'actual TS signing helpers must resolve');
const js=ts.transpileModule(selected.join('\n')+'\nmodule.exports={generateV3Signature};',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const key='0x'+'11'.repeat(32),user='0x52908400098527886e0f7030069857d2e4169ee7',signer='0xde709f2102306220921060314715629080e2fb77',cases=[];
for(const [name,params,nonce] of [['complete',{symbol:'ETHUSDT',quantity:'1',price:'1000'},'1700000000000000'],['omitted',{z:undefined,a:null,b:'',keep:'0',false:false},'9007199254740993'],['unicode',{z:'🙂\0é',a:'a&b'},'1'],['keys',{2:'two',10:'ten',z:'last',a:'first'},'0']]) {
 const trace={},module={exports:{}};
 const traced={...ethers,AbiCoder:{defaultAbiCoder:()=>({encode:(types,values)=>{trace.types=types;trace.values=values.map(v=>typeof v==='bigint'?v.toString():v);trace.abi=ethers.AbiCoder.defaultAbiCoder().encode(types,values);return trace.abi;}})},keccak256:bytes=>{trace.keccak=ethers.keccak256(bytes);return trace.keccak;},hashMessage:bytes=>{trace.messageHash=ethers.hashMessage(bytes);return trace.messageHash;},SigningKey:class extends ethers.SigningKey {sign(hash){trace.signDigest=hash;return super.sign(hash);}}};
 vm.runInNewContext(js,{module,exports:module.exports,ethers:traced});
 const signature=module.exports.generateV3Signature(params,user,signer,key,nonce);
 cases.push({name,params:Object.fromEntries(Object.entries(params).filter(([,v])=>v!==undefined)),nonce,key,...trace,signature});
}
const result=JSON.stringify({oracle:'actual TypeScript generateV3Signature/buildV3SigningPayload; preimages at ethers boundaries',sourcePath,sourceSha256:crypto.createHash('sha256').update(source).digest('hex'),cases},null,2)+'\n',target=path.join(__dirname,'aster-preimages.json');
if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(target,'utf8'),'actual Aster TS signing preimage drift');
