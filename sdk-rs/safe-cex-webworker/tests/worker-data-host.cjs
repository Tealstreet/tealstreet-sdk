const assert=require('node:assert/strict'),path=require('node:path');
assert(process.argv[2],'provide wasm-bindgen Node module');
const wasm=require(path.resolve(process.argv[2])),sent=[],received=[];
wasm.post_worker_data(value=>{sent.push(value);received.push(structuredClone(value));});
assert.equal(sent.length,1);assert.equal(received.length,1);
const native=sent[0].data.result[0],clone=received[0].data.result[0];
assert.equal(received[0].op,'callMethod:done');
for(const v of [native,clone]) {
 assert(Object.hasOwn(v,'undefined')&&v.undefined===undefined,'WORKER-DATA own undefined');
 assert.equal(v.bool,true);assert.equal(v.null,null);assert.equal(v.big,900719925474099312345n);
 assert(Number.isNaN(v.nan),'WORKER-DATA native NaN');assert.equal(v.positive,Infinity);assert.equal(v.negative,-Infinity);assert(Object.is(v.zero,-0),'WORKER-DATA native signed zero');
 assert(Object.hasOwn(v,'__proto__'));assert.equal(v.__proto__,'own proto data');
 assert(Object.hasOwn(v,'\ud800'));assert.equal(v['\ud800'].charCodeAt(0),0xdc00,'WORKER-DATA native UTF16');
 assert.equal(v.self,v,'WORKER-DATA cycle');
}
assert.notEqual(native,clone);assert.equal(sent[0].data.result[1],native);assert.equal(received[0].data.result[1],clone);
assert(received[0].data.map instanceof Map);assert(Number.isNaN(received[0].data.map.get(clone)));
assert(received[0].data.set instanceof Set);assert(received[0].data.set.has(clone));
assert.throws(()=>wasm.post_worker_data(()=>{throw new TypeError('post rejected');}),/post rejected/);
console.log('WORKER-DATA actual WASM materialization + native structuredClone: numeric bits, undefined, UTF16, proto property, aliases/cycles, Map/Set and host failure PASS');
let event, eventClone;
wasm.post_worker_error_data(value => { event=value; eventClone=structuredClone(value); });
const nativeError=event.data[1].sentryData.error, clonedError=eventClone.data[1].sentryData.error;
assert(nativeError instanceof Error, 'WORKER-EVENT native Error identity before post');
assert.equal(nativeError.name,'AxiosError'); assert.equal(nativeError.message,'retained event error');
assert.equal(nativeError.code,Infinity); assert.equal(nativeError.stack,'original event stack\n    at source-event');
assert(clonedError instanceof Error, 'WORKER-EVENT Error survives structured clone');
assert.equal(clonedError.name,'Error'); assert.equal(clonedError.message,nativeError.message);
assert.equal(clonedError.stack,nativeError.stack); assert(!Object.hasOwn(clonedError,'code'));
const typedError=event.data[1].sentryData.typedError, typedClone=eventClone.data[1].sentryData.typedError;
const sourceTyped=new TypeError('\ud800typed event'); sourceTyped.stack='original typed stack';
const sourceClone=structuredClone(sourceTyped);
assert(typedError instanceof TypeError, 'WORKER-EVENT standard Error prototype before post');
assert.equal(Object.hasOwn(typedError,'name'),Object.hasOwn(sourceTyped,'name'));
assert.equal(Object.getOwnPropertyDescriptor(typedError,'message').enumerable,
 Object.getOwnPropertyDescriptor(sourceTyped,'message').enumerable);
for(const [actual,expected] of [[typedError,sourceTyped],[typedClone,sourceClone]]) {
 assert(actual instanceof TypeError, 'WORKER-EVENT TypeError survives structured clone');
 assert.equal(actual.name,expected.name); assert.equal(actual.message,expected.message);
 assert.equal(actual.stack,expected.stack);
}
console.log('WORKER-EVENT actual WASM Error identity/stack and native structuredClone PASS');
