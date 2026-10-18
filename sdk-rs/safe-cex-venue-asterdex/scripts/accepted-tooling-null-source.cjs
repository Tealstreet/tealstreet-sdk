// Execute actual TS consumers: static optional syntax alone does not decide null.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const load = require('./load-module.cjs');
const { values, ts, root } = require('./source.cjs');
const compiler = path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)), '..');
const ethers = require(path.join(path.dirname(compiler), 'ethers'));
const shared = ts.createSourceFile('types.ts', fs.readFileSync(path.join(root,
  'packages/safe-cex/src/types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
const printer = ts.createPrinter();
const enumCode = shared.statements.filter(ts.isEnumDeclaration)
  .map(node => printer.printNode(ts.EmitHint.Unspecified, node, shared)).join('\n');
const enums = {};
new Function('exports', ts.transpileModule(enumCode, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText)(enums);
const { OrderType, OrderTimeInForce } = enums;
const fees = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts', [], {
  ethers, '../../types': { OrderType, OrderTimeInForce },
});
const builder = '0x0000000000000000000000000000000000000002';
const user = '0x0000000000000000000000000000000000000001';
const config = { builderAddress: builder, feeRateMaker: '0.0001', feeRateTaker: '0.0002' };
const fields = {
  CheckParams: ['apiKey', 'baseUrl', 'builderConfig', 'builderAddress', 'requiredFeeRate', 'testnet'],
  ApproveParams: ['baseUrl', 'builderName', 'testnet'],
  ExecutionOrder: ['postOnly'],
};
async function workflow(kind, params, fixture) {
  // Fresh actual API module resets its source nonce Map, so signatures, wallet
  // requests and fetch bytes are compared with identical inputs and clocks.
  delete require.cache[require.resolve('./api-source.cjs')];
  const api = require('./api-source.cjs');
  api.setOracleClock(1000000);
  const effects = [];
  const approval = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts', [], {
    ethers,
    '../../transport/registry': { getTransport(options) {
      effects.push(['getTransport', options]);
      return { privateProxy(exchange) {
        effects.push(['privateProxy', exchange]);
        return { url: 'https://proxy', async resolveHeaders() {
          effects.push(['resolveHeaders']); return { Authorization: 'proxy-token' };
        } };
      } };
    } },
    './asterdex.api': api, './asterdex.builder-fee': fees, './asterdex.types': values,
  });
  global.window = { ethereum: { async request(request) {
    effects.push(['wallet', request]);
    if (request.method === 'wallet_switchEthereumChain') return null;
    if (request.method === 'eth_requestAccounts') return [user];
    assert.equal(request.method, 'eth_signTypedData_v4');
    return '0xsignature';
  } } };
  global.fetch = async (url, options) => {
    effects.push(['fetch', url, { ...options, body: options.body === undefined ? null : String(options.body) }]);
    return { ok: true, status: 200, async json() {
      return kind === 'CheckParams' ? fixture : { code: 200 };
    } };
  };
  try {
    const result = await (kind === 'CheckParams'
      ? approval.checkAsterBuilderApproved(params)
      : approval.approveAsterBuilderFeeWithBrowserWallet(params));
    return { result, effects };
  } finally {
    delete global.window;
    delete global.fetch;
  }
}
async function main() {
  assert.equal(process.argv.length, 2);
  let comparisons = 0;
  // Approved and insufficient-cap rows force requiredFeeRate/config use after
  // signing and successful fetch; invalid address forces the source catch use.
  for (const field of fields.CheckParams) {
    for (const variant of ['approved', 'insufficient', 'invalid-address', 'config-only']) {
      const params = {
        apiKey: 'api-key', baseUrl: 'https://override', builderConfig: config,
        builderAddress: builder, requiredFeeRate: '0.0002', signerAddress: user,
        signerPrivateKey: `0x${'0'.repeat(63)}1`, testnet: true, userAddress: user,
      };
      delete params[field];
      // Exercise builderConfig's fee fallback as well as its address chain;
      // an explicit requiredFeeRate would otherwise short-circuit this use.
      if (field === 'builderConfig' && variant === 'approved') delete params.requiredFeeRate;
      if (variant === 'invalid-address') params.userAddress = 'invalid';
      if (variant === 'config-only' && field !== 'builderAddress') delete params.builderAddress;
      const rows = [{ builderAddress: builder, maxFeeRate: variant === 'insufficient' ? '0.00005' : '0.0003' }];
      const omitted = await workflow('CheckParams', params, rows);
      const explicit = await workflow('CheckParams', { ...params, [field]: null }, rows);
      assert.deepEqual(explicit, omitted, `CheckParams.${field}: ${variant} null equals omission, including effects`);
      const invalid = variant === 'invalid-address' || (variant === 'config-only' && field === 'builderConfig');
      assert.equal(omitted.result.approvalStatus,
        invalid ? 'unverified' : variant === 'insufficient' ? 'insufficient_fee_cap' : 'approved');
      const requests = omitted.effects.filter(effect => effect[0] === 'fetch');
      assert.equal(requests.length, invalid ? 0 : 1, `${field}: force the complete actual consumer path`);
      if (!invalid) assert.match(requests[0][1], /signature=0x[0-9a-f]{130}$/);
      comparisons++;
    }
  }
  for (const field of fields.ApproveParams) {
    for (const invalid of [false, true]) {
      const params = { baseUrl: 'https://override', builderAddress: builder, builderName: 'builder name',
        maxFeeRate: '0.0002', testnet: true, userAddress: invalid ? 'invalid' : user };
      delete params[field];
      const omitted = await workflow('ApproveParams', params);
      assert.deepEqual(await workflow('ApproveParams', { ...params, [field]: null }), omitted,
        `ApproveParams.${field}: null equals omission, including effects`);
      assert.equal(omitted.result.success, !invalid);
      assert.equal(omitted.effects.filter(effect => effect[0] === 'wallet').length, invalid ? 0 : 3);
      assert.equal(omitted.effects.filter(effect => effect[0] === 'fetch').length, invalid ? 0 : 1);
      comparisons++;
    }
  }
  for (const type of Object.values(OrderType)) {
    for (const timeInForce of [undefined, ...Object.values(OrderTimeInForce)]) {
      for (const apiType of ['LIMIT', 'STOP', 'TAKE_PROFIT', 'MARKET']) {
        const opts = { type, timeInForce };
        assert.equal(fees.isAsterMakerOnlyOrder({ ...opts, postOnly: null }, apiType),
          fees.isAsterMakerOnlyOrder(opts, apiType), 'ExecutionOrder.postOnly: actual maker predicate');
        assert.equal(fees.getAsterBuilderFeeRateForOrder({ ...opts, postOnly: null }, apiType, config),
          fees.getAsterBuilderFeeRateForOrder(opts, apiType, config), 'ExecutionOrder.postOnly: actual fee selection');
        comparisons++;
      }
    }
  }
  console.log(`Aster tooling null: 10 individually reviewed fields / ${comparisons} actual TS null-versus-omission comparisons pass.`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
