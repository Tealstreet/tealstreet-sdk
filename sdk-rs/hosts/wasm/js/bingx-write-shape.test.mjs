import test from 'node:test';
import assert from 'node:assert/strict';
import * as host from './host.mjs';
const record=()=>({endpointFamily:'order',method:'DELETE',parameterNames:['orderId','signature','symbol','timestamp'],hasOtherParameters:false,idClass:'numeric',idDigits:19,targetKind:'takeProfit',symbolFormat:'hyphenated',replacement:'unobserved'});

test('native BingX invalidInput detail retains a detached closed shape',()=>{
 const source=record();const error=new host.ExchangeError('fixture refusal',{kind:'invalidInput',venueCode:'109400',bingxWriteShape:source});
 assert.deepEqual(error.bingxWriteShape,source);assert.notEqual(error.bingxWriteShape,source);assert.notEqual(error.bingxWriteShape.parameterNames,source.parameterNames);
 source.parameterNames[0]='secret-value';assert.equal(error.bingxWriteShape.parameterNames[0],'orderId');
});
test('success and errors without this record have no extra shape',()=>{assert.equal(new host.ExchangeError('fixture').bingxWriteShape,undefined);});
test('shape validator drops unknown properties without reading their getters',()=>{
 const source=record();Object.defineProperty(source,'apiKey',{get(){throw Error('must not read secret')}});source.url='https://private.example';
 assert.deepEqual(host.bingxWriteShapeOf?.(source),record());
});
test('invalid enum, parameter value, numeric bounds and hostile accessors fail closed',()=>{
 const cases=[{...record(),endpointFamily:'https://private.example'},{...record(),parameterNames:['orderId=secret']},{...record(),idClass:'secret'}, {...record(),idDigits:65},{...record(),replacement:true},{...record(),targetKind:'raw-secret'}, {...record(),method:'GET'}, {...record(),symbolFormat:'BTC-USDT'}, {...record(),idClass:'other',idDigits:19}];
 for(const value of cases)assert.equal(host.bingxWriteShapeOf?.(value),undefined);
 const throwing=record();Object.defineProperty(throwing,'idClass',{get(){throw Error('unreadable')}});assert.equal(host.bingxWriteShapeOf?.(throwing),undefined);
});

test('nested indexed outcomes preserve the same validated record',()=>{
 const rows=host.writePlanElementsOf([{index:0,status:'notSent',error:{message:'fixture refusal',kind:'invalidInput',venueCode:'109400',bingxWriteShape:{...record(),orderId:'private-order',signature:'private-signature'}}}]);
 assert.deepEqual(rows[0].error.bingxWriteShape,record());
 assert.ok(!JSON.stringify(rows).includes('private-'));
});
