import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

const telemetryTrait = 'safe_cex_types::telemetry::TelemetrySink';
const exchangeTrait = 'safe_cex_exchange::exchange::Exchange';
const reviewedExchangeMethods = ['find_order_by_client_order_id', 'listen_order_book', 'listen_ohlcv_retained', 'fetch_income_history_range', 'after_method_called_in_worker', 'fetch_historical_positions', 'fetch_positions', 'fetch_and_set_orders', 'fetch_orders', 'fetch_leverage_for_symbol', 'fetch_symbol_max_leverage', 'set_leverage', 'change_position_mode', 'change_margin_type', 'cancel_symbol_orders', 'fetch_public_trades', 'update_symbol_metadata'].sort();
export function defaultTraitInventory(doc, paths, normalize, { classifications = {}, readSource } = {}) {
  const owners = [];
  const methodIds = new Set();
  for (const item of Object.values(doc.index)) {
    if (item.crate_id !== 0 || !item.inner.trait) continue;
    const name = paths.get(String(item.id))?.join('::');
    const record = classifications[name];
    if (!record) continue;
    const defaults = item.inner.trait.items.map(id => doc.index[String(id)]).filter(method => method?.inner.function?.has_body);
    const methods = defaults.map(method => ({ name: method.name, signature: normalize(method.inner.function.sig), generics: normalize(method.inner.function.generics) })).sort((left, right) => left.name.localeCompare(right.name));
    assert.equal(item.span?.filename, record.file, `${name}: accepted-set assertion: reviewed default-trait source identity changed`);
    assert.equal(typeof readSource, 'function', `${name}: accepted-set assertion: production default source reader required`);
    reviewedDefaultTrait({ name, span: item.span, methods }, classifications, readSource(item.span.filename));
    const classificationOnly = record.category === 'reviewed-exchange-defaults';
    for (const method of defaults) if (!classificationOnly || reviewedExchangeMethods.includes(method.name)) methodIds.add(String(method.id));
    owners.push({ name, ident: item.name, span: item.span, kind: classificationOnly ? 'trait-classifications' : 'trait-defaults', deserialize: false, generics: item.inner.trait.generics, methods, conversions: [], traitMethods: [], ...(classificationOnly ? { classificationContract: { category: 'non-decoder', evidence: record.evidence, reason: record.reason, reviewedMethods: record.reviewedMethods } } : {}) });
  }
  return { owners, methodIds };
}

export function reviewedDefaultTrait(trait, classifications, source) {
  const contract = classifications[trait.name];
  if (!contract) return undefined;
  const label = `${trait.name}: accepted-set assertion`;
  if (trait.name === exchangeTrait) {
    assert.deepEqual(Object.keys(contract).sort(), ['category', 'declarationEnd', 'evidence', 'file', 'methods', 'reason', 'reviewedMethods', 'sourceSha256'], `${label}: unsupported default classification fields`);
    assert.equal(contract.category, 'reviewed-exchange-defaults', `${label}: unknown Exchange default classification`);
    assert.equal(trait.span?.filename, 'safe-cex-exchange/src/exchange.rs', `${label}: reviewed Exchange source identity changed`);
    assert.equal(contract.file, trait.span.filename, `${label}: reviewed Exchange source identity changed`);
    assert.deepEqual(trait.span.end, contract.declarationEnd, `${label}: reviewed Exchange declaration moved`);
    assert.deepEqual(trait.methods, contract.methods, `${label}: reviewed Exchange default API changed`);
    assert.deepEqual(contract.reviewedMethods?.map(method => method.name).sort(), reviewedExchangeMethods, `${label}: reviewed Exchange candidate set changed`);
    assert.ok(contract.reason?.trim() && contract.evidence?.trim() && contract.reviewedMethods.every(method => method.reason?.trim() && trait.methods.some(actual => actual.name === method.name)), `${label}: every reviewed Exchange candidate needs specific evidence`);
    assert.equal(contract.sourceSha256, createHash('sha256').update(source).digest('hex'), `${label}: reviewed Exchange default source changed`);
    return contract;
  }
  assert.equal(trait.name, telemetryTrait, `${label}: unknown default-trait classification`);
  assert.deepEqual(Object.keys(contract).sort(), ['category', 'declarationEnd', 'evidence', 'file', 'methods', 'reason', 'sourceSha256', 'witness'], `${label}: unsupported default-trait evidence fields`);
  assert.equal(contract.category, 'telemetry-noop-defaults', `${label}: unknown default-method category`);
  assert.ok(contract.reason?.trim() && contract.evidence?.trim(), `${label}: default methods need reviewed evidence`);
  assert.equal(trait.span.filename, contract.file, `${label}: reviewed default-trait source identity changed`);
  assert.deepEqual(trait.span.end, contract.declarationEnd, `${label}: reviewed default-trait declaration moved`);
  assert.deepEqual(trait.methods.map(method => method.name).sort(), ['capture_exception', 'event'], `${label}: default-method set changed`);
  assert.deepEqual(trait.methods, contract.methods, `${label}: default-method API changed`);
  assert.equal(contract.sourceSha256, createHash('sha256').update(source).digest('hex'), `${label}: reviewed default-method source changed`);
  assert.equal(contract.witness, 'inherited-telemetry-defaults-v1', `${label}: inherited-default native witness required`);
  return contract;
}

// This template always implements zero methods. Classification data cannot
// substitute overrides or an empty observer for inherited production calls.
export function telemetryDefaultWitness() {
  return `{
    struct EmptySink;
    impl super::TelemetrySink for EmptySink {}
    let payload = std::rc::Rc::new(std::cell::Cell::new(17_i32));
    let data: std::rc::Rc<dyn std::any::Any> = payload.clone();
    let error = std::rc::Rc::new(std::cell::Cell::new(73_i32));
    let opaque_error: std::rc::Rc<dyn std::any::Any> = error.clone();
    let context = std::rc::Rc::new(super::TelemetryContext { tags: Some(data.clone()), extra: None });
    let event = <EmptySink as super::TelemetrySink>::event(&EmptySink, value["name"].as_str().expect("event name"), data.clone());
    let capture = <EmptySink as super::TelemetrySink>::capture_exception(&EmptySink, opaque_error.clone(), context.clone());
    serde_json::json!({"eventOk": matches!(event, Ok(())), "captureOk": matches!(capture, Ok(())), "payload": payload.get(), "error": error.get(), "payloadOwners": std::rc::Rc::strong_count(&payload), "errorOwners": std::rc::Rc::strong_count(&error), "contextOwners": std::rc::Rc::strong_count(&context)})
  }`;
}

export function defaultWitnessContract() {
  const output = { eventOk: true, captureOk: true, payload: 17, error: 73, payloadOwners: 3, errorOwners: 2, contextOwners: 1 };
  return { category: 'inherited-defaults', cases: [{ input: { name: 'invalid-json' }, output }, { input: { name: '{"sample":7}' }, output }] };
}
