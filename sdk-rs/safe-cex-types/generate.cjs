const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');

// In-process capabilities retain TS ownership until the IO/FFI implementation phase.
const runtimeBoundaries = ['RemoteSigner', 'PayloadSigner', 'TransportProvider', 'PublicDataSource'];

function stringLiterals(type) {
  const types = ts.isUnionTypeNode(type) ? type.types : [type];
  return types.every(t => ts.isLiteralTypeNode(t) && ts.isStringLiteral(t.literal))
    ? types.map(t => t.literal.text) : undefined;
}

function transform(source, metadata, filename, unions = {}) {
  const parsed = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
  assert.equal(parsed.parseDiagnostics.length, 0, `${filename}: invalid raw TypeScript`);
  const enumNames = [];
  const transformed = ts.transform(parsed, [context => {
    const visit = node => {
      if (ts.isTypeAliasDeclaration(node)) {
        const values = stringLiterals(node.type);
        if (values) {
          const name = node.name.text;
          const members = metadata[name];
          if (!members) {
            assert(unions[name], `${name}: missing Rust enum metadata or literal-alias intent`);
            assert.deepEqual(values, unions[name].map(m => m.value), `${name}: raw alias literal drift`);
            return node;
          }
          assert.equal(new Set(members.map(m => m.name)).size, members.length, `${name}: duplicate member name`);
          assert.equal(new Set(members.map(m => m.value)).size, members.length, `${name}: duplicate literal value`);
          assert.deepEqual(values, members.map(m => m.value), `${name}: raw literal drift`);
          enumNames.push(name);
          const declaration = ts.factory.createEnumDeclaration(node.modifiers, node.name,
            members.map(m => ts.factory.createEnumMember(m.name, ts.factory.createStringLiteral(m.value))));
          return ts.setTextRange(ts.setOriginalNode(declaration, node), node);
        }
        assert(!metadata[node.name.text], `${node.name.text}: expected raw string union`);
      }
      if (ts.isPropertySignature(node)) {
        // Rust field documentation owns mutability; the checker compares the oracle structurally.
        const mutable = node.jsDoc?.some(doc => doc.tags?.some(tag => tag.tagName.text === "ts-mutable"));
        const modifiers = node.modifiers?.filter(m => m.kind !== ts.SyntaxKind.ReadonlyKeyword) || [];
        return ts.factory.updatePropertySignature(node,
          mutable ? modifiers : [...modifiers, ts.factory.createModifier(ts.SyntaxKind.ReadonlyKeyword)],
          node.name, node.questionToken, ts.visitNode(node.type, visit));
      }
      return ts.visitEachChild(node, visit, context);
    };
    return root => ts.visitNode(root, visit);
  }]);
  try {
    return {
      source: ts.createPrinter({ newLine: ts.NewLineKind.LineFeed }).printFile(transformed.transformed[0]),
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
  fs.writeFileSync(path.join(bindings, 'runtime-boundaries.ts'), `// Runtime capabilities deliberately retain TypeScript ownership; see TYPE_PARITY.md.\nexport type { RemoteSigner, PayloadSigner, PublicDataSource } from '../../../packages/safe-cex/src/types';\nexport type { TransportProvider } from '../../../packages/safe-cex/src/transport/types';\n`);
  assert.deepEqual(seen.sort(), Object.keys(metadata).sort(), 'every exported Rust enum must be rendered exactly once');
}

if (require.main === module) generate(path.join(__dirname, 'bindings'));
module.exports = { transform, stringLiterals };
