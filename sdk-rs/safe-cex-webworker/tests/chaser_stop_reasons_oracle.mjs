// New stop-reason supplement; original task corpora remain unchanged.
// Capture provenance is emitted for each host; strings have no numeric bit projection.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const filename = new URL('../../../packages/safe-cex/src/webworker/chaserOrder/ChaserOrderOptions.ts', import.meta.url);
const source = fs.readFileSync(filename, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
}).outputText;
const exports = {};
vm.runInNewContext(compiled, {exports}, {filename: filename.pathname});
process.stdout.write(JSON.stringify({
  architecture: process.arch,
  nodeMajor: Number(process.versions.node.split('.')[0]),
  values: exports,
}));
