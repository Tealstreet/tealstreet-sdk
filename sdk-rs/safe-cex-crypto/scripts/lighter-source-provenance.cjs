// Preserve original captures; admit only an explicitly reviewed source revision.
const assert = require('node:assert/strict');
const pins = require('../tests/fixtures/lighter/reviewed-source-provenance.json');
function originalProvenance(capture) {
  for (const [file, pin] of Object.entries(pins)) {
    assert.equal(capture.sources[file], pin.currentSha256, `unreviewed independent source drift: ${file}`);
    capture.sources[file] = pin.originalSha256;
  }
  return capture;
}
module.exports = { originalProvenance };
