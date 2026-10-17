const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const file = path.resolve(__dirname, '../../../packages/http-over-websocket/src/tunnelEndpoints.ts');
const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const exportsValue = {};
vm.runInNewContext(code, { exports: exportsValue, URL }, { filename: file });
const urls = JSON.parse(fs.readFileSync(0, 'utf8'));
process.stdout.write(JSON.stringify(urls.map(url => {
  try {
    const parsed = exportsValue.parseRelayUrl(url);
    return { ok: [parsed.tunnelUrl, parsed.upstream] };
  } catch (error) {
    return { name: error.name, message: error.message };
  }
})));
