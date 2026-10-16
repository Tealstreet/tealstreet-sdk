const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
const text=fs.readFileSync(file,'utf8'),source=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
const names=['COINBASE_KEY_NAME_PATTERN','COINBASE_PRIVATE_KEY_PATTERN','COINBASE_ED25519_PREFIX','getCoinbaseJsonSecret','parseCoinbaseCredentialJson','base64ToBytes','normalizeCoinbaseApiCredentials'];
const selected=source.statements.filter(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>names.includes(d.name.getText(source))));
assert.equal(selected.length,names.length,'all credential declarations must exist');
const exportsObject={};
vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(source)).join('\n')+'\nexport {getCoinbaseJsonSecret,parseCoinbaseCredentialJson,base64ToBytes,normalizeCoinbaseApiCredentials};',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject,atob});
const values=[undefined,null,false,true,0,1,'','seed',[],{},['seed']];
const fields=['privateKey','private_key','keySecret','key_secret','secret'];
const secrets=[...values];
for(const key of fields)for(const value of values){
 secrets.push({[key]:value});
 for(const later of fields)secrets.push({[key]:value,[later]:'later'});
}
const encoded=['','=', '==','===','A','AB','ABC','ABCD','Zg','Zg=','Zg==','Zg===','Zg====','Zg==x','Zh==','Z/==','YWJj','YWJj=','-___','+/+/','a.b','null','💰'];
for(let length=0;length<=65;length++)encoded.push(Buffer.from(Array.from({length},(_,i)=>(i*37+length)%256)).toString('base64'));
const inputs=[];
for(const value of encoded)for(const prefix of ['','ed25519:','ED25519:','Ed25519:',' ed25519:','ed25519:ed25519:']){
 inputs.push(prefix+value);
 for(const space of [' ','\t','\n','\r','\v','\f','\u00a0','\uFEFF','\u0085','\u2028','\u2029'])inputs.push(prefix+space+Array.from(value).slice(0,1).join('')+space+Array.from(value).slice(1).join('')+space);
}
const credentialInputs=['',' {"privateKey":"pem"} ','\uFEFF{"name":"k"}','\u0085{"name":"k"}','[]','null','true','"x"','{','{}',' {"x":1,"private_key":"secret"} ','{"x":"\\u0041"}','{"x":1} trailing'];
const credentialJsonSurrogateInput='{"x":"\\ud800"}';
const keyName='organizations/org-1/apiKeys/key-2';
const pem='-----BEGIN PRIVATE KEY-----\\nabc\\n-----END PRIVATE KEY-----';
const normalizationPairs=[
 {label:'pem-secret',key:keyName,secret:pem},
 {label:'pem-json-secret',key:keyName,secret:JSON.stringify({privateKey:pem})},
 {label:'pem-json-key-name',key:JSON.stringify({name:` ${keyName} `}),secret:pem},
 {label:'escaped-pem-newlines',key:keyName,secret:pem.replaceAll('\\n','\\\\n')},
 {label:'seed-32',key:keyName,secret:Buffer.from(Array.from({length:32},(_,i)=>i)).toString('base64')},
 {label:'seed-64-prefixed',key:keyName,secret:`ED25519:${Buffer.from(Array.from({length:64},(_,i)=>255-i)).toString('base64')}`},
 {label:'wrong-key-prefix',key:` ${keyName}`,secret:pem},
 {label:'empty-organization',key:'organizations//apiKeys/id',secret:pem},
 {label:'empty-key-id',key:'organizations/org/apiKeys/',secret:pem},
 {label:'extra-key-segment',key:'organizations/org/apiKeys/id/extra',secret:pem},
 {label:'wrong-key-name-type',key:JSON.stringify({name:1}),secret:pem},
 {label:'bad-secret-length',key:keyName,secret:Buffer.from([1,2,3]).toString('base64')},
 {label:'bad-secret-characters',key:keyName,secret:'abc?'},
 {label:'json-secret-alias',key:keyName,secret:JSON.stringify({secret:Buffer.from(Array.from({length:32},(_,i)=>i+1)).toString('base64')})},
 {label:'json-key-secret-fallback',key:JSON.stringify({name:keyName,key_secret:Buffer.from(Array.from({length:32},(_,i)=>i+2)).toString('base64')}),secret:'invalid'},
 {label:'json-key-name-and-secret',key:JSON.stringify({name:keyName,private_key:pem}),secret:'invalid'},
 {label:'invalid-json-key',key:'{"name":',secret:pem},
 {label:'pem-empty-body',key:keyName,secret:'-----BEGIN PRIVATE KEY----------END PRIVATE KEY-----'},
 {label:'empty-values',key:'',secret:''},
];
const recoveryNormalization=normalizationPairs.map(({label,key,secret})=>{
 try {const value=exportsObject.normalizeCoinbaseApiCredentials(key,secret);return {label,key,secret,result:{...value,ed25519Seed:value.ed25519Seed===undefined?null:Array.from(value.ed25519Seed)}};}
 catch(error) {return {label,key,secret,error:String(error?.message??error)}};
});
const validKey='organizations/org-1/apiKeys/key-1';
const validPem='-----BEGIN EC PRIVATE KEY-----\nAQID\n-----END EC PRIVATE KEY-----';
const normalizationInputs=[];
for(const key of [validKey,` ${validKey}\uFEFF`,`${validKey}/extra`,'organizations//apiKeys/key',JSON.stringify({name:validKey}),JSON.stringify({name:7})]){
 for(const secret of [validPem,'-----BEGIN PRIVATE KEY-----x-----END EC PRIVATE KEY-----','-----BEGIN EC PRIVATE KEY----------END PRIVATE KEY-----',Buffer.from(Array.from({length:32},(_,i)=>i+1)).toString('base64'),`ed25519:${Buffer.from(Array.from({length:64},(_,i)=>i+1)).toString('base64')}`,JSON.stringify({private_key:validPem}),JSON.stringify({secret:'invalid'}),'','not-json']) normalizationInputs.push({key,secret});
}
const normalized=normalizationInputs.map(input=>{try{const value=exportsObject.normalizeCoinbaseApiCredentials(input.key,input.secret);return {...input,result:{...value,ed25519Seed:value.ed25519Seed?Array.from(value.ed25519Seed):undefined}}}catch(error){return {...input,error:{name:error.name,message:error.message}}}});
const parseInputs=['','  {"a":1} ','\uFEFF{"a":1}','null','[]','{}',' { invalid','true','{"a":1} trailing'];
const surrogateNormalizationKey='{"name":"organizations/org/apiKeys/id","ignored":"\\ud800"}';
const surrogateNormalizationSecret='-----BEGIN EC PRIVATE KEY-----\nAQID\n-----END EC PRIVATE KEY-----';
const surrogateNormalizationValue=exportsObject.normalizeCoinbaseApiCredentials(surrogateNormalizationKey,surrogateNormalizationSecret);
const surrogateNormalization={key:surrogateNormalizationKey,secret:surrogateNormalizationSecret,keyName:surrogateNormalizationValue.keyName,algorithm:surrogateNormalizationValue.algorithm};
process.stdout.write(JSON.stringify({secrets:secrets.map(value=>({...value===undefined?{}:{input:value},result:exportsObject.getCoinbaseJsonSecret(value)??null})),base64:inputs.map(input=>({input,result:(v=>v===undefined?null:Array.from(v))(exportsObject.base64ToBytes(input))})),credentialJson:credentialInputs.map(input=>({input,result:exportsObject.parseCoinbaseCredentialJson(input)??null})),credentialJsonSurrogateAccepted:exportsObject.parseCoinbaseCredentialJson(credentialJsonSurrogateInput)!==undefined,recoveryNormalization,surrogateNormalization,parse:parseInputs.map(input=>({input,result:exportsObject.parseCoinbaseCredentialJson(input)??null})),normalization:normalized}));
