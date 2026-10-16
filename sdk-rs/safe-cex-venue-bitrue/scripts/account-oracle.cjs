const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root,load}=require('../tests/configuration.cjs');
const PositionSide=load(path.join(root,'packages/safe-cex/src/types.ts')).PositionSide;
const file=path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitrueExchange');
const context={};
for(const name of ['getPositionAccounts','getAccountPositionSnapshots']) {
 const members=klass.members.filter(n=>n.name?.getText(source)===name);
 assert.equal(members.length,1,`actual initializer census: ${name}`);
 assert(ts.isArrowFunction(members[0].initializer));
 const code=ts.transpileModule(`return (${members[0].initializer.getText(source)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 context[name]=new Function('PositionSide',code).call(context,PositionSide);
}
function bits(n){if(Number.isNaN(n))return 'NaN';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function native(v){if(v===null)return {type:'null'};if(Array.isArray(v))return {type:'array',items:v.map(native)};switch(typeof v){case 'number':return {type:'number',bits:bits(v)};case 'undefined':return {type:'undefined'};case 'object':return {type:'object',entries:Object.entries(v).map(([k,x])=>[k,native(x)])};default:return {type:typeof v,value:v};}}
function observe(method,data){try{const value=context[method](data);if(value===undefined)return {type:'undefined'};if(method==='getPositionAccounts')return {type:'accounts',indices:value.map(account=>data.data.account.indexOf(account))};return {type:'snapshots',values:value.map(p=>({symbol:native(p.symbol),side:native(p.side),leverageLevel:native(p.leverageLevel),positionType:native(p.positionType)}))};}catch(e){return {type:'error',name:e.name};}}
const rows=[];
function add(raw,label){const data=raw===null?undefined:JSON.parse(raw);rows.push({raw,label,accounts:observe('getPositionAccounts',data),snapshots:observe('getAccountPositionSnapshots',data)});}
const position=(side='BUY')=>({side,volume:3,openPrice:101,leverageLevel:7,positionType:2});
const vo=(name='E-BTC-USDT',positions=[position()])=>({contractName:name,positions});
const account=(coin='USDT',positionVos=[vo()])=>({marginCoin:coin,positionVos});
const data=accounts=>({data:{account:accounts}});
const encode=v=>JSON.stringify(v);
const envelopes=[
 undefined,null,{},[],false,{data:{}},{data:{account:null}},data([]),
 data([null,false,0,{},account('BTC')]),
 data([account('USDT',[])]),
 data([account('USDT',[vo('BTC',[])])]),
 data([account('BTC'),account('USDC',[vo('ETH',[position('SELL')])]),account('USDT')]),
 data([account('USDT'),account('USDC',[vo('ETH',[{...position('SELL'),leverageLevel:13,positionType:1}])])]),
];
for(const input of envelopes)add(input===undefined?null:encode(input),'envelope');
for(const bad of [undefined,null,false,0,'x',{},[]]) {
 const a=account();if(bad===undefined)delete a.positionVos;else a.positionVos=bad;
 add(encode(data([a])),'positionVos');
 const item=vo();if(bad===undefined)delete item.positions;else item.positions=bad;
 add(encode(data([account('USDT',[item])])),'positions');
 const named=vo();if(bad===undefined)delete named.contractName;else named.contractName=bad;
 add(encode(data([account('USDT',[named])])),'contractName');
 add(encode(data([account('USDT',[bad])])),'positionVo');
 add(encode(data([account('USDT',[vo('BTC',[bad])])])),'position');
}
for(const side of [undefined,null,false,1,'buy','SELL','BUY','']) {
 const p=position();if(side===undefined)delete p.side;else p.side=side;
 add(encode(data([account('USDT',[vo('BTC',[p])])])),'side');
}
for(const field of ['volume','openPrice','leverageLevel','positionType']) {
 for(const raw of [null,'null','false','true','0','-0','-1','1.5','1e400','-1e400','1e-400','9007199254740993','""','"3"','"0x10"','"bad"','[]','[3]','[null]','[1,2]','{}','{"toString":1}']) {
  const p=position();delete p[field];const text=encode(p).slice(0,-1)+(raw===null?'':`,"${field}":${raw}`)+'}';
  add(`{"data":{"account":[{"marginCoin":"USDT","positionVos":[{"contractName":"BTC","positions":[${text}]}]}]}}`,field);
 }
}
// Different inputs distinguish source short-circuit order, even where finite
// validation would otherwise make swapping two numeric getters symmetric.
for(const [volume,openPrice] of [['bad',{toString:1}],[{toString:1},'bad']])add(encode(data([account('USDT',[vo('BTC',[{...position(),volume,openPrice}])])])),'coercion order');
for(const [leverageLevel,positionType] of [['11','2'],[[17],true],[true,null],[5.5,-0]])add(encode(data([account('USDT',[vo('BTC',[{...position(),leverageLevel,positionType}])])])),'raw snapshot types');
// No partial account results if a later eligible account is malformed.
add(encode(data([account('USDT'),{marginCoin:'USDC'}])),'later invalid account');
add(encode(data([{marginCoin:'BTC'},account('USDT')])),'irrelevant invalid account');
const surrogateRaw='{"data":{"account":[{"marginCoin":"USDT","positionVos":[{"contractName":"\\ud800","positions":[{"side":"BUY","volume":3,"openPrice":101,"leverageLevel":7,"positionType":2}]}]}]}}';
const surrogate=JSON.parse(surrogateRaw);
assert.equal(context.getPositionAccounts(surrogate).length,1,'source accepts surrogate contractName');
assert.equal(context.getAccountPositionSnapshots(surrogate)[0].symbol.charCodeAt(0),55296,'source snapshot retains surrogate');
// Separate bounded-ingress supplement; do not mix it into parity rows.
const depthRaw=encode(data([account('USDT',[vo('BTC',[{...position(),volume:JSON.parse('['.repeat(160)+'3'+']'.repeat(160))}])])]));
const deep=JSON.parse(depthRaw);
assert.equal(context.getPositionAccounts(deep).length,1,'source accepts depth-160 volume');
assert.equal(context.getAccountPositionSnapshots(deep)[0].leverageLevel,7,'source snapshots depth-160 volume');
process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),rows,surrogateRaw,depthRaw}));
