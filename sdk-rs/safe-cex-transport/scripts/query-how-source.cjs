const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const files = ['packages/http-over-websocket/src/axiosHttpOverWebsocketAdapter.ts', 'packages/http-over-websocket/src/HOWWebSocket.ts'];
function extract(file, predicate) {
  const source = fs.readFileSync(path.join(root, file), 'utf8');
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const matches = [];
  function visit(node) {
    if (predicate(node, ast)) matches.push(node.initializer.getText(ast));
    ts.forEachChild(node, visit);
  }
  visit(ast);
  if (matches.length !== 1) throw Error(`QUERY HOW source member count: ${file}: ${matches.length}`);
  return matches[0];
}
const requestExpression = extract(files[0], n => ts.isVariableDeclaration(n) && n.name.getText() === 'websocketRequest');
const sendExpression = extract(files[1], n => ts.isPropertyDeclaration(n) && n.name.getText() === 'send');
const compile = source => ts.transpileModule(source, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
const params = {nan: NaN, infinity: Infinity, zero: -0, text: '-0', missing: undefined, list: [undefined, -Infinity, 2.25]};
const request = new Function('config', 'newHeaders', compile(`return (${requestExpression});`))({params, method: 'get', url: '/query'}, {});
if (request.params !== params || !Number.isNaN(request.params.nan) || !Object.is(request.params.zero, -0)) throw Error('QUERY HOW source retained native params');
const frames = [];
const host = {validateAndSend(message, data, token) { if (token || data.arg.params !== params) throw Error('QUERY HOW source send identity'); frames.push(message); }, trackDomainFromUrl() {}};
const send = new Function(compile(`return (${sendExpression});`)).call(host);
send({type: 'rpc.http', arg: request});
if (frames.length !== 1) throw Error('QUERY HOW source frame total');
console.log(JSON.stringify({architecture: process.arch, nodeMajor: process.versions.node.split('.')[0], hashes: files.map(file => crypto.createHash('sha256').update(fs.readFileSync(path.join(root,file))).digest('hex')), params: JSON.parse(frames[0]).arg.params}));
