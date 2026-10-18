const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function load(file) {
  const mod = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  new Function('require', 'module', 'exports', code)(
    name => load(path.resolve(path.dirname(file), `${name}.ts`)), mod, mod.exports);
  return mod.exports;
}
const { ReconnectingWebSocket } = load(path.join(root, 'thirdparty/reconnecting-websocket/src/ReconnectingWebsocket.ts'));
process.on('uncaughtException', () => {});
process.on('unhandledRejection', () => {});
async function main() {
  const rows = [];
  for (const grouped of [false, true]) {
    for (const mode of ['throw', 'reject', 'deferred-reject']) {
      const rejection = mode !== 'throw', deferred = mode === 'deferred-reject';
      const socket = grouped
        ? new ReconnectingWebSocket('ws://unused', [], { startClosed: true })
        : new EventTarget();
      const trace = [];
      socket.addEventListener('message', () => {
        trace.push('first');
        if (deferred) return Promise.resolve().then(() => { throw new Error('listener rejection'); });
        if (rejection) return Promise.reject(new Error('listener rejection'));
        throw new Error('listener throw');
      });
      socket.addEventListener('message', () => trace.push('second'));
      socket.addEventListener('error', () => trace.push('socket-error'));
      try { socket.dispatchEvent(new Event('message')); } catch (error) {
        if (!grouped || error.message !== 'listener throw') throw error;
      }
      rows.push({ grouped, rejection, deferred, trace });
    }
  }
  await new Promise(resolve => setImmediate(resolve));
  console.log(JSON.stringify(rows));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
