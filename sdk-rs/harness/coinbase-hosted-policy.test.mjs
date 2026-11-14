import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
const rules = JSON.parse(readFileSync(new URL('../../packages/safe-cex/src/exchanges/rust/construction-rules.json', import.meta.url)));
for (const venue of ['coinbase', 'coinbasespot']) {
  test(`${venue} admits RemoteSigner JWT custody and REST-only PayloadSigner`, () => {
    assert.equal(rules.venues[venue].bridge.dispatchSigning, true);
    assert.equal(rules.venues[venue].remoteSigning.enabled, true);
    assert.equal(rules.venues[venue].remoteSigning.payloadSupported, true);
    assert.equal(rules.venues[venue].remoteSigning.payloadRestOnly, true);
  });
}
