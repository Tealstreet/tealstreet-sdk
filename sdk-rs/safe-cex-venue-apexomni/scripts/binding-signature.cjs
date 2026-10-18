// Actual source placement + actual pinned zkLink SDK. Transport is recorded;
// signing, decimal math, nonce/slot derivation and request serialization are real.
const fs = require('node:fs');
const path = require('node:path');
const prelude = fs.readFileSync(path.join(__dirname, 'trading.cjs'), 'utf8');
const marker = 'async function main(){const rows=[];';
if (prelude.split(marker).length !== 2) throw Error('review trading oracle harness drift');
const main = `
return (async () => {
  globalThis.crypto = require('node:crypto').webcrypto;
  const {verifyPackage} = require('../../safe-cex-crypto/tests/fixtures/oracle-packages.cjs');
  const artifact = verifyPackage('zklink-web-sdk-alpha');
  const js = fs.readFileSync(path.join(artifact, 'zklink-sdk-web.js'));
  const wasm = fs.readFileSync(path.join(artifact, 'zklink-sdk-web_bg.wasm'));
  const sdk = await import('data:text/javascript;base64,' + js.toString('base64'));
  sdk.initSync(wasm);
  context.sdk = sdk;
  context.signContract = new vm.Script(signerCode + '\\nsignContract').runInNewContext({exports:{},Array:walletArray,sdk,BigNumber:dependency('@ethersproject/bignumber').BigNumber,isAddress:dependency('ethers').isAddress});
  context.initSdk = async () => {};
  clock = Number(process.env.SAFE_CEX_VIRTUAL_TIME ?? '1700000000000');
  const s = store();
  s.update({markets:[{...market,precision:{amount:0.001,price:0.01},limits:{amount:{min:0.001,max:1000},leverage:{min:1,max:50}}}]});
  s.updateZkLinkConfig({id:'16909060',l2Key:'fixture-l2',contractAccount:{makerFeeRate:'0.0012',takerFeeRate:'0.0013'},contractConfig:{perpetualContract:[{symbol:'BTC-USDC',l2PairId:'11'}]}});
  const exchange = new Class('apexomni',{key:'fixture-key',secret:'fixture-secret',passphrase:'fixture-passphrase',omniKeySeed:'0x'+'01'.repeat(64)+'1b'},s);
  operations=[];
  await exchange.placeOrder({symbol:'BTCUSDC',type:OrderType.Limit,side:OrderSide.Buy,amount:1.2345,price:100.127,clientOrderId:'tscaller-1'});
  const calls=operations.filter(row => row[0]==='request' && row[2]==='/v3/order');
  assert.equal(calls.length,1,'one actual source placement');
  console.log(JSON.stringify({body:calls[0][4]}));
})()`;
new Function('require', '__dirname', prelude.slice(0, prelude.indexOf(marker)) + main)(require, __dirname)
  .catch(error => {console.error(error); process.exitCode = 1;});
