/** Kraken codec for the shared channel; all preimages and crypto stay native. */
const refused = () => new Error('Hosted operation refused');
const copy = value => JSON.parse(JSON.stringify(value));
const exact = (v,keys) => v && typeof v === 'object' && !Array.isArray(v) && Object.keys(v).length === keys.length && keys.every(k => Object.prototype.hasOwnProperty.call(v,k));
function closedWork(work) {
  if (!exact(work,['receipt','operations','context']) || !exact(work.context,['venue','method','host','path']) || !Array.isArray(work.operations) || work.operations.length !== 3) throw refused();
  const [present,hash,sign] = work.operations;
  if (!exact(present,['kind','key']) || present.kind !== 'present' || !exact(hash,['kind','payload']) || hash.kind !== 'sha256'
    || !exact(sign,['kind','algorithm','key','payload']) || sign.kind !== 'sign' || !Array.isArray(hash.payload) || !Array.isArray(sign.payload)) throw refused();
  for (const part of [...hash.payload,...sign.payload]) {
    if (part.kind === 'bytes' ? !exact(part,['kind','bytes']) : part.kind !== 'result' || !exact(part,['kind','index','encoding'])) throw refused();
  }
}
export function createKrakenOperationCodec({wasm,identity,executor,spot=false,testnet=false,slots=['primary']}) {
  const describe = wasm.krakenHostedDescriptor;
  const validate = wasm.krakenHostedValidate;
  const verify = wasm.krakenHostedVerify;
  if (![describe,validate,verify].every(fn => typeof fn === 'function')) throw refused();
  const selected = identity && Object.freeze(copy(identity));
  const binding = executor && executor.binding.bind(executor);
  const sign = executor && executor.sign.bind(executor);
  const installed = Object.fromEntries(executor ? slots.map(slot => [slot,Object.freeze(JSON.parse(binding(slot)))]) : []);
  const family = spot ? 'kraken.spot.http' : 'kraken.futures.http';
  function checkDescriptor(d,b) {
    if (d.kind !== family || d.testnet !== testnet || b.network !== (testnet ? 'testnet' : 'mainnet') || b.metadata !== null) throw refused();
  }
  return Object.freeze({
    recipes:Object.freeze([`${family}/1`]),
    replayUntil(req) {return req.descriptor.stampMs+5000;},
    recipe(descriptor) {return descriptor.kind === family ? `${family}/1` : undefined;},
    describe(work,{binding:b}) {
      closedWork(work);
      if (!selected || work.operations?.[0]?.key !== selected.credential || work.operations?.[2]?.key !== selected.signing) throw refused();
      const d = JSON.parse(describe(JSON.stringify(work)));
      checkDescriptor(d,b);return d;
    },
    validate(req,{now,deadlineMs}) {
      checkDescriptor(req.descriptor,req.binding);
      validate(JSON.stringify(req.descriptor),JSON.stringify(req.receipt),req.slot,JSON.stringify(req.role),now,deadlineMs);
    },
    execute(req,context) {
      if (!binding || !sign) throw refused();
      const selected = installed[req.slot];
      if (!selected || selected.stableSubject !== req.binding.stableSubject || selected.expectedCredential !== req.binding.expectedCredential) throw refused();
      return JSON.parse(sign(JSON.stringify(req.descriptor),JSON.stringify(req.receipt),req.slot,JSON.stringify(req.role),context.check(),req.deadlineMs));
    },
    verify(result,req) {
      verify(JSON.stringify(req.descriptor),JSON.stringify(result),req.binding.expectedCredential);
    },
  });
}
