// Use the native CI job's existing frozen-lockfile TypeScript installation.
// No tsx, root workspace install, or silent missing-tool fallback is required.
const fs = require('node:fs');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
module.exports = (filename, transform = source => source) => {
  const source = transform(fs.readFileSync(filename, 'utf8'));
  const exports = {};
  vm.runInNewContext(ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText, { exports, process, URL, console }, { filename });
  return exports;
};
