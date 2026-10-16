import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts, root, load} from './source.mjs';

const filename=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const ast=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcExchange');
assert(klass,'MEXC declaration');
const method=name=>{
 const member=klass.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)===name);
 assert(member?.initializer&&ts.isArrowFunction(member.initializer),name);
 return `exports.${name}=function(${member.initializer.parameters.map(n=>n.getText(ast)).join(',')}) ${member.initializer.body.getText(ast)};`;
};
const declarations=['extractData','extractRows','MEXC_REGULAR_ORDER_THROTTLE_COOLDOWN_MS'];
const helpers=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>declarations.includes(d.name.getText(ast))));
assert.equal(helpers.length,3,'three actual helper declarations');
const code=ts.transpileModule([...helpers.map(n=>n.getText(ast)),method('noteRegularOrderSubmitError'),method('assertRegularOrderSubmitSucceeded'),method('assertRegularOrderBatchSucceeded')].join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
let clockCalls=0;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${code}\n})(module.exports)`,{module,toNumber:utils.toNumber,toIntegerString:utils.toIntegerString,virtualClock:{getCurrentTimeMs:()=>{clockCalls++;return 1000;}}});
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function capture(task,payload){
 try{assert.equal(task(),undefined,'source void result');return {success:true};}
 catch(error){
  if(error.response){
   assert.equal(error.response.data,payload,'actual error response payload identity');
   error.response.data.fromError=17;
   assert.equal(payload.fromError,17,'source error to input alias');
   payload.fromInput=23;
   assert.equal(error.response.data.fromInput,23,'source input to error alias');
  }
  return {success:false,name:error.name,message:error.message,responseIdentity:!!error.response,
   partialIds:error.partialOrderIds??null,partialCount:error.partialOrderCount??null,
   totalBits:error.totalOrderCount===undefined?null:bits(error.totalOrderCount),failedIndex:error.failedOrderIndex??null};
 }
}
const malformed={toString:0,valueOf:0};
const singleInputs=[
 undefined,null,{}, {success:true}, {success:0}, {success:''},
 {success:false,message:'business-A',msg:'business-B',code:1}, {success:false,message:'',msg:'fallback-B',code:2},
 {success:false,code:510}, {success:false,message:'REQUESTS ARE TOO FREQUENT'},
 {success:false,message:false,msg:'Requests are too frequent'},
 {success:false,message:{},code:510}, {success:false,message:malformed,code:510},
 {success:false,code:malformed}, {success:false}, {success:false,message:17},
];
assert.equal(singleInputs.length,16,'independent single fixture floor');
const singles=singleInputs.map((input,index)=>{
 const original=structuredClone(input);
 const instance={regularOrderSubmitCooldownUntil:0,noteRegularOrderSubmitError:module.exports.noteRegularOrderSubmitError};
 clockCalls=0;const outcome=capture(()=>module.exports.assertRegularOrderSubmitSucceeded.call(instance,input),input);
 const expectedThrottle=[8,9,11].includes(index);
 assert.equal(clockCalls,expectedThrottle?1:0,'independent single clock floor');
 assert.equal(instance.regularOrderSubmitCooldownUntil,expectedThrottle?2500:0,'independent single cooldown floor');
 return {index,...(input===undefined?{}:{input:original}),outcome,cooldown:bits(instance.regularOrderSubmitCooldownUntil),clockCalls};
});
// Independent expected partial cardinalities; data is not used to derive them.
const batchInputs=[
 [undefined,0], [null,0], [{},0], [{success:false,message:'outer rejection',code:511},0],
 [{success:false,code:7},0], [{success:false,message:malformed},0],
 [[],0], [[{orderId:'all-ok',errorCode:0}],0],
 [[{orderId:'A'},{errorCode:9,errorMsg:'first failure'},{id:'B'},{errorCode:8,message:'second failure'}],2],
 [{data:[{errorCode:8},{orderId:0,errorCode:'0'},{orderId:'',id:'masked'},{orderId:null,id:'fallback'}]},2],
 [[{errorCode:7},{errorCode:8,msg:'more'}],0],
 [[{orderId:'success',errorCode:0},{errorCode:9,errorMsg:malformed},null],0],
 [[{orderId:malformed},{errorCode:8,errorMsg:'venue refusal'}],0],
 [[{orderId:'success'},{errorCode:8,errorMsg:malformed}],0],
 [{data:{items:[{errorCode:8}]}},0],
 [[{orderId:'D'},{orderId:'D'},{errorCode:'9 trailing',errorMsg:0,msg:'fallback message'}],2],
 [[{errorCode:8,errorMsg:'only failure'},{orderId:''}],0],
 [[{orderId:{},errorCode:null},{errorCode:7,message:false,msg:'fallback'}],1],
 [[{errorCode:malformed},{orderId:'later success'}],0],
 [[0,false,'primitive-row',{id:'final-success'},{errorCode:7}],1],
];
assert.equal(batchInputs.length,20,'independent batch fixture floor');
const totals=[3.75,-0,Number.NaN,Number.POSITIVE_INFINITY];
const batches=batchInputs.map(([input,expectedPartials],index)=>{
 const original=structuredClone(input);
 const total=totals[index%4];const outcome=capture(()=>module.exports.assertRegularOrderBatchSucceeded(input,total),input);
 assert.equal(outcome.partialIds?.length??0,expectedPartials,'independent source partial cardinality');
 if(expectedPartials){assert.equal(outcome.partialCount,expectedPartials);assert.equal(outcome.totalBits,bits(total));}
 return {index,...(input===undefined?{}:{input:original}),expectedPartials,totalBits:bits(total),outcome};
});
assert.equal(singles[15].index,15);assert.equal(batches[19].index,19,'last validation fixture');
process.stdout.write(JSON.stringify({singles,batches}));
