const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const { transform } = require('./generate.cjs');

const names = [
  'HistoricalOrder',
  'HistoricalTrade',
  'ClosedPnlEntry',
  'IncomeRecord',
  'DataProvenance',
  'IncomeCategory',
];
const sharedOrderEnums = new Set(['OrderSide', 'OrderType', 'OrderStatus']);

function generateHistory({ check = false } = {}) {
  const raw = path.join(__dirname, 'bindings/history/raw');
  const destination = path.resolve(__dirname, '../../packages/safe-cex/src/generated');
  for (const name of names) {
    const filename = `${name}.ts`;
    const source = fs.readFileSync(path.join(raw, filename), 'utf8');
    const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
    const alias = ast.statements.find((node) => ts.isTypeAliasDeclaration(node) && node.name.text === name);
    assert(alias, `missing Rust-generated ${name}`);
    const unions =
      name === 'DataProvenance' || name === 'IncomeCategory'
        ? { [name]: alias.type.types.map((node) => ({ value: node.literal.text })) }
        : {};
    let output = transform(source, {}, filename, unions).source;
    // Order owns these runtime enums; the public entry point re-exports that identity.
    output = output.replace(/from ["']\.\/(OrderSide|OrderType|OrderStatus)["']/g, (_match, dependency) => {
      assert(sharedOrderEnums.has(dependency));
      return 'from "../types"';
    });
    const target = path.join(destination, filename);
    if (check) {
      assert.equal(fs.readFileSync(target, 'utf8'), output, `regenerate ${name} from Rust`);
    } else {
      fs.mkdirSync(destination, { recursive: true });
      fs.writeFileSync(target, output);
    }
  }
  return true;
}

if (require.main === module) generateHistory({ check: process.argv.includes('--check') });
module.exports = { generateHistory };
