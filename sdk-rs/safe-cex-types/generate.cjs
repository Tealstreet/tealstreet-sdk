const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');

// In-process capabilities retain TS ownership until the IO/FFI implementation phase.
const runtimeBoundaries = ['RemoteSigner', 'PayloadSigner', 'TransportProvider', 'PublicDataSource', 'LocalSignerMetadata', 'RustPublicFeedPolicy'];

function stringLiterals(type) {
  const types = ts.isUnionTypeNode(type) ? type.types : [type];
  return types.every((t) => ts.isLiteralTypeNode(t) && ts.isStringLiteral(t.literal))
    ? types.map((t) => t.literal.text)
    : undefined;
}

function stripMutableDirectives(source) {
  const ast = ts.createSourceFile('public.ts', source, ts.ScriptTarget.Latest, true);
  const edits = [];
  function visit(node) {
    for (const doc of node.jsDoc || []) {
      if (!doc.tags?.some((tag) => tag.tagName.text === 'ts-mutable')) continue;
      const original = source.slice(doc.pos, doc.end);
      const cleaned = original
        .split('\n')
        .filter((line) => !/^\s*\*\s*@ts-mutable\s*$/.test(line))
        .join('\n')
        .replace(/@ts-mutable\b/g, '');
      if (!cleaned.slice(3, -2).replace(/\*/g, '').trim()) {
        const start = source.lastIndexOf('\n', doc.pos - 1) + 1;
        const wholeLine = !source.slice(start, doc.pos).trim();
        edits.push([wholeLine ? start : doc.pos, doc.end + (wholeLine && source[doc.end] === '\n' ? 1 : 0), '']);
      } else edits.push([doc.pos, doc.end, cleaned]);
    }
    ts.forEachChild(node, visit);
  }
  visit(ast);
  for (const [start, end, replacement] of edits.sort((a, b) => b[0] - a[0]))
    source = source.slice(0, start) + replacement + source.slice(end);
  return source;
}

function transform(source, metadata, filename, unions = {}) {
  const parsed = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
  assert.equal(parsed.parseDiagnostics.length, 0, `${filename}: invalid raw TypeScript`);
  const enumNames = [];
  const transformed = ts.transform(parsed, [
    (context) => {
      const visit = (node) => {
        if (ts.isTypeAliasDeclaration(node)) {
          const values = stringLiterals(node.type);
          if (values) {
            const name = node.name.text;
            const members = metadata[name];
            if (!members) {
              assert(unions[name], `${name}: missing Rust enum metadata or literal-alias intent`);
              assert.deepEqual(
                values,
                unions[name].map((m) => m.value),
                `${name}: raw alias literal drift`,
              );
              return node;
            }
            assert.equal(new Set(members.map((m) => m.name)).size, members.length, `${name}: duplicate member name`);
            assert.equal(new Set(members.map((m) => m.value)).size, members.length, `${name}: duplicate literal value`);
            assert.deepEqual(
              values,
              members.map((m) => m.value),
              `${name}: raw literal drift`,
            );
            enumNames.push(name);
            const declaration = ts.factory.createEnumDeclaration(
              node.modifiers,
              node.name,
              members.map((m) => ts.factory.createEnumMember(m.name, ts.factory.createStringLiteral(m.value))),
            );
            return ts.setTextRange(ts.setOriginalNode(declaration, node), node);
          }
          assert(!metadata[node.name.text], `${node.name.text}: expected raw string union`);
        }
        if (ts.isPropertySignature(node)) {
          // Rust field documentation owns mutability; the checker compares the oracle structurally.
          const mutable = node.jsDoc?.some((doc) => doc.tags?.some((tag) => tag.tagName.text === 'ts-mutable'));
          const modifiers = node.modifiers?.filter((m) => m.kind !== ts.SyntaxKind.ReadonlyKeyword) || [];
          return ts.factory.updatePropertySignature(
            node,
            mutable ? modifiers : [...modifiers, ts.factory.createModifier(ts.SyntaxKind.ReadonlyKeyword)],
            node.name,
            node.questionToken,
            ts.visitNode(node.type, visit),
          );
        }
        return ts.visitEachChild(node, visit, context);
      };
      return (root) => ts.visitNode(root, visit);
    },
  ]);
  try {
    return {
      source: stripMutableDirectives(
        ts.createPrinter({ newLine: ts.NewLineKind.LineFeed }).printFile(transformed.transformed[0]),
      ),
      enumNames,
    };
  } finally {
    transformed.dispose();
  }
}

function generate(bindings) {
  const metadata = JSON.parse(fs.readFileSync(path.join(bindings, 'enum-members.json'), 'utf8'));
  const unions = JSON.parse(fs.readFileSync(path.join(bindings, "string-unions.json"), "utf8"));
  const exported = new Set(fs.readdirSync(path.join(bindings, 'raw')).filter(n => n.endsWith('.ts')).map(n => n.slice(0, -3)));
  const seen = [];
  for (const name of fs.readdirSync(path.join(bindings, 'raw')).sort()) {
    assert(name.endsWith('.ts'), `${name}: unexpected raw binding`);
    const output = transform(fs.readFileSync(path.join(bindings, 'raw', name), 'utf8'), metadata, name, unions);
    seen.push(...output.enumNames);
    const parsed = ts.createSourceFile(name, output.source, ts.ScriptTarget.Latest, true);
    const imported = new Set(parsed.statements.filter(ts.isImportDeclaration).flatMap(n => n.importClause?.namedBindings?.elements?.map(e => e.name.text) || []));
    const referenced = new Set();
    const visit = node => {
      if (ts.isTypeReferenceNode(node) && ts.isIdentifier(node.typeName)) referenced.add(node.typeName.text);
      ts.forEachChild(node, visit);
    };
    visit(parsed);
    const missing = [...referenced].filter(n => exported.has(n) && n !== name.slice(0, -3) && !imported.has(n)).sort();
    const boundaryNames = [...referenced].filter(n => runtimeBoundaries.includes(n)).sort();
    const boundaryImport = boundaryNames.length ? `import type { ${boundaryNames.join(', ')} } from './runtime-boundaries';\n` : '';
    const imports = missing.map(n => `import type { ${n} } from './${n}';`).join('\n');
    fs.writeFileSync(path.join(bindings, name), boundaryImport + (imports ? `${imports}\n${output.source}` : output.source));
  }
  fs.writeFileSync(path.join(bindings, 'runtime-boundaries.ts'), `// Runtime capabilities deliberately retain TypeScript ownership; see TYPE_PARITY.md.\nexport type { RemoteSigner, PayloadSigner, PublicDataSource } from '../../../packages/safe-cex/src/types';\nexport type { TransportProvider } from '../../../packages/safe-cex/src/transport/types';\nexport type { LocalSignerMetadata } from '@tealstreet/safe-cex-wasm/host';\nexport type { RustPublicFeedPolicy } from '../../../packages/safe-cex/src/exchanges/rust/publicFeed';\n`);
  assert.deepEqual(seen.sort(), Object.keys(metadata).sort(), 'every exported Rust enum must be rendered exactly once');
}

function publishModels() {
  const bindings = path.join(__dirname, 'bindings');
  const destination = path.resolve(__dirname, '../../packages/safe-cex/src/generated');
  fs.mkdirSync(destination, { recursive: true });
  const visited = new Set();
  const copy = name => {
    if (visited.has(name)) return;
    visited.add(name);
    const filename = `${name}.ts`;
    const source = fs.readFileSync(path.join(bindings, filename), 'utf8');
    const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
    for (const statement of ast.statements) {
      if (ts.isImportDeclaration(statement)) {
        assert(statement.moduleSpecifier.text.startsWith('./'), 'model dependency must be local');
        copy(statement.moduleSpecifier.text.slice(2));
      }
    }
    fs.writeFileSync(path.join(destination, filename), source);
  };
  for (const name of ['Position', 'Order', 'PlaceOrderOpts', 'UpdateOrderOpts', 'Market', 'Ticker']) copy(name);
}

function publishExecution() {
  const destination = path.resolve(__dirname, '../../packages/safe-cex/src/generated');
  fs.mkdirSync(destination, { recursive: true });
  for (const name of ['OrderFillEvent', 'Execution', 'PublicTrade']) {
    const source = fs.readFileSync(path.join(__dirname, 'bindings', `${name}.ts`), 'utf8');
    // Order owns this enum; consumers use its stable public identity.
    fs.writeFileSync(path.join(destination, `${name}.ts`), source.replace('from "./OrderSide"', 'from "../types"'));
  }
}

function publishMarketData() {
  for (const name of ['Candle', 'OrderBook', 'OrderBookOrders']) {
    const source = fs
      .readFileSync(path.join(__dirname, 'bindings', `${name}.ts`), 'utf8')
      .replace('from "./OrderSide"', 'from "../types"');
    fs.writeFileSync(path.resolve(__dirname, '../../packages/safe-cex/src/generated', `${name}.ts`), source);
  }
}

module.exports = { transform, stringLiterals, stripMutableDirectives };

if (require.main === module) {
  generate(path.join(__dirname, 'bindings'));
  publishModels();
  publishExecution();
  publishMarketData();
  require('./generate-balance.cjs').generate();
  const { renderOptions } = require('./tests/options-render.cjs');
  for (const name of ['ExchangeOptions', 'ExchangeAccount', 'ExchangeAccountConfig']) {
    fs.writeFileSync(path.resolve(__dirname, `../../packages/safe-cex/src/generated/${name}.ts`),
      renderOptions(name, fs.readFileSync(path.join(__dirname, 'bindings/raw', `${name}.ts`), 'utf8'), transform));
  }
}
