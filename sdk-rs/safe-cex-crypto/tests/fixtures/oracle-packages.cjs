// Pin the actual published package files, not merely a version label or entrypoint.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const { createRequire } = require('node:module');
const fixture = path.join(__dirname, 'oracle-packages.json');
const oracleRequire = process.env.SAFE_CEX_CRYPTO_ORACLE_DEPS
  ? createRequire(path.join(process.env.SAFE_CEX_CRYPTO_ORACLE_DEPS, 'package.json')) : require;
function packageRoot(name) {
  let directory = path.dirname(oracleRequire.resolve(name));
  for (;;) {
    const manifest = path.join(directory, 'package.json');
    if (fs.existsSync(manifest) && JSON.parse(fs.readFileSync(manifest, 'utf8')).name === name) return directory;
    const parent = path.dirname(directory); assert.notEqual(parent, directory, `package root missing: ${name}`); directory = parent;
  }
}
function fingerprint(name) {
  const root = packageRoot(name), files = [];
  function walk(directory) {
    for (const entry of fs.readdirSync(directory, {withFileTypes:true})) {
      if (entry.name === 'node_modules' || entry.name.startsWith('.yarn-')) continue;
      const filename = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(filename);
      else if (entry.isFile()) files.push(path.relative(root, filename).split(path.sep).join('/'));
      else throw Error(`unexpected oracle package entry: ${filename}`);
    }
  }
  walk(root); files.sort(); const hash = createHash('sha256');
  for (const filename of files) hash.update(filename).update('\0').update(fs.readFileSync(path.join(root, filename)));
  const manifest = JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8'));
  return {version:manifest.version, files:files.length, sha256:hash.digest('hex')};
}
function verifyPackage(name) {
  const expected = JSON.parse(fs.readFileSync(fixture,'utf8'))[name];
  assert.ok(expected, `missing oracle package pin: ${name}`);
  assert.deepEqual(fingerprint(name), expected, `oracle package files changed: ${name}`);
  return packageRoot(name);
}
module.exports = { oracleRequire, packageRoot, verifyPackage };
if (require.main === module) {
  const result = JSON.stringify(Object.fromEntries(['ethers','@msgpack/msgpack','starknet','zklink-web-sdk-alpha'].map(name=>[name,fingerprint(name)])),null,2)+'\n';
  if (process.argv.includes('--capture')) fs.writeFileSync(fixture,result,{flag:'wx'});
  else assert.equal(result,fs.readFileSync(fixture,'utf8'),'oracle package artifact drift');
}
