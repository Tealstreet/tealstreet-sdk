import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import { spawnSync } from 'node:child_process';

const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = fileURLToPath(new URL('../../../', import.meta.url));
const crate = fileURLToPath(new URL('../', import.meta.url));
const variants = ['binance', 'binancepm', 'binancespot'];
const parse = (file) => ts.createSourceFile(file, fs.readFileSync(root + file, 'utf8'), ts.ScriptTarget.Latest, true);
const shared = parse('packages/safe-cex/src/types.ts');
const enums = Object.fromEntries(shared.statements.filter(ts.isEnumDeclaration).map(node => [node.name.text, node]));
const snake = name => name.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
const pascal = name => name.includes('_') ? name.toLowerCase().split('_').map(s => s[0].toUpperCase() + s.slice(1)).join('') : name;
const quote = value => JSON.stringify(value);
const mappings = {};
const snapshots = {};
const schemas = {};

function primitive(node) {
  if (node.kind === ts.SyntaxKind.StringKeyword) return 'String';
  if (node.kind === ts.SyntaxKind.NumberKeyword) return 'f64';
  if (node.kind === ts.SyntaxKind.BooleanKeyword) return 'bool';
  if (ts.isTypeReferenceNode(node)) return pascal(node.typeName.getText());
  throw Error('Unsupported wire field: ' + node.getText());
}
function record(name, node) {
  assert.ok(ts.isTypeLiteralNode(node), name);
  const fields = node.members.map(field => {
      assert.ok(ts.isPropertySignature(field) && field.type, field.getText());
      const key = field.name.text;
      const optional = !!field.questionToken;
      const type = primitive(field.type);
      return `#[serde(rename = ${quote(key)}${optional ? ', default, deserialize_with = "crate::deserialize_present", skip_serializing_if = "Option::is_none"' : ''})]\npub ${snake(key)}: ${optional ? `Option<${type}>` : type},`;
    }).join('\n');
  const names = node.members.map(field => snake(field.name.text));
  return `#[derive(Debug, Clone, PartialEq, serde::Serialize)]
    pub struct ${name} { ${fields} }
    #[derive(serde::Deserialize)]
    #[serde(remote = "${name}")]
    struct ${name}Wire { ${fields} }
    impl safe_cex_num::numeric_ingress::JsNumberOutput for ${name} {
      fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { ${names.join(',')} });
      }
    }
    impl<'de> serde::Deserialize<'de> for ${name} {
      fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(deserializer)?;
        input.decode_with(|decoder| ${name}Wire::deserialize(safe_cex_types::strict_decode::object(decoder)))
          .map_err(serde::de::Error::custom)
      }
    }\n`;
}
function rustValue(value) {
  if (typeof value === 'string') return ["&str", quote(value)];
  if (typeof value === 'number') return ['f64', String(value) + (Number.isInteger(value) ? '.0' : '')];
  if (Array.isArray(value)) {
    const rows = value.map(rustValue);
    assert.ok(rows.length && rows.every(row => row[0] === rows[0][0]));
    return [`&[${rows[0][0]}]`, '&[' + rows.map(row => row[1]).join(',') + ']'];
  }
  const rows = Object.entries(value).map(([key, item]) => [key, ...rustValue(item)]);
  assert.ok(rows.length && rows.every(row => row[1] === rows[0][1]));
  return [`&[(&str, ${rows[0][1]})]`, '&[' + rows.map(([key, , expression]) => `(${quote(key)},${expression})`).join(',') + ']'];
}
for (const variant of variants) {
  const source = `packages/safe-cex/src/exchanges/binance/${variant}.types.ts`;
  const ast = parse(source);
  const enumNames = ast.statements.filter(ts.isImportDeclaration).flatMap(node => {
    assert.equal(node.moduleSpecifier.text, '../../types');
    return node.importClause.namedBindings.elements.map(item => item.name.text);
  });
  const exports = {};
  const enumExports = {};
  const enumText = enumNames.map(name => { assert.ok(enums[name], name); return enums[name].getText(shared); }).join('\n');
  vm.runInNewContext(ts.transpileModule(enumText, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: enumExports });
  vm.runInNewContext(ts.transpileModule(ast.text, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, {
    exports, require: id => { assert.equal(id, '../../types'); return enumExports; },
  });
  snapshots[variant] = exports;
  let output = '// Generated from the live TypeScript types source by scripts/types.mjs. Do not edit.\n';
  const rows = {};
  for (const statement of ast.statements) {
    if (ts.isImportDeclaration(statement)) continue;
    if (ts.isVariableStatement(statement)) {
      for (const decl of statement.declarationList.declarations) {
        const name = decl.name.text;
        let [type, expression] = rustValue(exports[name]);
        if (decl.type && ts.isTypeReferenceNode(decl.type) && decl.type.typeName.getText(ast) === 'Record') {
          const enumName = decl.type.typeArguments[1].getText(ast);
          assert.ok(enumNames.includes(enumName), enumName);
          type = `&[(&str, safe_cex_types::${enumName})]`;
          expression = '&[' + Object.entries(exports[name]).map(([key,value]) => {
            const members = Object.entries(enumExports[enumName]).filter(([, wire]) => wire === value);
            assert.equal(members.length, 1);
            return `(${quote(key)},safe_cex_types::${enumName}::${members[0][0]})`;
          }).join(',') + ']';
        }
        output += `pub const ${name}: ${type} = ${expression};\n`;
        rows[name] = [name];
      }
    } else if (ts.isTypeAliasDeclaration(statement)) {
      const sourceName = statement.name.text;
      const name = pascal(sourceName);
      const node = statement.type;
      rows[sourceName] = [name];
      if (ts.isArrayTypeNode(node)) {
        output += record(name + 'Row', node.elementType);
        output += `pub type ${name} = Vec<${name}Row>;\n`;
        rows[sourceName].push(name + 'Row');
      } else if (ts.isUnionTypeNode(node)) {
        const members = node.types.map(member => {
          assert.ok(ts.isLiteralTypeNode(member) && ts.isStringLiteral(member.literal));
          return member.literal.text;
        });
        schemas[name] = members;
        const fields = members.map(value => `#[serde(rename = ${quote(value)})] ${pascal(value)},`).join('\n');
        output += `#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
          pub enum ${name} { ${fields} }
          #[derive(serde::Deserialize)]
          #[serde(remote = "${name}")]
          // Remote variants must retain the public source-derived identifiers.
          #[allow(clippy::upper_case_acronyms)]
          enum ${name}Wire { ${fields} }
          impl safe_cex_num::numeric_ingress::JsNumberOutput for ${name} {
            fn check_number_output(&self) {
              safe_cex_num::js_number_output!(self, enum { ${members.map(pascal).join(',')} });
            }
          }
          impl<'de> serde::Deserialize<'de> for ${name} {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
              let text = <String as serde::Deserialize>::deserialize(deserializer)?;
              ${name}Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(text))
            }
          }\n`;
      } else output += record(name, node);
    } else throw Error('Unclassified source statement: ' + statement.getText(ast));
  }
  // Root yarn invocations must use the workspace pin and its rustfmt component.
  const formatted = spawnSync(process.env.RUSTFMT || 'rustfmt', ['--edition', '2024'], {
    cwd: process.env.RUSTFMT ? undefined : root + 'sdk-rs',
    input: output, encoding: 'utf8',
  });
  assert.equal(formatted.status, 0, formatted.stderr);
  const file = `${crate}src/generated/${variant}.rs`;
  if (process.argv.includes('--generate')) fs.writeFileSync(file, formatted.stdout);
  else if (!process.argv.includes('--oracle') && !process.argv.includes('--schema')) assert.equal(fs.readFileSync(file, 'utf8'), formatted.stdout, `${source} generated table/type drift`);
  mappings[variant] = rows;
}
if (process.argv.includes('--mappings')) console.log(JSON.stringify(mappings));
else if (process.argv.includes('--schema')) console.log(JSON.stringify(schemas));
else if (process.argv.includes('--oracle')) console.log(JSON.stringify(snapshots));
else console.log('Binance types: three live source modules match generated Rust.');
