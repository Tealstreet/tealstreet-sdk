// Preserve original captures; admit only an explicitly reviewed source revision.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createHash} = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const pins = require('../tests/fixtures/lighter/reviewed-source-provenance.json');
const root = path.resolve(__dirname, '../../..');
const sharedTypes = 'packages/safe-cex/src/types.ts';
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const hash = text => createHash('sha256').update(text).digest('hex');
const parse = (file, text) => {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  assert.equal(source.parseDiagnostics.length, 0, `source syntax: ${file}`);
  return source;
};
const printer = ts.createPrinter({removeComments: true, newLine: ts.NewLineKind.LineFeed});

// Follow the actual emitted value imports: type-only imports such as Timeframe
// and unrelated ExchangeName/ExchangeOptions declarations have no signing effect.
function runtimeDeclarationNames() {
  const text = ts.transpileModule(read('packages/safe-cex/src/exchanges/lighter/lighter.types.ts'), {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  const source = parse('lighter.types.js', text);
  const aliases = new Set();
  function imports(node) {
    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer &&
        ts.isCallExpression(node.initializer) && node.initializer.expression.getText(source) === 'require' &&
        node.initializer.arguments[0]?.text === '../../types') aliases.add(node.name.text);
    ts.forEachChild(node, imports);
  }
  imports(source);
  assert.equal(aliases.size, 1, 'Lighter shared runtime import binding');
  const names = new Set();
  function uses(node) {
    if (ts.isPropertyAccessExpression(node) && ts.isIdentifier(node.expression) && aliases.has(node.expression.text)) {
      names.add(node.name.text);
    }
    ts.forEachChild(node, uses);
  }
  uses(source);
  assert.ok(names.size > 0, 'Lighter shared runtime declaration census must not be empty');
  return [...names].sort();
}

function assertDeclarations(file, pin, text) {
  assert.equal(file, sharedTypes, 'reviewed declaration scope');
  const names = Object.keys(pin.declarationsSha256).sort();
  assert.deepEqual(names, runtimeDeclarationNames(), 'Lighter runtime import coverage');
  const source = parse(file, text);
  for (const name of names) {
    const matches = source.statements.filter(node => ts.isEnumDeclaration(node) && node.name.text === name);
    assert.equal(matches.length, 1, `reviewed runtime declaration missing: ${file}::${name}`);
    assert.equal(hash(printer.printNode(ts.EmitHint.Unspecified, matches[0], source)),
      pin.declarationsSha256[name], `independent declaration drift: ${file}::${name}`);
  }
}

function verifySource(file, capturedHash) {
  const pin = pins[file];
  assert.ok(pin, `source has no reviewed provenance: ${file}`);
  const text = read(file);
  if (capturedHash !== undefined) {
    assert.equal(capturedHash, hash(text), `capture must identify current source bytes: ${file}`);
  }
  if (pin.declarationsSha256) assertDeclarations(file, pin, text);
  else assert.equal(hash(text), pin.currentSha256, `unreviewed independent source drift: ${file}`);
}

function originalProvenance(capture) {
  for (const [file, pin] of Object.entries(pins)) {
    assert.equal(typeof capture.sources[file], 'string', `captured source digest missing: ${file}`);
    verifySource(file, capture.sources[file]);
    capture.sources[file] = pin.originalSha256;
  }
  return capture;
}
module.exports = {originalProvenance, verifySource, assertDeclarations, runtimeDeclarationNames};
if (require.main === module) {
  try {
    assert.equal(process.argv[2], '--verify-source');
    verifySource(process.argv[3], process.argv[4]);
    console.log(JSON.stringify({verified: process.argv[3]}));
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  }
}
