const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
function compile(file, owner, field, bindings) {
  const filename = path.join(root, 'packages/safe-cex/src/exchanges/bitget', file);
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === owner);
  const member = klass?.members.find(n => n.name?.getText(ast) === field);
  assert(member && ts.isPropertyDeclaration(member), `POLICY actual ${owner}.${field}`);
  // Only the actual field body is under test. Imported callees and host
  // operations are supplied; transpilation is not semantic TS type checking.
  const emitted = ts.transpileModule(`class Source { ${member.getText(ast)} }`, {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  return new Function(...bindings, emitted + ';return Source;');
}
const pings = [
  ['public', compile('bitget.ws-public.ts', 'BitgetPublicWebsocket', 'ping', ['performance']), 'send'],
  ['secondary', compile('bitget.ws-secondary-public.ts', 'BitgetSecondaryPublicWebsocket', 'ping', ['performance']), 'send'],
  ['private', compile('bitget.ws-private.ts', 'BitgetPrivateWebsocket', 'ping', ['performance']), 'sendWithLog'],
];
const promote = compile('bitget.exchange.ts', 'BitgetExchange', 'promoteUtaRoutingFromClassicUnsupported', ['isBitgetUtaClassicUnsupported']);
const tag = number => Object.is(number, -0) ? '-0' : String(number);
const cases = [];
for (const [kind, factory, method] of pings) for (const disposed of [false,true]) for (const fail of [false,true]) for (const now of [-0,103.5]) {
  const events = [], failure = new Error('send failed');
  let receiver;
  const Source = factory({now:()=>{events.push('clock');return now;}});
  receiver = new Source(); receiver.isDisposed = disposed; receiver.pingAt = 37.5;
  receiver[method] = text => {
    assert.equal(text,'ping','POLICY source ping text');
    assert(Object.is(receiver.pingAt,now),'POLICY source timestamp before send');
    events.push(method);
    if(fail) throw failure;
  };
  let error = false;
  try { assert.equal(receiver.ping(),undefined,'POLICY source ping return'); }
  catch(thrown) { assert.strictEqual(thrown,failure,'POLICY source send error identity'); error=true; }
  const expectedEvents = disposed ? [] : ['clock',method];
  assert.deepEqual(events,expectedEvents,'POLICY source ping effects');
  assert(Object.is(receiver.pingAt,disposed?37.5:now),'POLICY source final timestamp');
  assert.equal(error,!disposed&&fail,'POLICY source ping failure disposition');
  cases.push({input:{kind,disposed,fail,now:tag(now)},output:{at:tag(receiver.pingAt),events,error}});
}
function capturePromotion(input) {
  const events=[], failure=new Error('route read failed');
  const payload=input.present?{code:'40085',marker:59}:undefined;
  const Source=promote(received=>{
    events.push('predicate');
    assert.strictEqual(received,payload,'POLICY source original payload');
    if(input.failure==='predicate') throw failure;
    return input.unsupported;
  });
  const receiver=new Source();
  receiver.isUta=input.initialUta; receiver._accountTypeDetected=input.initialDetected;
  Object.defineProperty(receiver,'supportsUtaRouting',{get(){events.push('supports');if(input.failure==='supports')throw failure;return input.supports;}});
  receiver.options={get testnet(){events.push('testnet');if(input.failure==='testnet')throw failure;return input.testnet;}};
  let returned=null,error=false;
  try { returned=receiver.promoteUtaRoutingFromClassicUnsupported(payload); }
  catch(thrown) {assert.strictEqual(thrown,failure,'POLICY source route error identity');error=true;}
  const success=input.failure===null&&input.unsupported&&input.supports&&!input.testnet;
  assert.equal(receiver.isUta,success?true:input.initialUta,'POLICY source UTA flag');
  assert.equal(receiver._accountTypeDetected,success?true:input.initialDetected,'POLICY source detected flag');
  if(!error) assert.equal(returned,success,'POLICY source promotion result');
  cases.push({input,output:{uta:receiver.isUta,detected:receiver._accountTypeDetected,returned,events,error}});
}
for(const present of [false,true]) for(const unsupported of [false,true]) for(const supports of [false,true]) for(const testnet of [false,true]) for(const initialUta of [false,true]) {
  capturePromotion({kind:'promotion',present,unsupported,supports,testnet,initialUta,initialDetected:!initialUta,failure:null});
}
for(const failure of ['predicate','supports','testnet']) capturePromotion({kind:'promotion',present:true,unsupported:true,supports:true,testnet:false,initialUta:false,initialDetected:true,failure});
process.stdout.write(JSON.stringify({cases})+'\n');
