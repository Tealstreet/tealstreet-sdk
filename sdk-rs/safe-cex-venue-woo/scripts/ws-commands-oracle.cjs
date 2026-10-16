'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,extras,imports={}) {
 const source=fs.readFileSync(path.join(root,file),'utf8');
 const exports={}; const code=ts.transpileModule(source+(extras.length?'\nexport { '+extras.join(',')+' };':''),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(code,{exports,require(name){assert.ok(Object.hasOwn(imports,name),'actual source import '+name);return imports[name];}});return exports;
}
const contracts=load('woo.v3.contracts.ts',[]);
const api=load('woo.v3.websocket.ts',['isRecord','requireRecord','requireArray','requireEnum','requireTopicList','assertTopicsMatchChannel','buildCommand'],{'./woo.v3.contracts':contracts});
function capture(fn,...args){try {const value=fn(...args);return {ok:value===undefined?{$undefined:true}:value};}catch(error){return {error:{name:error.name,message:error.message}};}}
const commandNames=['SUBSCRIBE','UN_SUBSCRIBE','UN_SUBSCRIBE_ALL','LIST_SUBSCRIPTION'];
const bbo='bbo@PERP_BTC_USDT', book='orderbookupdate@PERP_BTC_USDT@050';
const specs=[];
for(const channel of ['public','private'])for(const [index,cmd] of commandNames.entries()) {
 const topics=channel==='public'?[bbo,book]:['account','balance'];
 specs.push({operation:['subscribe','unsubscribe','all','list'][index],channel,cmd,id:'id-'+channel+'-'+index,...(index<2?{topics}:{})});
 specs.push({operation:'generic',channel,cmd,id:'generic-'+channel+'-'+index});
}
const extras=[
 {label:'twenty',topics:Array.from({length:20},(_,i)=>'bbo@PERP_BTC'+i+'_USDT')},
 {label:'twenty-one',topics:Array.from({length:21},(_,i)=>'bbo@PERP_BTC'+i+'_USDT')},
 {label:'empty-topics',topics:[]},{label:'duplicate',topics:[bbo,bbo]},
 {label:'unknown',topics:['bad']},{label:'wrong-public',topics:['account']},
 {label:'wrong-private',channel:'private',topics:[bbo]},{label:'mixed',topics:[bbo,'account']},
 {label:'original-depth-text',topics:[book,'orderbookupdate@PERP_BTC_USDT@0x32']},
 {label:'invalid-duplicate',topics:['bad','bad']},{label:'duplicate-invalid',topics:[bbo,bbo,'bad']},
 {label:'id-before-count',id:'',topics:Array.from({length:21},()=>bbo)},
 {label:'id-before-channel',id:null,topics:['account']},{label:'number-id',id:3,topics:[bbo]},
 {label:'missing-id',id:undefined,operation:'generic',topics:undefined},
 {label:'unicode-id',id:'é𐐀',topics:[bbo]},{label:'space-id',channel:'private',id:' ',topics:['account']},
 {label:'generic-list-params',operation:'generic',channel:'private',cmd:'LIST_SUBSCRIPTION',topics:['position']},
];
for(const extra of extras)specs.push({operation:'subscribe',channel:'public',cmd:'SUBSCRIBE',id:'edge-id',...extra});
const dispatch={subscribe:api.buildWooV3Subscribe,unsubscribe:api.buildWooV3Unsubscribe,all:api.buildWooV3UnsubscribeAll,list:api.buildWooV3ListSubscriptions};
const commands=specs.map(spec=>({ ...spec,...(spec.operation==='generic'?capture(api.buildCommand,spec.channel,spec.id,spec.cmd,spec.topics):capture(dispatch[spec.operation],spec.channel,spec.id,spec.topics)) }));
for(const row of commands)if(row.ok){assert.ok(Object.isFrozen(row.ok));assert.ok(Object.isFrozen(row.ok.wire));if(row.ok.wire.params)assert.ok(Object.isFrozen(row.ok.wire.params));}
assert.equal(commands.length,34,'WS-COMMAND independent operation floor');
const helperInputs=[undefined,null,false,3,'',[],{}, {marker:'kept'},[bbo,'account']];
const helpers=helperInputs.map(input=>({input,record:api.isRecord(input),object:capture(api.requireRecord,input,'test.object'),array:capture(api.requireArray,input,'test.array')}));
for(const input of helperInputs){if(api.isRecord(input))assert.equal(api.requireRecord(input,'x'),input,'source object alias');if(Array.isArray(input))assert.equal(api.requireArray(input,'x'),input,'source array alias');}
const enums=[...['public','private','unknown',null,3,'',undefined].map(input=>({input,values:['public','private']})),{input:'public',values:[]},{input:'x',values:['x','x']}].map(spec=>({...spec,...capture(api.requireEnum,spec.input,spec.values,'test.enum')}));
const lists=[undefined,null,{},[],[bbo,'account'],[bbo,bbo],[bbo,bbo,3],[''],[3],[['account']]].map(input=>({input,...capture(api.requireTopicList,input,'test.topics')}));
for(const row of lists)if(row.ok)assert.ok(Object.isFrozen(row.ok));
const channels=[{topics:[],channel:'public'},{topics:[bbo,book],channel:'public'},{topics:['account','balance'],channel:'private'},{topics:['account'],channel:'public'},{topics:[bbo],channel:'private'},{topics:[bbo,'account'],channel:'public'},{topics:['bad','account'],channel:'public'}].map(spec=>({...spec,...capture(api.assertTopicsMatchChannel,spec.topics,spec.channel,'test.channel')}));
assert.equal(helpers.length,9);assert.equal(enums.length,9);assert.equal(lists.length,10);assert.equal(channels.length,7);
const mutableTopics=[bbo];const sourceSnapshot=api.buildWooV3Subscribe('public','snapshot',mutableTopics);mutableTopics[0]='ticker@PERP_ETH_USDT';mutableTopics.push('account');
assert.deepEqual([...sourceSnapshot.wire.params],[bbo],'source command snapshots params');
const wireRecords=[{id:'wire-distinct',cmd:'UN_SUBSCRIBE',params:['account','balance']},{id:'no-params',cmd:'LIST_SUBSCRIPTION'},{id:'empty-params',cmd:'SUBSCRIBE',params:[]}];
const commandRecords=[{channel:'private',wire:wireRecords[0]},{channel:'public',wire:wireRecords[1]}];
const ast=ts.createSourceFile('woo.v3.websocket.ts',fs.readFileSync(path.join(root,'woo.v3.websocket.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const declaration=ast.statements.filter(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='WooV3WsCommandName');assert.equal(declaration.length,1);
assert.deepEqual(declaration[0].type.types.map(n=>n.literal.text),commandNames,'actual source command literal union');
console.log(JSON.stringify({commandNames,commands,helpers,enums,lists,channels,wireRecords,commandRecords,sourceSnapshot}));
