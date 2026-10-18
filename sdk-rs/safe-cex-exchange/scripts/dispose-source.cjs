// Disposal delegation is a source claim, including socket receivers and inherited members.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');

function hasSuperDispose(body) {
  let found = false;
  const scan = node => {
    // A nested class has its own super binding and cannot justify this method.
    if (ts.isClassDeclaration(node) || ts.isClassExpression(node)) return;
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) &&
        node.expression.expression.kind === ts.SyntaxKind.SuperKeyword && node.expression.name.text === 'dispose') {
      found = true;
    }
    ts.forEachChild(node, scan);
  };
  scan(body);
  return found;
}

class DisposeSources {
  constructor(root) {
    this.root = root;
    const files = [];
    const walk = dir => {
      for (const entry of fs.readdirSync(dir, {withFileTypes: true})) {
        const file = path.join(dir, entry.name);
        if (entry.isDirectory() && entry.name !== '__tests__') walk(file);
        else if (entry.isFile() && file.endsWith('.ts')) files.push(file);
      }
    };
    walk(path.join(root, 'packages/safe-cex/src/exchanges'));
    this.program = ts.createProgram(files, {
      target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext,
      moduleResolution: ts.ModuleResolutionKind.Bundler, skipLibCheck: true,
    });
    this.checker = this.program.getTypeChecker();
  }

  describe(file, name) {
    assert(file.startsWith('packages/safe-cex/src/exchanges/'), 'dispose source must be an exchange TS file');
    const source = this.program.getSourceFile(path.join(this.root, file));
    assert(source, `dispose source missing: ${file}`);
    const classes = [];
    const visit = node => {
      if (ts.isClassDeclaration(node) && node.name?.text === name) classes.push(node);
      ts.forEachChild(node, visit);
    };
    visit(source);
    assert.equal(classes.length, 1, `dispose class must resolve uniquely: ${file}::${name}`);
    const receiver = classes[0];
    let definition = receiver;
    const seen = new Set();
    let member;
    while (!(member = definition.members.find(m => m.name &&
      (ts.isIdentifier(m.name) || ts.isStringLiteral(m.name)) && m.name.text === 'dispose'))) {
      assert(!seen.has(definition), 'dispose inheritance cycle');
      seen.add(definition);
      const parent = definition.heritageClauses?.find(h => h.token === ts.SyntaxKind.ExtendsKeyword)?.types[0]?.expression;
      let symbol = parent && this.checker.getSymbolAtLocation(parent);
      if (symbol?.flags & ts.SymbolFlags.Alias) symbol = this.checker.getAliasedSymbol(symbol);
      definition = symbol?.declarations?.find(ts.isClassDeclaration);
      assert(definition, `dispose ancestor missing: ${file}::${name}`);
    }
    const body = member.body || member.initializer?.body;
    assert(body, `dispose body unsupported: ${file}::${name}`);
    const definedSource = member.getSourceFile();
    return {
      file, class: name,
      definition_file: path.relative(this.root, definedSource.fileName).split(path.sep).join('/'),
      definition_class: definition.name.text,
      start_line: definedSource.getLineAndCharacterOfPosition(member.getStart()).line + 1,
      end_line: definedSource.getLineAndCharacterOfPosition(member.end).line + 1,
      own: receiver === definition,
      super_call: hasSuperDispose(body),
    };
  }

  validate(row) {
    assert(Array.isArray(row.dispose_source) && row.dispose_source.length,
      `dispose classification needs TS source lines: ${row.file}: ${row.finding}`);
    for (const claim of row.dispose_source) {
      if (row.file.startsWith('safe-cex-venue-')) {
        const venue = row.file.split('/')[0].slice('safe-cex-venue-'.length);
        assert(claim.file.startsWith(`packages/safe-cex/src/exchanges/${venue}/`),
          'dispose citation must belong to the same venue');
      }
      const actual = this.describe(claim.file, claim.class);
      assert.deepEqual(claim, actual, `dispose source claim changed: ${claim.file}::${claim.class}`);
      if (row.kind === 'base-implementation') {
        assert(row.file === 'safe-cex-exchange/src/js_runtime/base_exchange.rs' &&
          actual.own && actual.class === 'BaseExchange' && !actual.super_call,
          'base dispose bridge must cite the BaseExchange implementation');
      } else if (row.kind === 'socket-receiver' && !actual.own) {
        assert.equal(actual.definition_class, 'BaseWebSocket', 'socket disposal must inherit the actual base method');
      } else {
        if (row.kind === 'socket-receiver') {
          assert(actual.file.includes('.ws-'), 'socket receiver must cite a source socket class');
        }
        assert(['source-super', 'socket-receiver'].includes(row.kind) && actual.own && actual.super_call,
          `dispose base cleanup has no source super call: ${claim.file}::${claim.class} (${row.kind})`);
      }
    }
  }
}

function validateDisposals(root, review) {
  const sources = new DisposeSources(root);
  for (const row of review.delegations) {
    if (/hook dispose\b|BaseExchange::dispose\b/.test(row.finding)) sources.validate(row);
  }
}
module.exports = {DisposeSources, validateDisposals, hasSuperDispose};
if (require.main === module) {
  const root = path.resolve(__dirname, '../../..');
  const review = JSON.parse(fs.readFileSync(process.argv[2] || path.join(__dirname, '../tests/override-base-delegations.json'), 'utf8'));
  validateDisposals(root, review);
}
