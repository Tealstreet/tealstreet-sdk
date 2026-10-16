'use strict';
const { ts, fs, path, vm, root, assert, load } = require('./oracle-loader.cjs');
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const constants = load(path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts'));
const positionId = load(path.join(root, 'packages/safe-cex/src/utils/createPositionId.ts'));
const source = path.join(root, 'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast = ts.createSourceFile(source, fs.readFileSync(source, 'utf8'), ts.ScriptTarget.Latest, true);
const classes = ast.statements.filter(node => ts.isClassDeclaration(node) && node.name?.text === 'BitunixExchange');
assert.equal(classes.length, 1, 'BITUNIX public unique owner');
const control = process.env.SAFE_CEX_BLOCK_SOURCE_CONTROL;
assert([undefined, 'candidate-data', 'first-value'].includes(control), 'BITUNIX public known control');
function method(name, owner) {
  const members = classes[0].members.filter(node => node.name?.getText(ast) === name);
  assert.equal(members.length, 1, 'BITUNIX public unique method ' + name);
  let initializer = members[0].initializer.getText(ast);
  if (control === 'candidate-data' && name === 'getResponseCandidates') {
    const selector = 'visit(value.data, depth + 1);';
    assert.equal(initializer.split(selector).length, 2, 'BITUNIX public candidate selector');
    initializer = initializer.replace(selector, 'void 0;');
  }
  if (control === 'first-value' && name === 'getFirstValue') {
    const selector = 'return value;';
    assert.equal(initializer.split(selector).length, 2, 'BITUNIX public value selector');
    initializer = initializer.replace(selector, 'return typeof value === "string" ? value.replace(/\\ud800/g, "\\ufffd") : value;');
  }
  const code = ts.transpileModule(`(function(){return (${initializer});})`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText;
  return vm.runInNewContext(code, {
    ...enums, ...constants, ...positionId, console,
    uniqBy: require(require.resolve('lodash/uniqBy', { paths: [root] })),
  }).call(owner);
}
async function main() {
  const lone = String.fromCharCode(0xd800);
  const raw = { symbol: lone, qty: '2', side: 'BUY', positionMode: 'ONE_WAY' };
  const response = { data: { data: [raw] } };
  const observed = { candidates: 0, selected: 0, first: 0, requests: 0 };
  const owner = {
    xhr: { get: async () => { observed.requests++; return response; } },
    store: { markets: [], positions: [] },
    symbolSettings: new Map(),
    accountPositionMode: enums.PositionMode.OneWay,
  };
  for (const name of ['isRecord', 'getResponseCandidates', 'getFirstValue', 'parseFiniteNumber', 'extractPositionRows']) {
    owner[name] = method(name, owner);
  }
  const candidates = owner.getResponseCandidates;
  owner.getResponseCandidates = payload => {
    const result = candidates(payload);
    assert(result.some(value => Array.isArray(value) && value[0] === raw), 'BITUNIX public candidate row');
    observed.candidates++;
    return result;
  };
  const select = owner.extractPositionRows;
  owner.extractPositionRows = payload => {
    const result = select(payload);
    assert.equal(result[0], raw, 'BITUNIX public selected identity');
    observed.selected++;
    return result;
  };
  const first = owner.getFirstValue;
  owner.getFirstValue = (row, fields) => {
    const result = first(row, fields);
    if (fields[0] === 'symbol') {
      assert.equal(row, raw, 'BITUNIX actual caller row');
      assert.equal(fields.join(','), 'symbol,instId,contract,instrument', 'BITUNIX actual caller fields');
      assert.equal(result.charCodeAt(0), 0xd800, 'BITUNIX actual first-value unit');
      observed.first++;
    }
    return result;
  };
  const positions = await method('fetchPositions', owner)();
  assert.equal(positions.length, 1, 'BITUNIX public position count');
  assert.equal(positions[0].symbol.length, 1, 'BITUNIX public symbol length');
  assert.equal(positions[0].symbol.charCodeAt(0), 0xd800, 'BITUNIX public symbol unit');
  assert.equal(positions[0].id.charCodeAt(0), 0xd800, 'BITUNIX public id unit');
  assert.equal(positions[0].contracts, 2, 'BITUNIX public decoded quantity');
  assert.deepEqual(observed, { candidates: 1, selected: 1, first: 2, requests: 1 }, 'BITUNIX independent consumer trace');
  process.stdout.write(JSON.stringify({ symbolUnits: [positions[0].symbol.charCodeAt(0)], idUnits: [positions[0].id.charCodeAt(0)], contracts: positions[0].contracts, observed }) + '\n');
}
main().catch(error => { process.stderr.write(error.stack + '\n'); process.exitCode = 1; });
