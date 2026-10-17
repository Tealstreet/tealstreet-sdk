const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const zlib = require('node:zlib');
const root = path.resolve(__dirname,'../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
function ast(relative) {
  const file = path.join(root, relative);
  return ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
}
function compile(text) {
  return ts.transpileModule(text, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
}
function moduleCode(relative) {
  const source = ast(relative);
  return compile(source.statements.filter(node => !ts.isImportDeclaration(node)).map(node => node.getText(source)).join('\n'));
}
const parsing = {};
new Function('exports', moduleCode('packages/json-parse/src/jsonParse.ts'))(parsing);
const binary = {};
new Function('exports', 'jsonParse', moduleCode('packages/http-over-websocket/src/tradeTunnelBinaryFrame.ts'))(binary, parsing.jsonParse);
const compression = {};
new Function('exports', moduleCode('packages/compression/src/decompress.ts'))(compression);
const adapter = ast('packages/http-over-websocket/src/axiosHttpOverWebsocketAdapter.ts');
const rawNames = ['decodeBase64', 'decodeTradeTunnelRawBody'];
const rawNodes = adapter.statements.filter(node => ts.isFunctionDeclaration(node) && rawNames.includes(node.name?.text));
assert.equal(rawNodes.length, 2);
const encodingNode = adapter.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(decl => decl.name.getText(adapter) === 'TRADE_TUNNEL_GZIP_BASE64_ENCODING'));
assert(encodingNode);
const decodeRaw = new Function('decompress', compile(encodingNode.getText(adapter) + '\n' + rawNodes.map(node => node.getText(adapter)).join('\n')) + '\nreturn decodeTradeTunnelRawBody;')(compression.decompress);
const parseRequestNode=adapter.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='parseWebsocketData');
assert(parseRequestNode);
const parseRequest=new Function(compile(parseRequestNode.getText(adapter))+'\nreturn parseWebsocketData;')();
const nativeRequest={nan:NaN,missing:undefined,zero:-0,slots:[undefined,Infinity]};
let parserCalls=0;
const forwarded=parseRequest(()=>{parserCalls++;throw new Error('non-string must bypass parser')},{data:nativeRequest});
assert.equal(forwarded,nativeRequest);assert.equal(parserCalls,0);assert(Number.isNaN(forwarded.nan));assert(Object.is(forwarded.zero,-0));assert(Object.hasOwn(forwarded,'missing'));
const requestWire=JSON.parse(JSON.stringify(forwarded));
const socketAst = ast('packages/http-over-websocket/src/HOWWebSocket.ts');
const owner = socketAst.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'HOWWebSocket');
const field = owner.members.find(node => ts.isPropertyDeclaration(node) && node.name.getText(socketAst) === 'onMessage');
assert(field?.initializer);
const Source = new Function('decodeTradeTunnelMessage', compile('class Source {onMessage=' + field.initializer.getText(socketAst) + ';}') + '\nreturn Source;')(binary.decodeTradeTunnelMessage);
const body = '{"label":"tp \\ud800","\\udfff":"\\ud800x","list":[{"id":1,"amount":-0},{"id":9007199254740993,"amount":1e400}],"dup":1,"dup":2,"own":{"1":"one","0":"zero","a":"tail"},"low":"\\udfff","tiny":-1e-999}';
const units = value => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
function bits(value) {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
}
function inspect(value) {
  return { keys: Object.keys(value).map(units), label: units(value.label), loneKeyValue: units(value['\udfff']), low: units(value.low),
    rows: value.list.map(row => ({ keys: Object.keys(row), id: bits(row.id), amount: bits(row.amount) })),
    duplicate: bits(value.dup), ownKeys: Object.keys(value.own), ownValues: Object.values(value.own), tiny: bits(value.tiny) };
}
async function emit(frame) {
  const socket = new Source(); socket.isDisposed = false;
  const emitted = []; socket.emit = (event, data) => emitted.push({ event, data });
  await socket.onMessage({ data: frame });
  assert.equal(emitted.length, 1); assert.equal(emitted[0].event, 'sub:witness');
  return emitted[0].data;
}
(async () => {
  const envelope = '{"type":"rpc.http","nonce":"witness","ignored":"\\ud800","data":{"status":200,"headers":{},"data":' + body + '}}';
  const ordinary = inspect((await emit(envelope)).data);
  const raw = inspect(parsing.jsonParse(await decodeRaw({ data_raw: body })));
  const gzip = zlib.gzipSync(Buffer.from(body));
  const header = Buffer.from('{"type":"rpc.http","nonce":"witness","data":{"status":200,"headers":{},"data_encoding":"gzip"}}');
  const prefix = Buffer.alloc(8); prefix.write('TSB1'); prefix.writeUInt32BE(header.length, 4);
  const compressed = await emit(Buffer.concat([prefix, header, gzip]));
  const compressedBody = await decodeRaw(compressed);
  const gzipBinary = inspect(parsing.jsonParse(compressedBody));
  const gzipBase64 = inspect(parsing.jsonParse(await decodeRaw({ data_encoding: 'gzip+base64', data_encoded: gzip.toString('base64') })));
  assert.deepEqual(ordinary, raw); assert.deepEqual(ordinary, gzipBinary); assert.deepEqual(ordinary, gzipBase64);
  assert.deepEqual(ordinary.label, [116,112,32,0xd800]); assert.deepEqual(ordinary.loneKeyValue, [0xd800,120]);
  assert.equal(ordinary.rows.length, 2); assert.equal(ordinary.rows[0].amount, '8000000000000000');
  assert.equal(ordinary.rows[1].id, '4340000000000000'); assert.equal(ordinary.rows[1].amount, '7ff0000000000000');
  assert.equal(ordinary.tiny, '8000000000000000'); assert.deepEqual(ordinary.ownKeys, ['0','1','a']);
  console.log(JSON.stringify({ body, ordinary, raw, gzipBinary, gzipBase64, requestWire, requestParserCalls:parserCalls, invalid: parsing.jsonParse('{'), delivered: 1 }));
})().catch(error => { console.error(error); process.exitCode = 1; });
