// Wire schemas are actual TS declarations. The separately executed actual TS
// consumers prove the ten individually listed nullable tooling fields first.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { ts, root } = require('./source.cjs');
const { schemas, shape, accepts } = require('./oracle.cjs');
const parse = relative => ts.createSourceFile(relative,
  fs.readFileSync(path.join(root, relative), 'utf8'), ts.ScriptTarget.Latest, true);
const fee = parse('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts');
const feeConfig = fee.statements.find(n => ts.isTypeAliasDeclaration(n) && n.name.text === 'AsterBuilderFeeConfig');
assert.ok(feeConfig);
schemas.AsterBuilderFeeConfig = shape(feeConfig.type);
const declaration = (ast, name) => {
  const node = ast.statements.flatMap(n => ts.isVariableStatement(n) ? [...n.declarationList.declarations] : [])
    .find(n => n.name.getText(ast) === name);
  assert.ok(node, name);
  return node;
};
const workflow = parse('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts');
const check = declaration(workflow, 'checkAsterBuilderApproved');
const approve = declaration(workflow, 'approveAsterBuilderFeeWithBrowserWallet');
assert.ok(ts.isTypeLiteralNode(check.initializer.parameters[0].type));
assert.ok(ts.isTypeLiteralNode(approve.initializer.parameters[0].type));
schemas.CheckParams = shape(check.initializer.parameters[0].type);
schemas.ApproveParams = shape(approve.initializer.parameters[0].type);
const maker = declaration(fee, 'isAsterMakerOnlyOrder');
const postOnly = maker.initializer.parameters[0].type.types.find(ts.isTypeLiteralNode);
assert.ok(postOnly);
const extra = shape(postOnly);
assert.deepEqual(extra.fields.map(f => f.name), ['postOnly']);
schemas.ExecutionOrder = { kind: 'object', fields: [...schemas.AsterDEXPlaceOrderOpts.fields, ...extra.fields] };
const selected = [
  ['types', 'AsterDEXMarketResponseFiltersItem'], ['types', 'AsterDEXMarketResponse'],
  ['types', 'AsterDEXPositionResponse'], ['types', 'AsterDEXOrderResponse'],
  ['types', 'AsterDEXTickerResponse'], ['types', 'AsterDEXKlineResponse'],
  ['types', 'AsterDEXWebSocketMessage'], ['types', 'AsterDEXPlaceOrderOpts'],
  ['order_execution', 'ExecutionOrder'], ['builder_approval', 'CheckParams'],
  ['builder_approval', 'ApproveParams'],
];
// These three tooling DTOs already collapse nullable native Option fields. This
// does not make null a permitted value for any ordinary wire-record field.
const nullableTooling = {
  CheckParams: ['apiKey', 'baseUrl', 'builderConfig', 'builderAddress', 'requiredFeeRate', 'testnet'],
  ApproveParams: ['baseUrl', 'builderName', 'testnet'],
  ExecutionOrder: ['postOnly'],
};
function bridge(owner, value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return value;
  const normalized = structuredClone(value);
  for (const field of nullableTooling[owner] ?? []) if (normalized[field] === null) delete normalized[field];
  return normalized;
}
function numericBits(value) {
  if (typeof value === 'number') {
    const bytes = Buffer.alloc(8);
    bytes.writeDoubleBE(value);
    return [bytes.toString('hex')];
  }
  if (Array.isArray(value)) return value.flatMap(numericBits);
  if (value && typeof value === 'object') return Object.values(value).flatMap(numericBits);
  return [];
}
function observedBits(value) {
  if (typeof value === 'string') return /^[0-9a-f]{16}$/.test(value) ? [value] : [];
  if (Array.isArray(value)) return value.flatMap(observedBits);
  if (value && typeof value === 'object') return Object.values(value).flatMap(observedBits);
  return [];
}
async function main() {
  assert.equal(process.argv.length, 2, 'decoder source gate takes no scope-changing arguments');
  const runtime = spawnSync(process.execPath, [path.join(__dirname, 'accepted-tooling-null-source.cjs')], {
    encoding: 'utf8', env: process.env,
  });
  assert.equal(runtime.status, 0, `actual TS null-use proof failed: ${runtime.error ?? ''}\n${runtime.stderr}`);
  assert.match(runtime.stdout, /10 individually reviewed fields \/ 150 actual TS null-versus-omission comparisons pass/);
  process.stdout.write(runtime.stdout);
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  let count = 0;
  for (const [module, name] of selected) {
    const owner = `safe_cex_venue_asterdex::${module}::${name}`;
    const contract = contracts[owner];
    const schema = schemas[name];
    assert.ok(contract?.nativeObservation && schema, owner);
    assert.ok(accepts(schema, contract.sample), `${name}: independent complete sample is source-valid`);
    if (schema.kind === 'object') {
      const fields = schema.fields.map(f => f.name);
      assert.deepEqual(Object.keys(contract.sample).filter(k => fields.includes(k)), fields,
        `${name}: complete sample covers actual source fields in declaration order`);
    }
    for (const row of contract.nativeObservation.cases) {
      const value = JSON.parse(row.input);
      if (row.label.startsWith('nullable-tooling-field-')) {
        assert.ok(nullableTooling[name]?.includes(row.label.slice('nullable-tooling-field-'.length)),
          `${name}: every native nullable Option is individually reviewed`);
      }
      assert.equal(accepts(schema, bridge(name, value)), row.output.ok,
        `${owner}: source schema with the documented tooling Option bridge, ${row.label}`);
      if (row.output.ok) {
        assert.deepEqual(numericBits(value).sort(), observedBits(row.output).sort(),
          `${owner}: independently expected native bits match actual Node JSON.parse numbers, ${row.label}`);
      }
      count++;
    }
  }
  assert.equal(selected.length, 11);
  assert.equal(count, 738);
  console.log(`Aster decoder source: 11 owners / ${count} independent observations pass (Node ${process.versions.node}, ${process.arch}).`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
