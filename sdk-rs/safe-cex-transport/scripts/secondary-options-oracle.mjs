import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const source=new URL('../../../packages/safe-cex/src/createApiWithSecondaryKey.ts',import.meta.url);
const code=ts.transpileModule(fs.readFileSync(source,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
let proxy;let events;
const module={exports:{}};
const require=name=>{assert.equal(name,'./transport/registry');return {getTransport:()=>({publicCorsProxy(){events.push('proxy');return proxy;}})};};
vm.runInThisContext('(function(require,module,exports){'+code+'\n})')(require,module,module.exports);
const cases=[];
for(const secondary_key of [undefined,'','secondary'])for(const secondary_secret of [undefined,'','secondary-secret'])for(const secondary_passphrase of [undefined,'','secondary-pass'])for(const secondaryProxyUrl of [undefined,'','secondary://proxy'])for(const proxyUrl of [undefined,'','argument://proxy'])for(const publicProxy of [undefined,'','public://proxy']){
 const options=JSON.parse(JSON.stringify({key:'primary',secret:'primary-secret',passphrase:'primary-pass',testnet:true,overrideProxyUrl:'old://proxy',secondary_key,secondary_secret,secondary_passphrase,secondaryProxyUrl}));
 const created=[];events=[];proxy=publicProxy;
 const [dynamic,primary,secondary]=module.exports.createApiWithSecondaryKey(options,{},opts=>{created.push(opts);events.push('create');return {get(){},options:opts};},{proxyUrl,rateLimit:client=>{events.push('primary-limit');return client;},secondaryRateLimit:client=>{events.push('secondary-limit');return client;}});
 assert.equal(primary===secondary,!secondary_key);
 assert.equal(created.length,secondary_key?2:1);
 cases.push({options,proxyUrl,publicProxy,derived:created[1]??null,shares:primary===secondary,events});
}
// Programmatic own-undefined properties are outside the typed JSON input ABI.
// Pin the source distinction separately instead of erasing it in fixture encoding.
events=[];proxy=undefined;const constructed=[];
module.exports.createApiWithSecondaryKey({key:'primary',secret:'primary-secret',passphrase:'primary-pass',secondary_key:'secondary',secondary_secret:undefined,secondary_passphrase:undefined},{},options=>{constructed.push(options);return {get(){}};});
assert.ok(Object.hasOwn(constructed[1],'secret') && constructed[1].secret===undefined,
  'TS own-undefined secondary secret must erase primary secret');
assert.ok(Object.hasOwn(constructed[1],'passphrase') && constructed[1].passphrase===undefined,
  'TS own-undefined secondary passphrase must erase primary passphrase');
// Observe native fields before JSON fixture encoding can erase their identity.
for (const signerType of [NaN, -0, Infinity, -Infinity, 1.5]) {
 events=[];proxy=undefined;const actual=[];
 const polymarket={apiKey:'native-credential',signerType};
 const original={key:'primary',secret:'primary-secret',secondary_key:'secondary',polymarket};
 module.exports.createApiWithSecondaryKey(original,{},options=>{actual.push(options);return {get(){}};});
 assert.equal(actual.length,2);
 assert.equal(actual[0],original);
 assert.equal(actual[1].polymarket,polymarket,'source preserves nested options identity');
 assert.ok(Object.is(actual[1].polymarket.signerType,signerType),'source preserves native signerType');
 assert.equal(original.key,'primary');
}
assert.equal(cases.length,729);
process.stdout.write(JSON.stringify(cases));
