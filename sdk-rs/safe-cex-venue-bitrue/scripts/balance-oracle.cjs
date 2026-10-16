const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root}=require('../tests/configuration.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitrueExchange');
const members=klass.members.filter(n=>n.name?.getText(source)==='parseBalance');
assert.equal(members.length,1,'actual parseBalance initializer census');
const code=ts.transpileModule(`return (${members[0].initializer.getText(source)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const stored={total:901,free:402,used:503,upnl:-17};
const parse=new Function(code).call({store:{balance:stored}});
function bits(n){if(Number.isNaN(n))return 'NaN';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function native(v){if(v===null)return {type:'null'};if(Array.isArray(v))return {type:'array',items:v.map(native)};switch(typeof v){case 'number':return {type:'number',bits:bits(v)};case 'undefined':return {type:'undefined'};case 'object':return {type:'object',entries:Object.entries(v).map(([k,x])=>[k,native(x)])};default:return {type:typeof v,value:v};}}
function fields(b){return {total:native(b.total),free:native(b.free),used:native(b.used),upnl:native(b.upnl)};}
function observe(data){try{const b=parse(data);return {type:'balance',stored:b===stored,fields:fields(b),currencies:b.currencies===undefined?null:Object.entries(b.currencies).map(([k,v])=>[k,fields(v)]),info:b.info===undefined?null:native(b.info)};}catch(e){return {type:'error',name:e.name,message:e.message};}}
const rows=[],refusals=[];
const base={marginCoin:'USDT',totalEquity:31,accountNormal:7,accountLock:11,sumMaintenanceMargin:13,totalPositionNormal:17,totalCost:19,sumOpenRealizedAmount:-23};
const encode=accounts=>JSON.stringify({data:{account:accounts}});
function add(raw,label){rows.push({raw,label,result:observe(raw===null?undefined:JSON.parse(raw))});}
for(const raw of [null,'null','false','0','-0','""','true','1','[]','{}','{"data":null}','{"data":{}}','{"data":{"account":null}}','{"data":{"account":{}}}',encode([]),encode([null]),encode([false,0,'x',{}]),encode([base]),encode([{...base,marginCoin:'USDC'}])])add(raw,'shape');
add(encode([{...base,marginCoin:'USDC',totalEquity:111,accountNormal:12,sumOpenRealizedAmount:29},base]),'primary USDT after USDC');
add(encode([base,{...base,totalEquity:61,accountNormal:8,sumOpenRealizedAmount:37}]),'duplicate replacement');
add(encode([{marginCoin:'BTC',totalEquity:{toString:1}},base]),'irrelevant coin');
const fieldsToConvert=['totalEquity','accountNormal','accountLock','sumMaintenanceMargin','totalPositionNormal','totalCost'];
for(const field of fieldsToConvert) {
 for(const raw of [null,'null','false','true','0','-0','-5','0.37','1e400','-1e400','1e-400','9007199254740993','""','"bad"','"31.75"','[]','[3]','{}','{"toString":1}']) {
  const a={...base};delete a[field];const text=JSON.stringify(a).slice(0,-1)+(raw===null?'':`,"${field}":${raw}`)+'}';
  add(`{"data":{"account":[${text}]}}`,field);
 }
 const a={...base,totalEquity:0,accountNormal:0,accountLock:0,sumMaintenanceMargin:0,totalPositionNormal:0,totalCost:0};a[field]=field==='accountNormal'?-91:91;add(encode([a]),'independent used candidate '+field);
}
for(const raw of ['-0','1e400','-1e400','9007199254740993'])add(encode([base]).replace('"sumOpenRealizedAmount":-23',`"sumOpenRealizedAmount":${raw}`),'native upnl');
add(encode([{...base,totalEquity:'bad',accountNormal:{toString:1}}]),'map before reduction');
for(const raw of [null,'null','false','"17"','[]','{}']) {
 const a={...base};delete a.sumOpenRealizedAmount;const text=JSON.stringify(a).slice(0,-1)+(raw===null?'':`,"sumOpenRealizedAmount":${raw}`)+'}';
 const input=`{"data":{"account":[${text}]}}`;
 refusals.push({raw:input,result:observe(JSON.parse(input))});
}
assert(refusals.every(r=>r.result.type==='balance'),'source permits raw nonnumeric upnl');
add(encode([{...base,sumOpenRealizedAmount:'raw'},base]),'overwritten raw upnl');
add(encode([{...base,sumOpenRealizedAmount:'raw'},{...base,marginCoin:'USDC',totalEquity:{toString:1}}]),'later coercion beats output refusal');
const utf8Raw=encode([base]).replace('"data":','"extra":"\\ud800","data":');
assert.equal(parse(JSON.parse(utf8Raw)).info.extra.charCodeAt(0),55296,'source balance retains surrogate info');
const depthRaw=encode([base]).replace('"data":',`"extra":${'['.repeat(160)}3${']'.repeat(160)},"data":`);
assert.equal(parse(JSON.parse(depthRaw)).total,31,'source balance retains deep info');
process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),stored,rows,refusals,utf8Raw,depthRaw}));
