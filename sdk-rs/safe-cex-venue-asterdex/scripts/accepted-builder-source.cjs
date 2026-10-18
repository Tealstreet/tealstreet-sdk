// Reviewed TS bodies and erased record schemas; fixed expectations live in contracts.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root, values } = require('./source.cjs');
const load = require('./load-module.cjs');
const { schemas, shape, accepts } = require('./oracle.cjs');
const unavailable = new Proxy(function () { throw Error('Unselected source dependency invoked'); }, {
  get(_, key) {
    if (key === '__esModule') return false;
    return () => { throw Error(`Unselected source dependency invoked: ${String(key)}`); };
  },
});
const fee = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts',
  ['parseScaledDecimal', 'normalizeDecimalString', 'assertFeeRate'],
  { ethers: unavailable, '../../types': unavailable });
const approval = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts',
  ['getBaseUrl', 'extractBuilderRows', 'parseFeeRate', 'getProviderErrorCode', 'getProviderErrorMessage'],
  { ethers: unavailable, '../../transport/registry': unavailable, './asterdex.api': unavailable,
    './asterdex.builder-fee': fee, './asterdex.types': values });
for (const file of ['asterdex.builder-fee.ts', 'asterdex.builder-approval.ts']) {
  const ast = ts.createSourceFile(file, fs.readFileSync(path.join(root,
    'packages/safe-cex/src/exchanges/asterdex', file), 'utf8'), ts.ScriptTarget.Latest, true);
  for (const node of ast.statements) {
    if (ts.isTypeAliasDeclaration(node) && node.modifiers?.some(item => item.kind === ts.SyntaxKind.ExportKeyword)) {
      schemas[node.name.text] = shape(node.type);
    }
  }
}
const optional = value => value ?? null;
const adapters = {
  get_base_url: input => approval.getBaseUrl(input.testnet, input.base),
  extract_builder_rows: input => approval.extractBuilderRows(input.payload),
  parse_fee_rate: input => {
    try {
      const number = approval.parseFeeRate(input.value);
      if (number === undefined) return null;
      const bytes = Buffer.alloc(8);
      bytes.writeDoubleBE(number);
      return { bits: bytes.toString('hex') };
    } catch (error) { return { error: error.name }; }
  },
  get_provider_error_code: input => {
    const number = approval.getProviderErrorCode(input.error);
    if (number === undefined) return null;
    const bytes = Buffer.alloc(8);
    bytes.writeDoubleBE(number);
    return { bits: bytes.toString('hex') };
  },
  get_provider_error_message: input => approval.getProviderErrorMessage(
    Object.hasOwn(input, 'instanceMessage') ? new Error(input.instanceMessage) : input.error, input.fallback),
  parse_scaled_decimal: input => optional(fee.parseScaledDecimal(input)?.toString()),
  normalize_decimal_string: input => optional(fee.normalizeDecimalString(input)),
  assert_fee_rate: input => {
    try { return { value: fee.assertFeeRate(input.value, input.field) }; }
    catch (error) { return { error: error.message }; }
  },
};
async function main() {
  assert.equal(process.argv.length, 2, 'builder source batch takes no scope-changing arguments');
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  let functions = 0;
  for (const [name, observe] of Object.entries(adapters)) {
    const owner = `safe_cex_venue_asterdex::builder::${name}`;
    assert.equal(contracts[owner]?.category, 'decoder', owner);
    for (const row of contracts[owner].cases) {
      assert.deepEqual(observe(structuredClone(row.input)), row.output,
        `${owner}: actual TS output for ${JSON.stringify(row.input)}`);
      functions++;
    }
  }
  let records = 0;
  for (const name of ['AsterBuilderFeeConfig', 'AsterBuilderApprovalCheckResult',
    'AsterBuilderApprovalResult', 'AsterBuilderApprovalStatus']) {
    const contract = contracts[`safe_cex_venue_asterdex::builder::${name}`];
    assert.ok(contract && schemas[name], name);
    for (const row of contract.nativeObservation.cases) {
      assert.equal(accepts(schemas[name], row.input), row.output.ok,
        `${name}: maintained TS shape for ${JSON.stringify(row.input)}`);
      records++;
    }
  }
  assert.equal(functions, 55);
  assert.equal(records, 27);
  console.log(`Aster builder actual source: 8 helpers / ${functions} expectations; 4 erased record schemas / ${records} shape witnesses pass (Node ${process.versions.node}, ${process.arch}).`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
