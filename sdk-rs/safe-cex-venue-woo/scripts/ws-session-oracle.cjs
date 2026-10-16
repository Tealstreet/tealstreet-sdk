'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,extras,imports={}) {
 const source=fs.readFileSync(path.join(root,file),'utf8'),exports={};
 const code=ts.transpileModule(source+'\nexport { '+extras.join(',')+' };',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(code,{exports,require(name){assert.ok(Object.hasOwn(imports,name));return imports[name];}});return exports;
}
const contracts=load('woo.v3.contracts.ts',[]);
const api=load('woo.v3.websocket.ts',['requireBoolean','requireInteger','requireDecimal','requireNonNegativeDecimal','requirePositiveDecimal'],{'./woo.v3.contracts':contracts});
// Observe source numbers before JSON projection can erase -0 or infinity.
function native(value){if(typeof value==='number')return {$number:Object.is(value,-0)?'-0':String(value)};if(Array.isArray(value))return value.map(native);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,native(v)]));return value;}
function capture(fn,...args){try{return {ok:native(fn(...args))};}catch(error){return {error:{name:error.name,message:error.message}};}}
const parse=raw=>raw===undefined?undefined:JSON.parse(raw);
const scalarInputs=[undefined,'null','true','false','0','-0','1','-1','0.5','9007199254740991','9007199254740992','1e999','-1e999','1e-999','-1e-999','"1"','""','[]','{}'];
const scalars=scalarInputs.map(input=>({input,boolean:capture(api.requireBoolean,parse(input),'test.boolean'),integer:capture(api.requireInteger,parse(input),'test.integer'),minimumOne:capture(api.requireInteger,parse(input),'test.integer',1),ping:capture(api.buildWooV3WsPing,parse(input))}));
assert.equal(scalars.length,19,'WS-SESSION scalar floor');
const decimalInputs=[undefined,'null','1','""','"0"','"-0"','"001.20"','"1."','".5"','"-.5"','"0.000"','"0.001"','"+1"','"1e3"','" 1"','"1 "','"1\\n"','"1\\r\\n"','"1\\u2028"','"1.2.3"','"."','"-"','"١"','"1a"','"NaN"'];
const decimals=decimalInputs.map(input=>({input,decimal:capture(api.requireDecimal,parse(input),'test.decimal'),nonNegative:capture(api.requireNonNegativeDecimal,parse(input),'test.decimal'),positive:capture(api.requirePositiveDecimal,parse(input),'test.decimal')}));
assert.equal(decimals.length,25,'WS-SESSION decimal floor');
const good={success:true,timestamp:11,data:{authKey:'distinct/key',expiredTime:37}};
const listens=[undefined,'null','[]','{}',JSON.stringify(good),'{"success":true,"timestamp":-0,"data":{"authKey":"zero","expiredTime":1}}',JSON.stringify({...good,success:false}),JSON.stringify({...good,success:'true'}),JSON.stringify({...good,timestamp:0.5,data:null}),JSON.stringify({...good,data:null}),JSON.stringify({...good,data:{authKey:'',expiredTime:11}}),JSON.stringify({...good,data:{authKey:'',expiredTime:12}}),JSON.stringify({...good,data:{authKey:'k',expiredTime:0}}),JSON.stringify({...good,data:{authKey:3,expiredTime:12}}),'{"success":true,"timestamp":1e999,"data":{"expiredTime":1}}','{"success":true,"timestamp":0,"data":{"authKey":"max","expiredTime":9007199254740991}}'];
const listen=listens.map(input=>({input,...capture(api.decodeWooV3ListenKey,parse(input))}));assert.equal(listen.length,16);
const pongs=[undefined,'null','[]','{}','{"cmd":"PING","success":false,"time":-1}','{"cmd":"PONG","success":false,"time":-1}','{"cmd":"PONG","success":"true","time":-1}','{"cmd":"PONG","success":true,"time":31}','{"cmd":"PONG","success":true,"time":-0}','{"cmd":"PONG","success":true,"time":0.5}','{"cmd":"PONG","success":true,"time":1e999}','{"cmd":"PONG","success":true}'];
const pong=pongs.map(input=>({input,...capture(api.decodeWooV3WsPong,parse(input))}));assert.equal(pong.length,12);
const urls=[...['livenet','testnet'].flatMap(environment=>['key',"a b+/=?#&é𐐀-_.!~*'()"].map(key=>({environment,input:JSON.stringify(key)}))),{environment:'fallback',input:'"key"'},{environment:'livenet',input:'""'},{environment:'testnet',input:'3'},{environment:'testnet'}].map(spec=>({...spec,...capture(api.buildWooV3PrivateWebSocketUrl,spec.environment,parse(spec.input))}));assert.equal(urls.length,8);
const request=api.buildWooV3ListenKeyRequest();assert.ok(Object.isFrozen(request)&&Object.isFrozen(request.body));
assert.ok(Object.isFrozen(api.WOO_V3_WS_HEARTBEAT));
for(const row of listen)if(row.ok)assert.ok(Object.isFrozen(api.decodeWooV3ListenKey(parse(row.input))));
for(const row of pong)if(row.ok)assert.ok(Object.isFrozen(api.decodeWooV3WsPong(parse(row.input))));
for(const row of scalars)if(row.ping.ok)assert.ok(Object.isFrozen(api.buildWooV3WsPing(parse(row.input))));
console.log(JSON.stringify({scalars,decimals,listen,pong,urls,request:native(request),heartbeat:native(api.WOO_V3_WS_HEARTBEAT)}));
