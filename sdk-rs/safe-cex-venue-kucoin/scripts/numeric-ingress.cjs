// Live capture architecture and Node major: captureArchitecture/captureNodeMajor per record.
// Compile assignments against the live declarations, then inspect JSON.parse bits.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const {bits:captureBits}=require('./numeric-bits.cjs');
const declarations=[];
for(const name of ['kucoinspot.types.ts','kucoin.uta-account-mode.ts']) {
 const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin',name);
 let text=fs.readFileSync(file,'utf8');
 if(process.env.KUCOIN_NUMERIC_INGRESS_MUTATION==='source-string'&&name==='kucoinspot.types.ts') {
  if(!text.includes('pingInterval: number'))throw Error('missing numeric source control');
  text=text.replace('pingInterval: number','pingInterval: string');
 }
 const ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
 for(const node of ast.statements)if(ts.isTypeAliasDeclaration(node)||(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>d.name.getText(ast)==='KUCOIN_UTA_ACCOUNT_MODES')))declarations.push(node.getText(ast));
}
const rows=[];
if(process.env.KUCOIN_INGRESS_CORPUS!=='strings')
for(const kind of ['server','data','response','mode'])for(const field of [0,1])for(const literal of ['0','1','1.5','1e3','-0','-0.0','1e400','-1e400','-1e-4000']) {
 const a=field===0?literal:'1',b=field===1?literal:'1';
 const server=`{"endpoint":"url","encrypt":false,"protocol":"ws","pingInterval":${a},"pingTimeout":${b}}`;
 const data=`{"token":"t","instanceServers":[${server}]}`;
 const raw=kind==='server'?server:kind==='data'?data:kind==='response'?`{"code":"200000","data":${data}}`:`{"status":"identified","accountMode":"CLASSIC","classicSubAccount":[${a}],"unifiedSubAccount":[${b}]}`;
 const type=kind==='server'?'KucoinSpotWsServer':kind==='data'?"NonNullable<KucoinSpotWsTokenResponse['data']>":kind==='response'?'KucoinSpotWsTokenResponse':'KucoinAccountModeDetection';
 const parsed=JSON.parse(raw);const pair=kind==='mode'?[parsed.classicSubAccount[0],parsed.unifiedSubAccount[0]]:(()=>{const s=kind==='server'?parsed:kind==='data'?parsed.instanceServers[0]:parsed.data.instanceServers[0];return [s.pingInterval,s.pingTimeout];})();
 const bits=pair.map(captureBits);
 rows.push({kind,field,literal,raw,type,bits,captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split(".")[0])});
}

if(process.env.KUCOIN_INGRESS_CORPUS==='strings') {
 for(const [kind,field] of [['server','endpoint'],['server','protocol'],['data','token'],['response','code'],['response','msg']]) {
  for(const literal of ['"\\ud800"','"\\udfff"','"\\ud83d\\ude80"','"café"','" \\ufeff\\t"','"__proto__"']) {
   // literal contains a JSON escape, not a host replacement character.
   const escaped=literal.replaceAll('\\\\','\\');
   const server={endpoint:'url',encrypt:false,protocol:'ws',pingInterval:1,pingTimeout:2};
   const base=kind==='server'?server:kind==='data'?{token:'t',instanceServers:[server]}:{code:'200000',msg:'ok',data:{token:'t',instanceServers:[server]}};
   const value=JSON.parse(escaped);base[field]=value;
   const raw=JSON.stringify(base);
   const type=kind==='server'?'KucoinSpotWsServer':kind==='data'?"NonNullable<KucoinSpotWsTokenResponse['data']>":'KucoinSpotWsTokenResponse';
   rows.push({kind,field,raw,type,stringJson:JSON.stringify(value),units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i)),lone:value.length===1&&value.charCodeAt(0)>=0xd800&&value.charCodeAt(0)<=0xdfff});
  }
 }
}
const text=declarations.join('\n')+'\n'+rows.map((r,i)=>`const probe${i}: ${r.type} = ${r.raw};`).join('\n');
const filename=path.join(__dirname,'virtual-numeric-ingress.ts');
const options={strict:true,noEmit:true,target:ts.ScriptTarget.ES2022};
const host=ts.createCompilerHost(options),original=host.getSourceFile.bind(host);
host.getSourceFile=(file,version,...args)=>file===filename?ts.createSourceFile(file,text,version,true):original(file,version,...args);
const errors=ts.getPreEmitDiagnostics(ts.createProgram([filename],options,host));
if(errors.length)throw Error('live KuCoin numeric type corpus rejected: '+ts.formatDiagnosticsWithColorAndContext(errors,{getCurrentDirectory:()=>process.cwd(),getCanonicalFileName:x=>x,getNewLine:()=> '\n'}));
console.log(JSON.stringify(rows));
