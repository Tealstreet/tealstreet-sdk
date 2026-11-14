const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const { stripMutableDirectives } = require('./generate.cjs');
const names = ['StoreOptions', 'StoreDataLoaded', 'StoreUpdateIds', 'FetchedLeverage', 'PositionSettings', 'DisplayedMargin', 'StoreData'];

function generateStore({ check = false } = {}) {
  const destination = path.resolve(__dirname, '../../packages/safe-cex/src/generated');
  for (const name of names) {
    const filename = `${name}.ts`;
    const source = fs.readFileSync(path.join(__dirname, 'bindings/store/raw', filename), 'utf8');
    const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
    const alias = ast.statements.find((node) => ts.isTypeAliasDeclaration(node) && node.name.text === name);
    assert(alias && ts.isTypeLiteralNode(alias.type), `Rust-generated record ${name} required`);
    const referenced = new Set();
    function visit(node) {
      if (ts.isTypeReferenceNode(node) && ts.isIdentifier(node.typeName)) referenced.add(node.typeName.text);
      ts.forEachChild(node, visit);
    }
    visit(alias);
    const builtins = new Set(['Record', 'Partial', 'Array']);
    const dependencies = [...referenced].filter((n) => !builtins.has(n) && n !== name).sort();
    // Shared public imports resolve to each owning slice's generated runtime identity.
    const local = dependencies.filter((n) => names.includes(n));
    const special = {
      AccountReconnectDiagnostics: '@tealstreet/http-over-websocket/src/connectionDiagnostics',
      MessageHandlersStats: '../exchanges/types',
    };
    const external = dependencies.filter((n) => !names.includes(n) && !special[n]);
    const imports = local.map((n) => `import type { ${n} } from './${n}';`);
    for (const n of dependencies.filter((n) => special[n])) imports.push(`import type { ${n} } from '${special[n]}';`);
    if (external.length) imports.push(`import type { ${external.join(', ')} } from '../types';`);
    const converted = ts.transform(alias, [
      () => (node) => {
        const members = node.type.members.map((member) => {
          assert(ts.isPropertySignature(member), 'Store fields must be explicit named properties');
          const mutable = member.jsDoc?.some((doc) => doc.tags?.some((tag) => tag.tagName.text === 'ts-mutable'));
          return ts.factory.updatePropertySignature(
            member,
            mutable ? member.modifiers : [ts.factory.createModifier(ts.SyntaxKind.ReadonlyKeyword)],
            member.name,
            member.questionToken,
            member.type,
          );
        });
        return ts.factory.updateTypeAliasDeclaration(
          node,
          node.modifiers,
          node.name,
          node.typeParameters,
          ts.factory.updateTypeLiteralNode(node.type, members),
        );
      },
    ]);
    let output;
    try {
      output =
        '// Generated from safe-cex-types/src/store_model.rs. Do not edit.\n' +
        imports.join('\n') +
        (imports.length ? '\n' : '') +
        ts
          .createPrinter({ newLine: ts.NewLineKind.LineFeed })
          .printNode(ts.EmitHint.Unspecified, converted.transformed[0], ast) +
        '\n';
    } finally {
      converted.dispose();
    }
    if (name === 'StoreData') {
      const initial = JSON.parse(fs.readFileSync(path.join(__dirname, 'bindings/store/initial.json'), 'utf8'));
      output += `\nexport const initialStoreData: StoreData = ${JSON.stringify(initial, null, 2)};\n`;
    }
    output = stripMutableDirectives(output);
    const target = path.join(destination, filename);
    if (check) assert.equal(fs.readFileSync(target, 'utf8'), output, `regenerate ${name} from Rust`);
    else {
      fs.mkdirSync(destination, { recursive: true });
      fs.writeFileSync(target, output);
    }
  }
  return true;
}
if (require.main === module) generateStore({ check: process.argv.includes('--check') });
module.exports = { generateStore };
