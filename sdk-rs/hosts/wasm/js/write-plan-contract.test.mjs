import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import ts from 'typescript';

const fixture = readFileSync(new URL('../../../engine/tests/fixtures/write-plan-error.json', import.meta.url), 'utf8');
const filename = fileURLToPath(new URL('write-plan-contract.mts', import.meta.url));

function diagnostics(value) {
  const options = {
    strict: true, noEmit: true, types: [],
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
  };
  const host = ts.createCompilerHost(options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (name, languageVersion, onError, shouldCreateNewSourceFile) =>
    name === filename
      ? ts.createSourceFile(name, `import type { VenueErrorDetail } from './host.mjs';
          const detail = ${value.trim()} satisfies VenueErrorDetail;`, languageVersion)
      : original(name, languageVersion, onError, shouldCreateNewSourceFile);
  const program = ts.createProgram([filename], options, host);
  return ts.getPreEmitDiagnostics(program);
}

test('serialized engine write-plan fixture satisfies the wasm declaration', () => {
  const errors = diagnostics(fixture);
  assert.deepEqual(errors.map((e) => ts.flattenDiagnosticMessageText(e.messageText, '\n')), []);
});

test('wasm declaration rejects an unknown row without its required error', () => {
  const errors = diagnostics(JSON.stringify({ message: 'stopped', elements: [{ index: 0, status: 'unknown' }] }));
  assert.equal(errors.length, 1);
  assert.match(ts.flattenDiagnosticMessageText(errors[0].messageText, '\n'), /error.*missing/);
});

test('Rust error detail retains indexed outcomes on the actual host error', async () => {
  const { ExchangeError } = await import('./host.mjs');
  const detail = JSON.parse(fixture);
  const error = new ExchangeError(detail.message, detail);
  assert.deepEqual(error.elements, detail.elements);
  assert.notEqual(error.elements, detail.elements);
  assert.notEqual(error.elements[0].orderIds, detail.elements[0].orderIds);
  assert.deepEqual(error.partialOrderIds, ['native-0']);
  assert.equal(error.totalOrderCount, 3);
  assert.equal(error.failedOrderIndex, 1);
});

test('malformed logical rows discard the array without leaking arbitrary fields', async () => {
  const { writePlanElementsOf } = await import('./host.mjs');
  for (const row of [
    null, [], { index: -1, status: 'notSent' }, { index: 0.5, status: 'notSent' },
    { index: Number.MAX_SAFE_INTEGER + 1, status: 'notSent' },
    { index: 0, clientOrderId: 7, status: 'notSent' },
    { index: 0, status: 'bogus' }, { index: 0, status: 'placed' },
    { index: 0, status: 'placed', orderIds: [7] }, { index: 0, status: 'unknown' },
    { index: 0, status: 'unknown', error: null },
    { index: 0, status: 'notSent', error: { message: 7 } },
  ]) assert.equal(writePlanElementsOf([{ index: 0, status: 'notSent' }, row]), undefined);
  assert.deepEqual(writePlanElementsOf([{ index: 0, status: 'notSent', request: 'excluded', error: {
    message: 'rejected', outcome: 'rejected', kind: 'rejected', venueCode: '-1', request: 'excluded',
    partial: { orderIds: ['native'], total: 2, failedIndex: 1, request: 'excluded' },
  } }]), [{ index: 0, status: 'unknown', error: {
    message: 'rejected', outcome: 'unknown', kind: 'rejected', venueCode: '-1',
    partial: { orderIds: ['native'], total: 2, failedIndex: 1 },
  } }]);
});

test('recursive outcome errors reject cycles and clone valid nested details', async () => {
  const { writePlanElementsOf } = await import('./host.mjs');
  const elements = [{ index: 0, status: 'unknown', error: { message: 'unknown' } }];
  elements[0].error.elements = elements;
  assert.equal(writePlanElementsOf(elements), undefined);
  const nested = [{ index: 0, status: 'unknown', error: { message: 'unknown', outcome: 'unknown', elements: [] } }];
  assert.deepEqual(writePlanElementsOf(nested), nested);
});


test('applied effects retain known targets without claiming created order IDs', async () => {
  const { writePlanElementsOf } = await import('./host.mjs');
  for (const effect of ['cancel', 'amend', 'tradingStop', 'settings', 'deferredChild', 'positionTrigger']) {
    const rows = [{ index: 0, status: 'applied', effect, targetOrderId: 'existing', positionKey: 'BTCUSD:long' }];
    assert.deepEqual(writePlanElementsOf(rows), rows);
    for (const invalid of [
      { ...rows[0], effect: 'bogus' }, { ...rows[0], targetOrderId: '' },
      { ...rows[0], positionKey: '' }, { ...rows[0], orderIds: ['invented'] },
      { ...rows[0], orderIds: [] }, { ...rows[0], orderId: 'invented' },
    ]) assert.equal(writePlanElementsOf([invalid]), undefined);
  }
});

test('placed identities must be nonempty dense arrays of nonempty strings', async () => {
  const { writePlanElementsOf } = await import('./host.mjs');
  for (const orderIds of [[], [''], Array(1), [7]]) {
    assert.equal(writePlanElementsOf([{ index: 0, status: 'placed', orderIds }]), undefined);
  }
});
