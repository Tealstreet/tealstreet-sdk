import assert from 'node:assert/strict';
import { test } from 'node:test';
import { observeCpu } from './cpu-observation.mjs';

test('records model, topology and observed frequency without machine identity', () => {
  const info = 'processor : 0\nmodel name : Example CPU\nphysical id : 0\ncore id : 0\ncpu MHz : 2400\n\nprocessor : 1\nmodel name : Example CPU\nphysical id : 0\ncore id : 0\ncpu MHz : 3200\n';
  const value = observeCpu({ cpuInfo: info, logical: [{ model: 'ignored', speed: 1 }, { model: 'ignored', speed: 1 }] });
  assert.deepEqual(value, { models: ['Example CPU'], logicalCpus: 2, physicalCores: 1, sockets: 1,
    frequencyMHz: { source: 'proc-cpuinfo', min: 2400, max: 3200 } });
  assert.doesNotMatch(JSON.stringify(value), /hostname|username|serial|url/i);
});

test('unavailable topology is explicit; nominal OS speed is not called observed frequency', () => {
  assert.deepEqual(observeCpu({ cpuInfo: '', logical: [{ model: 'Example', speed: 2400 }] }), {
    models: ['Example'], logicalCpus: 1, physicalCores: null, sockets: null,
    frequencyMHz: { source: 'os-nominal', min: 2400, max: 2400 },
  });
  assert.equal(observeCpu({ cpuInfo: '', logical: [] }).frequencyMHz, null);
});
