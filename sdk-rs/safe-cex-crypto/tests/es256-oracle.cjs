// Live Coinbase TypeScript/WebCrypto oracle. The only private key is RFC 6979
// A.2.5's public test vector, reconstructed as a JWK; no credentials or fixtures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createPrivateKey, createPublicKey, webcrypto } = require('node:crypto');

const scalar = Buffer.from([
  0xc9, 0xaf, 0xa9, 0xd8, 0x45, 0xba, 0x75, 0x16,
  0x6b, 0x5c, 0x21, 0x57, 0x67, 0xb1, 0xd6, 0x93,
  0x4e, 0x50, 0xc3, 0xdb, 0x36, 0xe8, 0x9b, 0x12,
  0x7b, 0x8a, 0x62, 0x2b, 0x12, 0x0f, 0x67, 0x21,
]);
const x = Buffer.from('60fed4ba255a9d31c961eb74c6356d68c049b8923b61fa6ce669622e60f29fb6', 'hex');
const y = Buffer.from('7903fe1008b8bc99a41ae9e95628bc64f2f1b20c2d7e9f5177a3c294d4462299', 'hex');
const key = createPrivateKey({ format: 'jwk', key: {
  kty: 'EC', crv: 'P-256', d: scalar.toString('base64url'),
  x: x.toString('base64url'), y: y.toString('base64url'),
} });
const publicKey = createPublicKey(key);
const publicSec1 = Buffer.concat([Buffer.from([4]), x, y]);

async function main() {
  const raw = fs.readFileSync(0, 'utf8');
  const input = raw ? JSON.parse(raw) : {};
  const verifier = await webcrypto.subtle.importKey('spki', publicKey.export({ format: 'der', type: 'spki' }),
    { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify']);
  if (input.verify) {
    const results = [];
    for (const { payload, signature } of input.verify) {
      const bytes = Uint8Array.from(payload);
      const sig = Uint8Array.from(signature);
      results.push({
        valid: await webcrypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, verifier, sig, bytes),
        changed: await webcrypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, verifier, sig,
          Uint8Array.from([...bytes, 32])),
      });
    }
    process.stdout.write(JSON.stringify(results));
    return;
  }

  const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
  const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const names = new Set([
    'COINBASE_KEY_NAME_PATTERN', 'COINBASE_PRIVATE_KEY_PATTERN', 'COINBASE_ED25519_PREFIX',
    'getCoinbaseJsonSecret', 'parseCoinbaseCredentialJson', 'base64ToBytes', 'normalizeCoinbaseApiCredentials',
    'cachedCryptoKey', 'cachedKeySecret', 'base64UrlEncode', 'base64UrlEncodeString',
    'asn1Wrap', 'pemToPkcs8Der', 'getSigningKey', 'sign',
  ]);
  const selected = source.statements.filter(n =>
    (ts.isFunctionDeclaration(n) && names.has(n.name?.text)) ||
    (ts.isVariableStatement(n) && n.declarationList.declarations.some(d => names.has(d.name.getText(source)))));
  assert.equal(selected.length, names.size, 'complete live Coinbase signing dependency closure');
  const exports = {};
  vm.runInNewContext(ts.transpileModule(selected.map(n => n.getText(source)).join('\n') +
    '\nexport { sign, pemToPkcs8Der };', { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText,
  { exports, crypto: webcrypto, TextEncoder, Uint8Array, ArrayBuffer, atob, btoa,
    uuid: () => 'fixture-nonce', virtualClock: { getCurrentTimeMs: () => 1700000000123 },
    ed25519Sign: () => { throw Error('ES256 oracle must not select Ed25519'); } });

  const pkcs8 = key.export({ format: 'der', type: 'pkcs8' });
  const zeroScalar = Buffer.from(pkcs8);
  const scalarOffset = zeroScalar.indexOf(scalar);
  assert(scalarOffset >= 0, 'RFC scalar is present in PKCS8');
  zeroScalar.fill(0, scalarOffset, scalarOffset + scalar.length);
  const invalidImports = [pkcs8.subarray(0, -1), publicKey.export({ format: 'der', type: 'spki' }), zeroScalar];
  for (const invalid of invalidImports) {
    await assert.rejects(webcrypto.subtle.importKey('pkcs8', invalid,
      { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign']), 'real WebCrypto rejects malformed imports');
  }
  const imported = await webcrypto.subtle.importKey('pkcs8', pkcs8,
    { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign']);
  const cases = [];
  for (const payload of ['sample', '', '雪\u0000💰', '{"zeta":1e400,"alpha":-0,"nonce":"7"}']) {
    const bytes = Buffer.from(payload);
    const signature = Buffer.from(await webcrypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, imported, bytes));
    assert.equal(signature.length, 64, 'WebCrypto uses fixed-width r||s');
    cases.push({ label: `raw-${cases.length}`, pkcs8: [...pkcs8], payload: [...bytes], signature: [...signature] });
  }
  for (const type of ['pkcs8', 'sec1']) {
    const pem = key.export({ format: 'pem', type }).toString();
    const der = Buffer.from(exports.pemToPkcs8Der(pem));
    for (const uri of [undefined, 'GET api.coinbase.com/api/v3/brokerage/portfolios', 'POST api.coinbase.com/api/v3/brokerage/orders']) {
      const jwt = await exports.sign('organizations/fixture/apiKeys/fixture', pem, uri);
      const parts = jwt.split('.');
      assert.equal(parts.length, 3);
      const header = JSON.parse(Buffer.from(parts[0], 'base64url'));
      const claims = JSON.parse(Buffer.from(parts[1], 'base64url'));
      assert.deepEqual(header, { alg: 'ES256', typ: 'JWT', kid: 'organizations/fixture/apiKeys/fixture', nonce: 'fixture-nonce' });
      assert.deepEqual(claims, { iss: 'coinbase-cloud', nbf: 1700000000, exp: 1700000120,
        sub: 'organizations/fixture/apiKeys/fixture', ...(uri ? { uri } : {}) });
      const payload = Buffer.from(`${parts[0]}.${parts[1]}`);
      const signature = Buffer.from(parts[2], 'base64url');
      assert.equal(signature.length, 64);
      assert(await webcrypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, verifier, signature, payload));
      cases.push({ label: `${type}-${uri?.split(' ')[0] ?? 'ws'}`, pkcs8: [...der], payload: [...payload], signature: [...signature] });
    }
  }
  assert.equal(cases.length, 10);
  process.stdout.write(JSON.stringify({ publicKey: [...publicSec1], cases, invalidImports: invalidImports.map(value => [...value]) }));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
