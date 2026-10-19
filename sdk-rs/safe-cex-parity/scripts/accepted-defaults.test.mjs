import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { reviewedDefaultTrait, telemetryDefaultWitness, defaultWitnessContract, defaultTraitInventory } from './accepted-defaults.mjs';

const source = 'independent source fixture';
const trait = { name: 'safe_cex_types::telemetry::TelemetrySink', span: { filename: 'safe-cex-types/src/telemetry.rs', end: [27, 2] }, methods: [{ name: 'capture_exception', signature: { inputs: ['opaque'] } }, { name: 'event', signature: { inputs: ['name', 'opaque'] } }] };
const contract = { category: 'telemetry-noop-defaults', declarationEnd: [27, 2], evidence: 'Reviewed default bodies.', file: trait.span.filename, methods: trait.methods, reason: 'Opaque host callbacks with default no-op bodies.', sourceSha256: createHash('sha256').update(source).digest('hex'), witness: 'inherited-telemetry-defaults-v1' };
const classify = (item = trait, record = contract, text = source) => reviewedDefaultTrait(item, { [item.name]: record }, text);

test('reviewed no-op defaults pin their complete APIs, source and declaration', () => {
  assert.equal(classify(), contract);
  assert.throws(() => classify({ ...trait, methods: [...trait.methods, { name: 'decode' }] }), /default-method set changed/);
  assert.throws(() => classify({ ...trait, methods: [{ ...trait.methods[0], signature: { inputs: ['string'] } }, trait.methods[1]] }), /default-method API changed/);
  assert.throws(() => classify(trait, contract, `${source} decode_then_discard()`), /reviewed default-method source changed/);
  assert.throws(() => classify({ ...trait, span: { ...trait.span, filename: 'other.rs' } }), /source identity changed/);
  assert.equal(reviewedDefaultTrait({ ...trait, name: 'other::Trait' }, {}, source), undefined);
});
test('reviewed defaults cannot substitute arbitrary or missing native witnesses', () => {
  assert.throws(() => classify(trait, { ...contract, witness: '' }), /inherited-default native witness required/);
  assert.throws(() => classify(trait, { ...contract, call: 'serde_json::Value::Null' }), /unsupported default-trait evidence fields/);
  assert.throws(() => classify({ ...trait, name: 'other::Trait' }), /unknown default-trait classification/);
  const witness = telemetryDefaultWitness();
  assert.match(witness, /impl super::TelemetrySink for EmptySink \{\}/);
  for (const method of ['event', 'capture_exception']) assert.match(witness, new RegExp(`<EmptySink as super::TelemetrySink>::${method}`));
  const expected = defaultWitnessContract();
  assert.equal(expected.cases.length, 2);
  assert.deepEqual(expected.cases.map(row => row.input.name), ['invalid-json', '{"sample":7}']);
});

test('default inventory supplies every inherited body and never infers exemptions for unknown traits', () => {
  const doc = { index: { 1: { id: 1, crate_id: 0, name: 'TelemetrySink', span: trait.span, inner: { trait: { items: [2, 3], generics: { params: [] } } } } } };
  for (const [index, method] of trait.methods.entries()) doc.index[index + 2] = { id: index + 2, name: method.name, inner: { function: { has_body: true, sig: method.signature, generics: { params: [] } } } };
  const record = { ...contract, methods: trait.methods.map(method => ({ ...method, generics: { params: [] } })) };
  const paths = new Map([['1', trait.name.split('::')]]);
  const options = { classifications: { [trait.name]: record }, readSource: () => source };
  const found = defaultTraitInventory(doc, paths, value => value, options);
  assert.deepEqual([...found.methodIds], ['2', '3']);
  assert.deepEqual(found.owners.map(owner => owner.name), [trait.name]);
  assert.deepEqual(defaultTraitInventory(doc, paths, value => value).owners, []);
  doc.index[1].inner.trait.items.push(4);
  doc.index[4] = { id: 4, name: 'new_default', inner: { function: { has_body: true, sig: {}, generics: {} } } };
  assert.throws(() => defaultTraitInventory(doc, paths, value => value, options), /default-method set changed/);
});

test('Exchange default classifications cover only seventeen reviewed candidates and preserve source refusal', async () => {
  const fs = await import('node:fs'); const { spawnSync } = await import('node:child_process');
  const compiler = JSON.parse(fs.readFileSync(new URL('../accepted-set/exchange-defaults-fixture.json', import.meta.url)));
  const records = loadPolicy('default-traits');
  const text = fs.readFileSync(new URL('../../safe-cex-exchange/src/exchange.rs', import.meta.url));
  assert.equal(compiler.methods.length, 55, 'review snapshot includes all fifty-five defaults');
  assert.equal(records[compiler.name].reviewedMethods.length, 17, 'independent reviewed candidate floor');
  assert.equal(reviewedDefaultTrait(compiler, records, text), records[compiler.name]);
  assert.equal(records[compiler.name].reviewedMethods.some(method => ['sign_http_request', 'sign_remote_request'].includes(method.name)), false, 'HTTP signing decode path is outside this classification');
  for (const name of ['fetch_option_chain', 'fetch_option_expiries', 'fetch_account_greeks']) {
    assert.ok(compiler.methods.some(method => method.name === name), `${name} remains in the complete compiler API snapshot`);
    assert.equal(records[compiler.name].reviewedMethods.some(method => method.name === name), false, `${name} does not inherit a reviewed non-decoder classification`);
  }
  const expected = ['after_method_called_in_worker','cancel_symbol_orders','change_margin_type','change_position_mode','fetch_and_set_orders','fetch_historical_positions','fetch_income_history_range','fetch_leverage_for_symbol','fetch_orders','fetch_positions','fetch_public_trades','fetch_symbol_max_leverage','find_order_by_client_order_id','listen_ohlcv_retained','listen_order_book','set_leverage','update_symbol_metadata'];
  const doc = { index: { 1: { id: 1, crate_id: 0, name: 'Exchange', span: compiler.span, inner: { trait: { items: compiler.methods.map((_,i)=>i+2), generics: { params: [] } } } } } };
  compiler.methods.forEach((method,i)=>doc.index[i+2]={id:i+2,name:method.name,inner:{function:{has_body:true,sig:method.signature,generics:method.generics}}});
  const found = defaultTraitInventory(doc,new Map([['1',compiler.name.split('::')]]),value=>value,{classifications:records,readSource:()=>text});
  assert.deepEqual([...found.methodIds].map(id=>doc.index[id].name).sort(),expected,'only seventeen independently named defaults may be classified');
  assert.equal(found.owners[0].kind,'trait-classifications');
  assert.equal(found.owners[0].classificationContract.category,'non-decoder');

  for (const [label, mutation, assertion] of [
    ['Exchange default reviewed source changed', 'bytes=Buffer.concat([bytes,Buffer.from("\\nchanged body")]);', 'reviewed Exchange default source changed'],
    ['Exchange default reviewed candidate omitted', 'records[trait.name].reviewedMethods.pop();', 'reviewed Exchange candidate set changed'],
  ]) {
    const program = `import fs from 'node:fs';import {loadPolicy} from ${JSON.stringify(new URL('./accepted-policy-files.mjs', import.meta.url).href)};import {reviewedDefaultTrait} from ${JSON.stringify(new URL('./accepted-defaults.mjs', import.meta.url).href)};const trait=JSON.parse(fs.readFileSync(new URL(${JSON.stringify(new URL('../accepted-set/exchange-defaults-fixture.json', import.meta.url).href)})));const records=loadPolicy('default-traits');let bytes=fs.readFileSync(new URL(${JSON.stringify(new URL('../../safe-cex-exchange/src/exchange.rs', import.meta.url).href)}));${mutation}reviewedDefaultTrait(trait,records,bytes);`;
    const result = spawnSync(process.execPath, ['--input-type=module', '--eval', program], { encoding: 'utf8' });
    assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, new RegExp(assertion));
    console.log(`${label}: named assertion; process exit 1`);
    assert.equal(reviewedDefaultTrait(compiler, records, text), records[compiler.name]);
  }
  const signatureDrift = structuredClone(compiler);
  signatureDrift.methods.find(method => method.name === 'listen_ohlcv_retained').signature.output = null;
  assert.throws(() => reviewedDefaultTrait(signatureDrift, records, text), /reviewed Exchange default API changed/);
  const missingAdapter = structuredClone(records);
  missingAdapter[compiler.name].reviewedMethods = missingAdapter[compiler.name].reviewedMethods.filter(method => method.name !== 'listen_ohlcv_retained');
  assert.throws(() => reviewedDefaultTrait(compiler, missingAdapter, text), /reviewed Exchange candidate set changed/);
  const redirected = { ...compiler, name: 'other::Exchange' };
  assert.throws(() => reviewedDefaultTrait(redirected, { [redirected.name]: records[compiler.name] }, text), /unknown default-trait classification/);
});
