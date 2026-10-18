const assert = require('node:assert/strict');
const path = require('node:path');
const load = require('./load-module.cjs');
const { values } = require('./source.cjs');
const api = require('./api-source.cjs');

const compiler = path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)), '..');
const ethers = require(path.join(path.dirname(compiler), 'ethers'));
const fees = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts', [], {
  ethers, '../../types': { OrderType: {}, OrderTimeInForce: {} },
});
const approval = load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts',
  ['switchToAsterSignatureChain', 'normalizeAddress', 'buildAsterApprovalRequest'], {
    ethers, '../../transport/registry': { getTransport: () => ({ privateProxy: () => undefined }) },
    './asterdex.api': api, './asterdex.builder-fee': fees, './asterdex.types': values,
  });

function rejection(answer) {
  return answer.isError ? Object.assign(new Error(answer.reject.message), answer.reject) : answer.reject;
}

async function workflow(input, method) {
  const wallet = [...(input.wallet ?? [])], fetches = [...(input.fetch ?? [])];
  const walletMethods = [], fetchMethods = [];
  api.setOracleClock(input.now ?? 1_000_000_000_000);
  if (input.noWallet) delete global.window;
  else global.window = { ethereum: { request: async request => {
    walletMethods.push(request.method);
    assert.ok(wallet.length, 'planned source wallet request');
    const answer = wallet.shift();
    if (Object.hasOwn(answer, 'reject')) throw rejection(answer);
    return answer.value;
  } } };
  global.fetch = async (_url, options) => {
    fetchMethods.push(options.method ?? 'GET');
    assert.ok(fetches.length, 'planned source HTTP request');
    const answer = fetches.shift();
    if (Object.hasOwn(answer, 'reject')) throw rejection(answer);
    return {
      ok: answer.ok ?? true, status: answer.status ?? 200,
      json: async () => {
        if (answer.jsonError) throw new Error(answer.jsonError);
        return answer.payload;
      },
    };
  };
  try {
    const result = await approval[method](input.params);
    assert.equal(wallet.length, 0);
    assert.equal(fetches.length, 0);
    return { result, walletMethods, fetchMethods };
  } finally {
    delete global.window;
    delete global.fetch;
  }
}

(async () => {
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts();
  const prefix = 'safe_cex_venue_asterdex::';
  let comparisons = 0;
  for (const [name, method] of [
    ['builder_approval::switch_to_aster_signature_chain', 'switchToAsterSignatureChain'],
    ['builder_approval::check_aster_builder_approved', 'checkAsterBuilderApproved'],
    ['builder_approval::approve_aster_builder_fee_with_browser_wallet', 'approveAsterBuilderFeeWithBrowserWallet'],
  ]) {
    for (const row of contracts[prefix + name].cases) {
      assert.deepEqual(await workflow(row.input, method), row.output, name);
      comparisons++;
    }
  }
  for (const row of contracts[prefix + 'builder_approval::normalize_address'].cases) {
    let output;
    try { output = { value: approval.normalizeAddress(row.input.address, row.input.field) }; }
    catch (error) { output = { error: error.message }; }
    assert.deepEqual(output, row.output, 'maintained normalizeAddress');
    comparisons++;
  }
  for (const row of contracts[prefix + 'builder::build_aster_approval_request'].cases) {
    const result = await approval.buildAsterApprovalRequest(row.input.target, row.input.base ?? undefined);
    assert.deepEqual(result, row.output, 'maintained buildAsterApprovalRequest');
    comparisons++;
  }
  for (const row of contracts[prefix + 'builder_approval::form_component'].cases) {
    assert.equal(new URLSearchParams({ value: row.input }).toString().slice('value='.length), row.output);
    comparisons++;
  }
  assert.equal(comparisons, 17);
  process.stdout.write(`Aster builder/binding source: 6 roles / ${comparisons} independent expectations PASS.\n`);
})().catch(error => { console.error(error); process.exitCode = 1; });
