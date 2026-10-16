const assert=require('node:assert/strict');
const {write,ts,writeCode}=require('./load-starbase.cjs');
const {preserveSpecialNumbers}=require('./special-numbers.cjs');
const {captureHeader,numberBits}=require('./special-numbers.cjs');
const source=ts.createSourceFile('orders.ts',writeCode);
const fn=source.statements.filter(ts.isVariableStatement).flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)==='buildCoinbaseStarbaseEdit').initializer;
assert(ts.isTypeReferenceNode(fn.type));
const returnContract={name:fn.type.typeName.getText(source),args:fn.type.typeArguments.map(t=>t.getText(source))};
const cases=[];
function add(values={}){
 // Input order mirrors the declared intent record; blocked gates return it as-is.
 const intent={orderId:values.orderId??'order-1',price:values.price,postOnly:values.postOnly,reduceOnly:values.reduceOnly,rejectPostOnly:values.rejectPostOnly,size:values.size,triggerOffset:values.triggerOffset,triggerPrice:values.triggerPrice,validUntil:values.validUntil};
 const bits={};for(const key of ['price','triggerOffset','triggerPrice'])if(intent[key]!==undefined)bits[key]=numberBits(intent[key]);
 let result;try{const gate=write.buildCoinbaseStarbaseEdit(intent);if(gate.status==='blocked')assert.strictEqual(gate.intent,intent);result=preserveSpecialNumbers({ok:gate});}catch(error){result={error:{name:error.name,message:error.message}};}
 cases.push({...captureHeader(),intent,bits,result});
}
const base=write.coinbaseStarbaseTotalBaseAmount(1),contracts=write.coinbaseStarbaseTotalContracts(1),valid=write.coinbaseStarbaseUnresolvedValidUntil(19);
const fields={price:17,postOnly:true,reduceOnly:false,rejectPostOnly:false,size:base,triggerOffset:13,triggerPrice:11,validUntil:valid};
for(let mask=0;mask<256;mask++){const input={};Object.keys(fields).forEach((key,i)=>{if(mask&(1<<i))input[key]=fields[key];});add(input);}
const numbers=[undefined,-0,0,-1,1,0.5,Number.MIN_VALUE,Number.MAX_VALUE,NaN,Infinity,-Infinity];
for(const price of numbers)for(const triggerOffset of numbers)for(const triggerPrice of numbers)add({price,triggerOffset,triggerPrice});
for(const orderId of ['', 'order-1',' ','\uFEFF','\u0085','💰'])for(const size of [undefined,base,contracts])for(const validUntil of [undefined,valid])for(const postOnly of [undefined,false,true])for(const rejectPostOnly of [undefined,false,true])for(const price of [undefined,-1,NaN,1])add({orderId,size,validUntil,postOnly,rejectPostOnly,price});
for(const [reduceOnly,rejectPostOnly] of [[false,true],[true,false]])add({postOnly:true,reduceOnly,rejectPostOnly});
for(const key of ['postOnly','reduceOnly','rejectPostOnly'])add({[key]:false});
process.stdout.write(JSON.stringify({cases,returnContract}));
