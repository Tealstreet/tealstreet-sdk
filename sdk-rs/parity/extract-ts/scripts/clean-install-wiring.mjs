import assert from 'node:assert/strict';

// Literal expectations are independent of the workflow under audit.
const proofStep = 'Check isolated parity extractor runtime install';
const command = 'node sdk-rs/parity/extract-ts/scripts/check-clean-install.mjs';
const requiredPaths = [
  'sdk-rs/**',
  // The dependency check moved into the slice (sdk-rs/parity/extract-ts/scripts/
  // check-dependency.mjs) so it runs on the public mirror too, and `sdk-rs/**`
  // already covers it. What replaces it here is the mirror's own workflow: the
  // wasm policy audit now validates whichever workflow declares the wasm job,
  // so an edit to the mirror's CI must trigger this one.
  'mirrors/tealstreet-sdk/.github/workflows/rust.yml',
  'packages/safe-cex-web/src/candleCache/**',
  'packages/safe-cex-server/src/candleCache/**',
  'packages/safe-cex-react-native/src/candleCache/**',
  'packages/json-parse/src/**',
];
export function auditExtractorInstallWiring(source) {
  const types = source.match(/^  types:\n([\s\S]*?)(?=^  [\w-]+:|$(?![\s\S]))/m)?.[1];
  assert.ok(types, 'extractor install wiring: missing types job');
  assert.doesNotMatch(types, /^    (?:if|continue-on-error):/m, 'extractor install wiring: types job cannot bypass proof');
  const blocks = [...types.matchAll(/^      - name: Check isolated parity extractor runtime install\n([\s\S]*?)(?=^      - |$(?![\s\S]))/gm)];
  assert.equal(blocks.length, 1, 'extractor install wiring: proof must run exactly once');
  assert.equal(blocks[0][1], `        working-directory: .\n        run: ${command}\n`, 'extractor install wiring: proof must retain exact mandatory root command');
  const proofAt = types.indexOf(`- name: ${proofStep}`);
  const installAt = types.indexOf('- name: Install dependencies for manifest coverage');
  const coverageAt = types.indexOf('- name: Check parity manifest coverage');
  assert.ok(proofAt >= 0 && proofAt < installAt && installAt < coverageAt, 'extractor install wiring: isolated proof must precede complete root install and semantic coverage');
  for (const event of ['push', 'pull_request']) {
    const block = source.match(new RegExp(`^  ${event}:\\n([\\s\\S]*?)(?=^  [\\w-]+:|^permissions:)`, 'm'))?.[1];
    assert.ok(block, `extractor install wiring: missing ${event}`);
    for (const name of requiredPaths) assert.ok(block.split('\n').includes(`      - '${name}'`), `extractor install wiring: ${event} missing ${name}`);
  }
}
