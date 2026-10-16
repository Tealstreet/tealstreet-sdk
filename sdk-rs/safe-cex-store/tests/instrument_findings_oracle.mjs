import assert from 'node:assert/strict';
import {loadSource} from './source_loader.mjs';
const {bybitInstrumentClass}=loadSource(new URL('../../../packages/safe-cex/src/instrument-class/bybit.instrumentClass.ts',import.meta.url));
const keys=['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString'];
assert.equal(keys.length,12,'independent inherited member floor');
const rows=keys.map(key=>{
 const result=bybitInstrumentClass({info:{symbolType:key}});
 const repeated=bybitInstrumentClass({info:{symbolType:key}});
 const member=key==='__proto__'?Object.prototype:Object.prototype[key];
 assert.equal(result.kind,'classified');assert.equal(result.assetClass,member,'actual source native member identity BEFORE serialization');
 assert.equal(repeated.assetClass,result.assetClass,'native identity stable across calls');
 const kind=typeof result.assetClass;
 assert.equal(kind,key==='__proto__'?'object':'function');
 return {key,kind,arity:result.assetClass.length??null};
});
assert.equal(new Set(keys.map(key=>bybitInstrumentClass({info:{symbolType:key}}).assetClass)).size,12,'source retains twelve distinct referents');
process.stdout.write(JSON.stringify(rows));
