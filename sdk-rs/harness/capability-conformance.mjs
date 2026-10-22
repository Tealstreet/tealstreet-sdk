// Shared shipped-artifact conformance boundary. No transport is dispatched.
import assert from 'node:assert/strict';

export function requireCapabilityEngine(create, label) {
  let instance;
  let error;
  try { instance = create(); } catch (thrown) { error = thrown; }
  assert.doesNotMatch(String(error?.message ?? ''), /unsupported venue/, label);
  return instance;
}

export function probeCapabilityCommand(instance, command, label) {
  assert.doesNotThrow(() => instance.command(JSON.stringify(command), 0), label);
}
