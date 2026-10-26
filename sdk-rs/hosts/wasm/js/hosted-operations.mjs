/** One structured-operation channel. Crypto policy belongs to the native family codec. */
const LIMIT = 1_048_576;
const CAPACITY = 64;
const encoder = new TextEncoder();
const refused = () => new Error('Hosted operation refused');
const record = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const own = (v, k) => Object.prototype.hasOwnProperty.call(v, k);
const exact = (v, keys) => record(v) && Object.keys(v).length === keys.length && keys.every(k => own(v, k));
const id = v => typeof v === 'string' && v.length > 0 && encoder.encode(v).length <= 256;
const integer = v => Number.isSafeInteger(v) && v >= 0;
const time = v => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= Number.MAX_SAFE_INTEGER;
const equal = (a, b) => {
  if (a === b) return true;
  if (Array.isArray(a) || Array.isArray(b)) return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v,i) => equal(v,b[i]));
  return record(a) && record(b) && Object.keys(a).length === Object.keys(b).length && Object.keys(a).every(k => own(b,k) && equal(a[k],b[k]));
};
function freeze(value) {
  if (record(value) || Array.isArray(value)) { for (const item of Object.values(value)) freeze(item); Object.freeze(value); }
  return value;
}
function encode(value) {
  const text = JSON.stringify(value);
  if (encoder.encode(text).length > LIMIT) throw refused();
  return text;
}
function decode(text) {
  if (typeof text !== 'string' || text.length > LIMIT || encoder.encode(text).length > LIMIT) throw refused();
  const value = JSON.parse(text);
  // JSON.parse discards duplicate keys. Check the original token stream as well.
  const stack = [];
  for (const [token] of text.matchAll(/"(?:\\.|[^"\\])*"|[{}\[\],:]|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null/g)) {
    const top = stack.at(-1);
    if (token === '{') stack.push({keys:new Set(), key:true});
    else if (token === '[') stack.push(null);
    else if (token === '}' || token === ']') stack.pop();
    else if (token === ',' && top) top.key = true;
    else if (token === ':' && top) top.key = false;
    else if (token[0] === '"' && top?.key) {
      const key = JSON.parse(token);
      if (top.keys.has(key)) throw refused();
      top.keys.add(key); top.key = false;
    }
  }
  return value;
}
function snapshot(value) { return freeze(decode(encode(value))); }
function receipt(value) {
  if (!record(value) || Object.keys(value).some(k => !['stampMs','expiryMs','nonce'].includes(k))) throw refused();
  for (const k of ['stampMs','expiryMs']) if (own(value,k) && !integer(value[k])) throw refused();
  if (own(value,'nonce') && (typeof value.nonce !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value.nonce)
    || BigInt(value.nonce) > 18446744073709551615n)) throw refused();
  if (own(value,'stampMs') && own(value,'expiryMs') && value.expiryMs <= value.stampMs) throw refused();
}
const scopes = new Set(['account','balance','orders','positions','settings']);
function role(value, slot) {
  if (!['primary','secondary'].includes(slot) || !record(value)) throw refused();
  if (value.kind === 'read' || value.kind === 'primaryRead') {
    const keys = value.kind === 'read' ? ['kind','scope'] : ['kind','scope','reason'];
    if (!exact(value,keys) || value.scope !== null && !scopes.has(value.scope)) throw refused();
    if (value.kind === 'primaryRead' && (!['clientOrderLookup','postUnknownOrderRead','degradedRead','catalog','history'].includes(value.reason) || slot !== 'primary')) throw refused();
  } else if (!['mutation','sessionAuth'].includes(value.kind) || !exact(value,['kind']) || slot !== 'primary') throw refused();
}
function meta(value) {
  if (!exact(value,['task','owner','deadlineMs','slot','role']) || !integer(value.task) || !time(value.deadlineMs)) throw refused();
  role(value.role,value.slot);
  const owner = value.owner;
  if (!record(owner) || !integer(owner.generation) || (owner.kind === 'managed'
    ? !exact(owner,['kind','request','generation']) || !integer(owner.request)
    : owner.kind !== 'session' || !exact(owner,['kind','generation']))) throw refused();
  if ((owner.kind === 'session') !== (value.role.kind === 'sessionAuth')) throw refused();
}
function binding(value) {
  if (!exact(value,['account','network','stableSubject','expectedCredential','generation','metadata'])
    || !['account','network','stableSubject','generation'].every(k => id(value[k]))
    || typeof value.expectedCredential !== 'string' || !value.expectedCredential || encoder.encode(value.expectedCredential).length > 8192
    || value.metadata !== null && !record(value.metadata)) throw refused();
}
function synchronous(value) { if (value && typeof value.then === 'function') throw refused(); return value; }
function result(value, expected) {
  if (!exact(value,['values','receipt']) || !equal(value.receipt,expected) || !Array.isArray(value.values)
    || value.values.length > 32 || value.values.some(v => !Array.isArray(v) || v.length > LIMIT || v.some(b => !Number.isInteger(b) || b < 0 || b > 255))) throw refused();
  receipt(value.receipt);
  encode(value);
}
function config(options) {
  const bindings = snapshot(options.bindings);
  if (!record(bindings) || Object.keys(bindings).length === 0 || Object.keys(bindings).some(k => !['primary','secondary'].includes(k))) throw refused();
  for (const value of Object.values(bindings)) binding(value);
  if (!id(options.exchangeName) || typeof options.accountName !== 'string' || encoder.encode(options.accountName).length > 4096
    || typeof options.codec?.recipe !== 'function' || !Array.isArray(options.codec.recipes)
    || options.codec.recipes.length === 0 || options.codec.recipes.length > 32 || !options.codec.recipes.every(id)
    || new Set(options.codec.recipes).size !== options.codec.recipes.length) throw refused();
  return bindings;
}
function alive(now, deadline, signal) {
  if (!time(now) || !time(deadline) || now >= deadline || signal?.aborted) throw refused();
}
function request(value) {
  if (!exact(value,['attemptId','deadlineMs','receipt','slot','role','binding','descriptor']) || !integer(value.attemptId)
    || !time(value.deadlineMs) || !record(value.descriptor)) throw refused();
  receipt(value.receipt); role(value.role,value.slot); binding(value.binding);
}
function codecCheck(codec, req, now, side, original) {
  alive(now,req.deadlineMs);
  synchronous(codec.validate(req,{now,deadlineMs:req.deadlineMs,side,work:original}));
}

/** Capabilities and bindings are confirmed by the authenticated host before construction. */
export function createOperationProvider(options) {
  const bindings = config(options), capabilities = snapshot(options.capabilities);
  if (!exact(capabilities,['v','recipes','slots']) || capabilities.v !== 2 || !Array.isArray(capabilities.recipes)
    || !Array.isArray(capabilities.slots) || !capabilities.slots.every(s => ['primary','secondary'].includes(s))
    || !capabilities.recipes.every(id)) throw refused();
  const {codec,now,send} = options;
  let serial = 0;
  const pending = new Set();
  return async (work,context) => {
    let correlation,registered=false;
    try {
      const m = snapshot(context?.meta); meta(m); receipt(work.receipt);
      if (!context.signal || context.signal.aborted || !capabilities.slots.includes(m.slot) || pending.size >= CAPACITY) throw refused();
      const b = bindings[m.slot]; if (!b) throw refused();
      const start = now(); alive(start,m.deadlineMs,context.signal);
      const deadlineMs = Math.min(m.deadlineMs,start+5000);
      const descriptor = snapshot(synchronous(codec.describe(work,{meta:m,binding:b,deadlineMs})));
      const recipe = synchronous(codec.recipe(descriptor));
      if (!codec.recipes.includes(recipe) || !capabilities.recipes.includes(recipe)) throw refused();
      const req = snapshot({attemptId:m.task,deadlineMs,receipt:work.receipt,slot:m.slot,role:m.role,binding:b,descriptor});
      request(req); codecCheck(codec,req,now(),'origin',work);
      correlation = options.id ? options.id() : `operation-${++serial}`;
      if (!id(correlation) || pending.has(correlation)) throw refused();
      pending.add(correlation);registered=true;
      const frame = encode({id:correlation,type:'op',v:2,exchangeName:options.exchangeName,accountName:options.accountName,request:req});
      const response = decode(await send(frame,{signal:context.signal}));
      alive(now(),deadlineMs,context.signal);
      if (!exact(response,['id','type','v','attemptId','result']) || response.id !== correlation || response.type !== 'op'
        || response.v !== 2 || response.attemptId !== m.task) throw refused();
      codecCheck(codec,req,now(),'origin',work);
      result(response.result,req.receipt);
      synchronous(codec.verify(response.result,{...req,work,now:now(),side:'origin'}));
      alive(now(),deadlineMs,context.signal);
      return response.result;
    } catch { throw refused(); }
    finally { if (registered) pending.delete(correlation); }
  };
}

/** One instance per authenticated account incarnation. Disposal never grants joined-work evidence. */
export function createOperationPeer(options) {
  const bindings = config(options), {codec,now,revision} = options;
  const decisions = new Map(), ids = new Map(), active = new Set();
  let disposed = false;
  function cleanup(at) {
    for (const [key,value] of decisions) if (at >= value.deadline && !value.pending) {decisions.delete(key);ids.delete(value.id);}
  }
  return {
    capabilities: freeze({v:2,recipes:[...codec.recipes],slots:Object.keys(bindings)}),
    async handle(frame,{signal}={}) {
      let d, controller, entry, executing=false;
      try {
        if (disposed) throw refused();
        d = decode(frame);
        if (!exact(d,['id','type','v','exchangeName','accountName','request']) || !id(d.id) || d.type !== 'op' || d.v !== 2
          || d.exchangeName !== options.exchangeName || d.accountName !== options.accountName) throw refused();
        const req = snapshot(d.request); request(req);
        const at = now(); alive(at,req.deadlineMs,signal); cleanup(at);
        const b = bindings[req.slot];
        if (req.deadlineMs-at > 5000 || !b || !equal(req.binding,b)) throw refused();
        const recipe = synchronous(codec.recipe(req.descriptor));
        if (!codec.recipes.includes(recipe)) throw refused();
        const key = encode([req.attemptId,['stampMs','expiryMs','nonce'].map(k => own(req.receipt,k) ? [k,req.receipt[k]] : [k])]);
        if (decisions.has(key) || ids.has(d.id) || decisions.size >= CAPACITY || active.size >= CAPACITY) throw refused();
        codecCheck(codec,req,now(),'peer');
        const epoch = revision();
        if (!id(epoch)) throw refused();
        controller = new AbortController();
        const cancel = () => controller.abort();
        if (signal) signal.addEventListener('abort',cancel,{once:true});
        entry = {deadline:req.deadlineMs,id:d.id,pending:true};
        decisions.set(key,entry);ids.set(d.id,key);active.add(controller);
        try {
          // Read the peer clock at signing, after all admission work.
          codecCheck(codec,req,now(),'peer');
          if (disposed || signal?.aborted || revision() !== epoch) throw refused();
          const check = () => {
            alive(now(),req.deadlineMs,controller.signal);
            if (disposed || revision() !== epoch) throw refused();
            return now();
          };
          executing=true;
          const value = await codec.execute(req,{signal:controller.signal,now,check,deadlineMs:req.deadlineMs});
          alive(now(),req.deadlineMs,controller.signal);
          if (disposed || revision() !== epoch) throw refused();
          codecCheck(codec,req,now(),'peer');
          result(value,req.receipt);
          synchronous(codec.verify(value,{...req,now:now(),side:'peer'}));
          alive(now(),req.deadlineMs,controller.signal);
          return encode({id:d.id,type:'op',v:2,attemptId:req.attemptId,result:value});
        } finally { if (signal) signal.removeEventListener('abort',cancel); }
      } catch {
        if (disposed || signal?.aborted || !id(d?.id) || !integer(d?.request?.attemptId)) throw refused();
        return encode({id:d.id,type:'op',v:2,attemptId:d.request.attemptId,error:executing?'refused':'invalidRequest'});
      }
      finally {
        if (entry) entry.pending = false;
        if (controller) active.delete(controller);
      }
    },
    dispose() { disposed=true;for (const controller of active) controller.abort(); },
  };
}
