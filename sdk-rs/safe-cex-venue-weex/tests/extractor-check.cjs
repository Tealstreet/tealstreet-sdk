// Scope the canonical extractor to WEEX's six source files.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { execFileSync } = require('node:child_process');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const extractor = path.join(root, 'sdk-rs/parity/extract-ts/src/extract.ts');
const compiled = ts.transpileModule(fs.readFileSync(extractor, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true },
}).outputText;
const exportsObject = {};
vm.runInThisContext(`(function(require,exports){${compiled}\n})`, { filename: extractor })(
  name => name === 'typescript' ? ts : require(name), exportsObject,
);
const directory = path.join(root, 'packages/safe-cex/src/exchanges/weex');
const files = fs.readdirSync(directory).filter(name => name.endsWith('.ts')).map(name => path.join(directory, name));
const configPath = path.join(root, 'packages/safe-cex/tsconfig.json');
const config = ts.readConfigFile(configPath, ts.sys.readFile);
assert.equal(config.error, undefined);
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configPath), undefined, configPath);
assert.equal(parsed.errors.length, 0);
const program = ts.createProgram(files, parsed.options);
const checker = program.getTypeChecker();
function requireNoDiagnostics(diagnostics) {
  if (diagnostics.length) throw new Error(ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: name => name,
    getCurrentDirectory: () => root,
    getNewLine: () => '\n',
  }));
}
requireNoDiagnostics([...program.getOptionsDiagnostics(), ...program.getGlobalDiagnostics()]);
const manifests = JSON.parse(execFileSync('python3.12', ['-c', `
import pathlib,tomllib,json
p=pathlib.Path('sdk-rs/parity/manifest/exchanges/weex')
print(json.dumps({f.name.removesuffix('.toml'):[{'ts':r['ts'],'kind':r['kind']} for r in tomllib.loads(f.read_text())['symbol']] for f in p.glob('*.toml')}))
`], { cwd: root, encoding: 'utf8' }));
let count = 0;
for (const file of files) {
  const source = program.getSourceFile(file);
  assert.ok(source, `missing source ${file}`);
  requireNoDiagnostics([...program.getSyntacticDiagnostics(source), ...program.getSemanticDiagnostics(source)]);
  const symbols = exportsObject.extractFile(source, checker, path.join(root, 'packages/safe-cex'));
  assert.deepEqual(symbols, manifests[path.basename(file)], `extractor_drift::${path.basename(file)}`);
  count += symbols.length;
}
console.log(`WEEX scoped canonical extractor: ${count} rows, ${files.length} files; zero identity/kind drift`);
