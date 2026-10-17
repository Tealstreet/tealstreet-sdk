const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
function load(file) {
  const mod = {exports: {}};
  const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  new Function('require', 'module', 'exports', code)(
    name => load(path.resolve(path.dirname(file), `${name}.ts`)), mod, mod.exports);
  return mod.exports;
}
const {ReconnectingWebSocket} = load(path.resolve(__dirname, '../../../thirdparty/reconnecting-websocket/src/ReconnectingWebsocket.ts'));
const rows = [];
for (const event of ['open', 'message']) for (const grouped of [false, true]) {
  for (const kind of ['remove', 'remove-add', 'unrelated', 'pre-remove']) {
    const ws = grouped ? new ReconnectingWebSocket('ws://unused', [], {startClosed: true}) : new EventTarget();
    const trace = [];
    const second = () => trace.push('second');
    const unrelated = () => {};
    let firstTime = true;
    ws.addEventListener(event, () => {
      trace.push('first');
      if (!firstTime) return;
      firstTime = false;
      if (kind === 'remove' || kind === 'remove-add') ws.removeEventListener(event, second);
      if (kind === 'remove-add') ws.addEventListener(event, second);
      if (kind === 'unrelated') ws.removeEventListener(event, unrelated);
    });
    ws.addEventListener(event, second);
    if (kind !== 'remove-add') ws.addEventListener(event, () => trace.push('third'));
    if (kind === 'pre-remove') ws.removeEventListener(event, second);
    ws.dispatchEvent(new Event(event));
    if (event === 'message') ws.dispatchEvent(new Event(event));
    rows.push({event, grouped, kind, trace});
  }
}
process.stdout.write(JSON.stringify(rows));
