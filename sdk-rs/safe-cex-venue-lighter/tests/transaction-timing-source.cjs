'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../../../packages/lighter-sdk/src/client/transaction-client.ts'), 'utf8');
const compiled = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(compiled.diagnostics.length, 0);
const exportsObject = {};
new Function('exports', compiled.outputText)(exportsObject);
const methods = [
  ['CreateOrder', 'orderId'], ['CreateOrders', 'orderId'],
  ['CreateGroupedOrders', 'groupingType'], ['CancelOrder', 'orderId'],
  ['Transfer', 'toAccountIndex'], ['UpdateLeverage', 'marketId'],
  ['ApproveIntegrator', 'integratorAccountIndex'],
];
const flush = async () => { for (let i = 0; i < 100; i++) await Promise.resolve(); };
(async () => {
  if (process.argv.includes('--success')) {
    const observations = [];
    for (const [suffix] of methods) for (const prefix of ['get', 'submit']) {
      const method = prefix === 'submit' ? prefix + suffix : suffix === 'CreateOrders' ? 'getCreateOrderTransactions' : prefix + suffix + 'Transaction';
      let release, nonceCalls = 0;
      const sent = [];
      const hashes = ['A\ud800', 'B'];
      const api = {getNextNonce() {nonceCalls++; return new Promise(resolve => {release = resolve;});}, sendRawTx(tx) {sent.push(tx); return Promise.resolve('hash\ud800');}, sendRawTxBatch(txs) {sent.push(...txs); return Promise.resolve({txHashes: hashes});}};
      const crypto = {isReady: () => true, poseidon2Hash: () => '00', poseidon2HashTwoToOne: () => '00', schnorrSign: () => '00'};
      const client = new exportsObject.TransactionClient({httpClient: api, crypto, chainId: 304, privateKey: 'test', accountIndex: 42n, apiKeyIndex: 3});
      const order = {orderId: 1n, marketId: 1, isBuy: true, price: 100n, amount: 1n};
      const child = {...order, orderId: 2n, amount: 0n, isBuy: false, orderType: 'stop_loss', reduceOnly: true};
      const params = {...order, groupingType: 1, orders: [order, child], toAccountIndex: 2n, leverage: 5, marginMode: 0, integratorAccountIndex: 3n, maxPerpsTakerFee: 0, maxPerpsMakerFee: 0, maxSpotTakerFee: 0, maxSpotMakerFee: 0, approvalExpiry: 1710000000000n};
      const argument = suffix === 'CreateOrders' ? [params] : params;
      const pending = client[method](argument);
      params.price = 199n; order.price = 199n; params.orderId = 9n; params.amount = 9n; params.leverage = 10; params.maxPerpsTakerFee = 1;
      if (suffix === 'CreateOrders') argument.push({...params, orderId: 10n});
      await flush(); release(7n);
      const value = await pending;
      const txs = prefix === 'submit' ? sent : suffix === 'CreateOrders' ? value : [value];
      const tx = txs[0];
      const selected = suffix.startsWith('CreateGrouped') ? tx.groupedOrderDetails.orders[0].price : suffix.startsWith('CreateOrder') ? tx.orderDetails.price : suffix === 'CancelOrder' ? tx.cancelDetails.orderId : suffix === 'Transfer' ? tx.transferDetails.amount : suffix === 'UpdateLeverage' ? tx.leverageDetails.initialMarginFraction : tx.approveIntegratorDetails.maxPerpsTakerFee;
      assert.equal(nonceCalls, 1);
      assert.equal(txs.length, suffix === 'CreateOrders' ? 2 : 1);
      if (method === 'submitCreateOrders') assert.equal(value, hashes);
      observations.push({method, selected: String(selected), transactions: txs.length, nonceCalls, nonce: String(tx.nonce)});
    }
    assert.equal(observations.length, 14);
    process.stdout.write(JSON.stringify(observations));
    return;
  }
  const rows = [];
  for (const [suffix, field] of methods) for (const prefix of ['get', 'submit']) for (const rejectNonce of [false, true]) for (const explicitNonce of suffix === 'ApproveIntegrator' ? [false, true] : [false]) {
    if (explicitNonce && rejectNonce) continue;
    const method = prefix === 'submit' ? prefix + suffix : suffix === 'CreateOrders' ? 'getCreateOrderTransactions' : prefix + suffix + 'Transaction';
    let reads = 0, calls = 0, release, reject;
    const getterError = {}, nonceError = {};
    const order = {orderId: 1n, marketId: 1, isBuy: true, price: 100n, amount: 1n};
    const params = {...order, groupingType: 1, orders: [order, {...order, orderId: 2n, isBuy: false, amount: 0n, reduceOnly: true, orderType: 'stop_loss'}], toAccountIndex: 2n, leverage: 5, marginMode: 0, integratorAccountIndex: 3n, maxPerpsTakerFee: 0, maxPerpsMakerFee: 0, maxSpotTakerFee: 0, maxSpotMakerFee: 0, approvalExpiry: 1710000000000n};
    Object.defineProperty(params, field, {get() { reads++; throw getterError; }});
    const client = new exportsObject.TransactionClient({httpClient: {getNextNonce() { calls++; return new Promise((a, b) => {release = a; reject = b;}); }}, crypto: {}, chainId: 304, privateKey: 'test', accountIndex: 42n, apiKeyIndex: 3});
    if (explicitNonce) params.nonce = 7n;
    const result = client[method](suffix === 'CreateOrders' ? [params] : params).then(() => false, e => e === (rejectNonce ? nonceError : getterError));
    const before = reads;
    await flush();
    const held = reads;
    if (!explicitNonce) { if (rejectNonce) reject(nonceError); else release(7n); }
    const identity = await result;
    assert.equal(before, explicitNonce ? 1 : 0, method);
    assert.equal(held, explicitNonce ? 1 : 0, method);
    assert.equal(calls, explicitNonce ? 0 : 1, method);
    assert.equal(identity, true, method);
    assert.equal(reads > 0, !rejectNonce, method);
    rows.push({method, field, rejectNonce, explicitNonce, before, held, calls, identity, readAfter: reads > 0});
  }
  assert.equal(rows.length, 30);
  process.stdout.write(JSON.stringify(rows));
})().catch(e => {console.error(e); process.exitCode = 1;});
