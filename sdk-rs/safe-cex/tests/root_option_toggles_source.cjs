// Later source supplement; retained root_services_oracle.cjs stays unchanged.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const metadata=load(path.join(root,'packages/safe-cex/src/exchangeMetadata.ts'));
const routing=load(path.join(root,'packages/safe-cex/src/transport/tealstreet/transportResolution.ts'));
const site=load(path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.site.ts'));
function evaluate(relative,extra,params,values,names) {
 const file=path.join(root,relative),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const statements=ast.statements.filter(n=>!ts.isImportDeclaration(n)&&!ts.isExportDeclaration(n)&&(!names|| (ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast)))) || (ts.isFunctionDeclaration(n)&&names.includes(n.name.text))));
 if(names) assert.equal(statements.length,names.length,'FACTORY independently expected selected declaration count');
 const code=ts.transpileModule(statements.map(n=>n.getText(ast)).join('\n')+extra,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const exportsValue={};new Function('exports',...params,code)(exportsValue,...values);return exportsValue;
}

const provider={publicCorsProxy:()=>undefined,serverProxyUrl:()=>undefined};
let warnings=[];
const account=evaluate('packages/safe-cex/src/createExchangeFromAccount.ts','',['getExchangeMetadata','resolveAccountTransport','normalizeBybitSiteId','getTransport','createExchange','process','console'],[metadata.getExchangeMetadata,routing.resolveAccountTransport,site.normalizeBybitSiteId,()=>provider,()=>{throw Error('unexpected constructor')},{env:{}},{warn:message=>warnings.push(message)}]);
if(process.argv.includes('--hyro-modes')) {
 const rows=[];
 for(const exchange of ['hyrotrader','bybit'])for(const demo of ['absent',false,true])for(const override of ['absent','undefined','demo','production']) {
  const input={exchange,name:'hyro-mode-account',apiKey:'fixture-key',apiSecret:'fixture-secret',...(demo==='absent'?{}:{hyrotraderDemo:demo})};
  const patch=override==='absent'?undefined:{extra:{hyrotraderMode:override==='undefined'?undefined:override}};
  const result=account.accountToExchangeOptions(input,patch);
  rows.push({account:input,override,extra:result.extra??null});
 }
 assert.equal(rows.length,24);
 process.stdout.write(JSON.stringify(rows));
 process.exit(0);
}
const fields=['enableSpotMarkets','enableOptionMarkets','enableInverseMarkets'];
const names=Object.keys(metadata.exchangeMetadata);assert.equal(names.length,49,'all declared exchange metadata entries');
const rows=[];
for(const exchange of names)for(const field of fields)for(const enabled of ['absent',false,true])for(const override of ['absent','undefined',false,true]){
 const input={exchange,name:'toggle-witness',apiKey:'',apiSecret:'',...(enabled==='absent'?{}:{[field]:enabled})};
 const patch=override==='absent'?undefined:{[field]:override==='undefined'?undefined:override};
 warnings=[];
 const value=account.accountToExchangeOptions(input,patch);
 const expected=Object.fromEntries(fields.filter(key=>Object.hasOwn(value,key)&&value[key]!==undefined).map(key=>[key,value[key]]));
 rows.push({account:input,field,override,expected,warnings});
}
assert.equal(rows.length,1764,'49 venues x 3 independent toggles x 3 inputs x 4 override states');
assert.equal(rows.some(row=>row.field==='enableOptionMarkets'&&row.expected.enableOptionMarkets===true),true);
assert.equal(rows.some(row=>row.field==='enableInverseMarkets'&&row.expected.enableInverseMarkets===true),true);
process.stdout.write(JSON.stringify(rows));
