// Reproduce only the short-lived auth oracle; original 24h auth/order vectors remain intact.
// corepack yarn node sdk-rs/venues/paradex/tests/auth-short-vectors.cjs
const fs = require('fs');
const { createRequire } = require('module');
const req = createRequire(process.cwd() + '/package.json');
const { typedData, ec } = req('starknet');
const path = 'sdk-rs/venues/paradex/tests/sign-vectors.json';
const vectors = JSON.parse(fs.readFileSync(path, 'utf8'));
function sign(auth, address, key) {
  const raw = JSON.parse(auth.raw);
  raw.message.expiration = raw.message.timestamp + 300;
  const hash = typedData.getMessageHash(raw, address);
  const { r, s } = ec.starkCurve.sign(hash, key);
  return { raw: JSON.stringify(raw), hash, signature: JSON.stringify([r.toString(), s.toString()]) };
}
for (const row of vectors.cases) {
  row.shortAuth = sign(row.auth, row.address, row.key);
  row.ethereum.shortAuth = sign(row.ethereum.auth, row.ethereum.address, row.ethereum.derivedKey);
}
fs.writeFileSync(path, JSON.stringify(vectors, null, 2) + '\n');
