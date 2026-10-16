const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const external = require('node:module').createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const ts = external('typescript');
const bs58 = external('bs58').default;
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/orderly/orderly.encryption.ts');
const source = fs.readFileSync(file, 'utf8') + '\nexport { sha512ForEd25519 };\n';
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true } }).outputText;
const moduleUnderTest = { exports: {} };
new vm.Script(`(function(require,module,exports){${code}\n})`, { filename: file }).runInThisContext()(external, moduleUnderTest, moduleUnderTest.exports);
const actual = moduleUnderTest.exports;
const hashes = [0, 1, 31, 32, 63, 64, 128, 255].map(length => {
  const bytes = Array.from({ length }, (_, index) => (index * 37 + length) % 256);
  return { bytes, digest: Buffer.from(actual.sha512ForEd25519(Uint8Array.from(bytes))).toString('hex') };
});
const encodings = ['', '+', '/', '+/+//++', 'a+b/c==', 'a=b=+/', 'already-_', ' 😀+/\n'].map(input => ({ input, output: actual.base64url(input) }));
const seeds = [new Uint8Array(32), new Uint8Array(32).fill(255), Uint8Array.from({ length: 32 }, (_, i) => i)];
const signatures = [];
function capture(text, secret) {
  try { signatures.push({ text, secret, signature: actual.orderlySign(text, secret) }); }
  catch (error) { signatures.push({ text, secret, error: { name: error.name, message: error.message } }); }
}
for (const seed of seeds) for (const prefix of ['ed25519', 'unvalidated-prefix', '']) {
  for (const text of ['', '1700000000000GET/v1/orders?status=INCOMPLETE', '0POST/v1/order{"b":2,"a":1}', '🚀e\u0301\u0000日本語']) {
    capture(text, `${prefix}:${bs58.encode(seed)}`);
    capture(text, `${prefix}:${bs58.encode(seed)}:ignored-tail`);
  }
}
for (const secret of ['', 'ed25519', 'ed25519:', 'ed25519:0', 'ed25519:O', 'ed25519:abc ', 'ed25519::ignored',
  ...[1, 31, 33, 64].map(length => 'ed25519:' + bs58.encode(new Uint8Array(length)))]) capture('message', secret);
const surrogateText = '\ud800';
const surrogateSecret = 'ed25519:' + bs58.encode(seeds[2]);
const surrogate = { secret: surrogateSecret, base64Units: Array.from(actual.base64url('\ud800+/'), ch => ch.charCodeAt(0)), units: [surrogateText.charCodeAt(0)], replacementSignature: actual.orderlySign(surrogateText, surrogateSecret),
  explicitReplacementSignature: actual.orderlySign('\ufffd', surrogateSecret) };
assert.equal(surrogate.replacementSignature, surrogate.explicitReplacementSignature, 'source TextEncoder surrogate replacement');
const ed = external('@noble/ed25519');
const originalHash = ed.hashes.sha512;
const hook = { secret: surrogateSecret, normal: actual.orderlySign('hook', surrogateSecret) };
try {
  ed.hashes.sha512 = message => { const bytes = originalHash(message); bytes[0] ^= 0x80; return bytes; };
  hook.changed = actual.orderlySign('hook', surrogateSecret);
} finally { ed.hashes.sha512 = originalHash; }
assert.notEqual(hook.normal, hook.changed, 'source mutable hash hook');
assert.equal(actual.orderlySign('hook', surrogateSecret), hook.normal, 'source hash hook restored');
console.log(JSON.stringify({ runtime: { node: process.version, arch: process.arch }, hashes, encodings, signatures, surrogate, hook }));
