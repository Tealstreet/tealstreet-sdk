const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),moduleApi=require('node:module'),assert=require('node:assert/strict');
const file=path.resolve(__dirname,'signing-oracle.cjs'),source=fs.readFileSync(file,'utf8'),marker='const cases = [';
assert.equal(source.split(marker).length,2,'maintained signer fixture loader boundary');
const probes=JSON.parse(fs.readFileSync(0,'utf8'));
const context={require:moduleApi.createRequire(file),__dirname:path.dirname(file),process,Buffer,console,probes};
vm.createContext(context);vm.runInContext(source.split(marker)[0]+`
const observations=[];
bindings['../../transport/registry'].getTransport=options=>({proxyPrefix(context){observations.push({...context.config});return 'https://proxy.invalid';}});
const result=[];
for(const version of [1,3])for(const [index,input] of probes.entries()){
 const before=observations.length;let entry;
 try{const signed=(version===1?out.signV1:out.signV3)(input,{key:'fixture-key',secret:'fixture-secret'});entry={method:signed.method,url:signed.url,preimage};}
 catch(error){entry={error:error.message,name:error.name};}
 entry.proxyCalls=observations.length-before;assert.equal(entry.proxyCalls,index===5?1:0,"WOO-AUDIT source proxy invocation count");
 entry.proxy=observations.length===before?null:observations.at(-1);result.push(entry);
}
globalThis.result=result;
`,context,{filename:file});
process.stdout.write(JSON.stringify(context.result));
