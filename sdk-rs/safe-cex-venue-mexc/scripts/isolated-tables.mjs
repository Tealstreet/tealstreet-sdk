import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const mirror=fs.mkdtempSync(path.join(os.tmpdir(),'mexc-tables-source-'));
const files=[
  'packages/safe-cex/src/exchanges/mexc/mexc.types.ts',
  'packages/safe-cex/src/types.ts',
  'packages/safe-cex/src/exchanges/types.ts',
  'packages/logging/src/LogSeverity.ts',
  'sdk-rs/safe-cex-venue-mexc/tests/source.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/tables_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_ticker_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_private_order_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_private_fill_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_private_fill_missing_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_private_stop_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_private_position_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_private_filter_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/ws_private_ping_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/utils_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/mappers_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/alias_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/numeric_ingress_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/symbol_config_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/exchange_helpers_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/shields_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/errors_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/inherited_oracle.mjs',
  'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',
  'packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts',
  'packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts',
  'sdk-rs/safe-cex-venue-mexc/tests/api_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/factory_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/reconciliation_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/subscriber_throws_oracle.mjs',
  'sdk-rs/safe-cex-venue-mexc/tests/fetch_orders_oracle.mjs',
  'packages/safe-cex/src/store/store.base.ts',
  'packages/safe-cex/src/utils/clone.ts',
  'sdk-rs/safe-cex-venue-mexc/tests/string_boundary_oracle.mjs',
  'packages/safe-cex/src/remoteSigningAdapter.ts',
  'packages/safe-cex/src/utils/retryReadsOnly.ts',
  'packages/safe-cex/src/utils/signatureRejection.ts',
  'packages/json-parse/src/jsonParse.ts',
  'packages/safe-cex/src/exchanges/mexc/mexc.utils.ts',
  'packages/safe-cex/src/exchanges/mexc/mexc.api.ts',
  'packages/ccxt-compat/src/functions/ccxt_exchange.ts',
  'packages/safe-cex/src/utils/createPositionId.ts',
  'packages/safe-cex/src/utils/omitUndefined.ts',
  'packages/safe-cex/src/utils/orderbook.ts',
  'packages/safe-cex/src/utils/symbolConverter.ts',
];
try {
  for(let dir=mirror;;dir=path.dirname(dir)) {
    assert(!fs.existsSync(path.join(dir,'node_modules')),`unexpected node_modules ancestor: ${dir}`);
    if(dir===path.dirname(dir))break;
  }
  for(const file of files) {
    fs.mkdirSync(path.dirname(path.join(mirror,file)),{recursive:true});
    fs.copyFileSync(path.join(root,file),path.join(mirror,file));
  }
  let oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/tables_oracle.mjs');
  const run=env=>spawnSync(process.execPath,[oracle],{cwd:mirror,env,encoding:'utf8'});
  const result=run(process.env);
  assert.equal(result.status,0,result.stderr);
  assert.equal(Object.keys(JSON.parse(result.stdout).tables).length,21);
  const loader=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/source.mjs');
  const source=fs.readFileSync(loader,'utf8');
  fs.writeFileSync(loader,source.replace('require(process.env.SAFE_CEX_TYPESCRIPT_PATH)','require(process.env.SAFE_CEX_TYPESCRIPT_PATH + "/mexc-missing-import-control")'));
  const missingImport=run(process.env);
  assert.notEqual(missingImport.status,0);
  assert.match(missingImport.stderr,/MODULE_NOT_FOUND/);
  fs.writeFileSync(loader,source);
  const missingCompiler=run({...process.env,SAFE_CEX_TYPESCRIPT_PATH:''});
  assert.notEqual(missingCompiler.status,0);
  assert.match(missingCompiler.stderr,/SAFE_CEX_TYPESCRIPT_PATH must name the isolated compiler/);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/utils_oracle.mjs');
  const utils=run(process.env);assert.equal(utils.status,0,utils.stderr);assert.equal(JSON.parse(utils.stdout).length,595);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/mappers_oracle.mjs');
  const mappers=run({...process.env,TZ:'UTC'});assert.equal(mappers.status,0,mappers.stderr);assert.equal(JSON.parse(mappers.stdout).length,279);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/exchange_helpers_oracle.mjs');
  const helpers=run(process.env);assert.equal(helpers.status,0,helpers.stderr);assert.equal(JSON.parse(helpers.stdout).length,552);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/shields_oracle.mjs');
  const shields=run(process.env);assert.equal(shields.status,0,shields.stderr);assert.equal(JSON.parse(shields.stdout).length,16);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/errors_oracle.mjs');
  const errors=run(process.env);assert.equal(errors.status,0,errors.stderr);assert.equal(JSON.parse(errors.stdout).length,132);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/inherited_oracle.mjs');
  const inherited=run(process.env);assert.equal(inherited.status,0,inherited.stderr);assert.equal(JSON.parse(inherited.stdout).witnesses.length,52);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/api_oracle.mjs');
  const api=run(process.env);assert.equal(api.status,0,api.stderr);assert.equal(JSON.parse(api.stdout).length,186);
  const anchor='return runtimeRequire(specifier);';assert(source.includes(anchor));
  fs.writeFileSync(loader,source.replace(anchor,'return runtimeRequire(specifier + "-missing-control");'));
  const missingRuntime=run(process.env);assert.notEqual(missingRuntime.status,0);assert.match(missingRuntime.stderr,/MODULE_NOT_FOUND/);assert.match(missingRuntime.stderr,/create-hmac-missing-control/);
  fs.writeFileSync(loader,source);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/factory_oracle.mjs');
  const factory=run(process.env);assert.equal(factory.status,0,factory.stderr);assert.equal(JSON.parse(factory.stdout).length,71);
  for(const [flag,count] of [['--retry-body',3],['--response-numbers',4]]) {
    const result=spawnSync(process.execPath,[oracle,flag],{cwd:mirror,env:process.env,encoding:'utf8'});
    assert.equal(result.status,0,result.stderr);assert.equal(JSON.parse(result.stdout).length,count);
  }
  for(const [name,count] of [['reconciliation',112],['fetch_orders',240]]) {
    oracle=path.join(mirror,`sdk-rs/safe-cex-venue-mexc/tests/${name}_oracle.mjs`);
    const result=run(process.env);assert.equal(result.status,0,result.stderr);assert.equal(JSON.parse(result.stdout).length,count);
  }
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/subscriber_throws_oracle.mjs');
  const subscriberThrows=run(process.env);assert.equal(subscriberThrows.status,0,subscriberThrows.stderr);assert.equal(JSON.parse(subscriberThrows.stdout).length,2);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/alias_oracle.mjs');
  const aliases=run(process.env);assert.equal(aliases.status,0,aliases.stderr);assert.equal(JSON.parse(aliases.stdout).length,7);
  const prototype=spawnSync(process.execPath,[oracle,'--prototype'],{cwd:mirror,env:process.env,encoding:'utf8'});
  assert.equal(prototype.status,0,prototype.stderr);assert.equal(JSON.parse(prototype.stdout).inheritedFree,7);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/numeric_ingress_oracle.mjs');
  const ingress=run(process.env);assert.equal(ingress.status,0,ingress.stderr);assert.equal(JSON.parse(ingress.stdout).length,9);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/symbol_config_oracle.mjs');
  const configs=run(process.env);assert.equal(configs.status,0,configs.stderr);assert.equal(JSON.parse(configs.stdout).length,180);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/string_boundary_oracle.mjs');
  const strings=run(process.env);assert.equal(strings.status,0,strings.stderr);assert.equal(JSON.parse(strings.stdout).length,10);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_oracle.mjs');
  const ws=run(process.env);assert.equal(ws.status,0,ws.stderr);assert.equal(JSON.parse(ws.stdout).length,5);
  const wsMutation=run({...process.env,MEXC_WS_TS_CONTROL:'subscription-param'});
  assert.equal(wsMutation.status,0,wsMutation.stderr);assert.notDeepEqual(JSON.parse(wsMutation.stdout),JSON.parse(ws.stdout));
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_ticker_oracle.mjs');
  const wsTicker=run(process.env);assert.equal(wsTicker.status,0,wsTicker.stderr);
  const tickerMutation=run({...process.env,MEXC_WS_TS_CONTROL:'ticker-volume'});
  assert.equal(tickerMutation.status,0,tickerMutation.stderr);
  assert.notDeepEqual(JSON.parse(tickerMutation.stdout).fields,JSON.parse(wsTicker.stdout).fields);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_private_order_oracle.mjs');
  const wsPrivateOrder=run(process.env);assert.equal(wsPrivateOrder.status,0,wsPrivateOrder.stderr);
  assert.deepEqual(JSON.parse(wsPrivateOrder.stdout),{events:[['orders','123'],['status','123','canceled']],rejected:[['123',510]]});
  const wsPrivateOrderMutation=run({...process.env,MEXC_WS_PRIVATE_ORDER_CONTROL:'rejection-state'});
  assert.equal(wsPrivateOrderMutation.status,0,wsPrivateOrderMutation.stderr);
  assert.deepEqual(JSON.parse(wsPrivateOrderMutation.stdout).rejected,[]);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_private_fill_oracle.mjs');
  const wsPrivateFill=run(process.env);assert.equal(wsPrivateFill.status,0,wsPrivateFill.stderr);
  assert.equal(JSON.parse(wsPrivateFill.stdout).length,1);
  const wsPrivateFillMutation=run({...process.env,MEXC_WS_PRIVATE_FILL_CONTROL:'fee-value'});
  assert.equal(wsPrivateFillMutation.status,0,wsPrivateFillMutation.stderr);
  assert.notDeepEqual(JSON.parse(wsPrivateFillMutation.stdout),JSON.parse(wsPrivateFill.stdout));
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_private_fill_missing_oracle.mjs');
  const missingFill=run(process.env);assert.equal(missingFill.status,0,missingFill.stderr);
  assert.deepEqual(JSON.parse(missingFill.stdout).ownProperties,{orderId:true,fee:true,realizedPnl:true});
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_private_stop_oracle.mjs');
  const wsPrivateStop=run(process.env);assert.equal(wsPrivateStop.status,0,wsPrivateStop.stderr);
  assert.deepEqual(JSON.parse(wsPrivateStop.stdout),{seen:['plan-1','cancel-1','stop-1'],orders:['plan-1','stop-1']});
  const wsPrivateStopMutation=run({...process.env,MEXC_WS_PRIVATE_STOP_CONTROL:'disable-cancel-shield'});
  assert.equal(wsPrivateStopMutation.status,0,wsPrivateStopMutation.stderr);
  assert(JSON.parse(wsPrivateStopMutation.stdout).orders.includes('cancel-1'));
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_private_position_oracle.mjs');
  const wsPrivatePosition=run(process.env);assert.equal(wsPrivatePosition.status,0,wsPrivatePosition.stderr);
  assert.equal(Object.keys(JSON.parse(wsPrivatePosition.stdout).numericBits).length,9);
  const wsPrivatePositionMutation=run({...process.env,MEXC_WS_PRIVATE_POSITION_CONTROL:'entry-price'});
  assert.equal(wsPrivatePositionMutation.status,0,wsPrivatePositionMutation.stderr);
  assert.notDeepEqual(JSON.parse(wsPrivatePositionMutation.stdout).numericBits,JSON.parse(wsPrivatePosition.stdout).numericBits);
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_private_filter_oracle.mjs');
  const wsPrivateFilter=run(process.env);assert.equal(wsPrivateFilter.status,0,wsPrivateFilter.stderr);
  assert.equal(JSON.parse(wsPrivateFilter.stdout).param.filters.length,7);
  const wsPrivateFilterMutation=run({...process.env,MEXC_WS_PRIVATE_FILTER_CONTROL:'remove-asset'});
  assert.equal(wsPrivateFilterMutation.status,0,wsPrivateFilterMutation.stderr);
  assert.notDeepEqual(JSON.parse(wsPrivateFilterMutation.stdout),JSON.parse(wsPrivateFilter.stdout));
  oracle=path.join(mirror,'sdk-rs/safe-cex-venue-mexc/tests/ws_private_ping_oracle.mjs');
  const wsPrivatePing=run(process.env);assert.equal(wsPrivatePing.status,0,wsPrivatePing.stderr);
  assert.deepEqual(JSON.parse(wsPrivatePing.stdout),{method:'ping'});
  const wsPrivatePingMutation=run({...process.env,MEXC_WS_PRIVATE_PING_CONTROL:'wrong-method'});
  assert.equal(wsPrivatePingMutation.status,0,wsPrivatePingMutation.stderr);
  assert.deepEqual(JSON.parse(wsPrivatePingMutation.stdout),{method:'pong'});
  console.log('Tracked-source mirror without node_modules ancestors: 21 tables / 595 utils / 279 mappers / 524 exchange helpers / 304 shield steps / 132 rejection cases / 52 inherited lookups / 186 API / 78 factory / 112 reconciliation / 220 fetch orders / 180 symbol-config cases / 10 string-boundary cases / 5 WebSocket subscription keys / 11 ticker bits / private ping, filter, order, fill, stop and position source mutations / Store-write and rejection steps / plan-stop shields / position numeric bits pass; missing import/compiler controls fail loudly.');
} finally { fs.rmSync(mirror,{recursive:true,force:true}); }
