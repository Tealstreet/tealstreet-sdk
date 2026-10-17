const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {execFileSync} = require('node:child_process');
function certificates(dir) {
  const openssl = (...args) => execFileSync('openssl', args, {cwd: dir, stdio: 'pipe'});
  openssl('req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '30', '-subj', '/CN=Native network test CA', '-keyout', 'key.pem', '-out', 'root.pem');
  openssl('req', '-new', '-key', 'key.pem', '-subj', '/CN=localhost', '-out', 'request.pem');
  fs.writeFileSync(path.join(dir, 'index'), '');
  fs.writeFileSync(path.join(dir, 'serial'), '01\n');
  fs.writeFileSync(path.join(dir, 'ca.cnf'), `[ca]
default_ca=default
[default]
database=index
serial=serial
new_certs_dir=.
certificate=root.pem
private_key=key.pem
default_md=sha256
unique_subject=no
policy=policy
[policy]
commonName=supplied
[server]
basicConstraints=critical,CA:FALSE
subjectAltName=DNS:localhost,IP:127.0.0.1
extendedKeyUsage=serverAuth
`);
  openssl('ca', '-batch', '-config', 'ca.cnf', '-in', 'request.pem', '-out', 'expired.pem', '-startdate', '20000101000000Z', '-enddate', '20010101000000Z', '-extensions', 'server');
  openssl('ca', '-batch', '-config', 'ca.cnf', '-in', 'request.pem', '-out', 'valid.pem', '-startdate', '20200101000000Z', '-enddate', '20400101000000Z', '-extensions', 'server');
  for (const name of ['root', 'expired', 'valid']) openssl('x509', '-in', `${name}.pem`, '-outform', 'DER', '-out', `${name}.der`);
  openssl('pkcs8', '-topk8', '-nocrypt', '-in', 'key.pem', '-outform', 'DER', '-out', 'key.der');
}
if (process.argv[2] === '--certificates') {
  certificates(path.resolve(process.argv[3]));
} else {
  (async () => {
    const {createRequire} = require('node:module');
    assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated compiler required');
    const isolated = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
    const axios = isolated('axios');
    const {isNetworkError} = isolated('axios-retry');
    const ts = isolated('typescript');
    const vm = require('node:vm');
    const source = fs.readFileSync(path.resolve(__dirname, '../../../packages/safe-cex/src/utils/retryReadsOnly.ts'), 'utf8');
    const exports = {};
    vm.runInNewContext(ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS}}).outputText, {exports, require(name) { assert.equal(name, 'axios-retry'); return {isNetworkError}; }});
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'safe-cex-network-source-'));
    let server, validServer;
    try {
      certificates(dir);
      server = require('node:https').createServer({key: fs.readFileSync(path.join(dir, 'key.pem')), cert: fs.readFileSync(path.join(dir, 'expired.pem'))});
      await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
      validServer = require('node:https').createServer({key: fs.readFileSync(path.join(dir, 'key.pem')), cert: fs.readFileSync(path.join(dir, 'valid.pem'))});
      await new Promise(resolve => validServer.listen(0, '127.0.0.1', resolve));
      const cases = [
        ['ENOTFOUND', false, {url: 'http://missing.invalid/snapshot', lookup: (_host, _opts, callback) => callback(Object.assign(new Error('getaddrinfo ENOTFOUND missing.invalid'), {code: 'ENOTFOUND'}))}],
        ['CERT_HAS_EXPIRED', false, {url: `https://127.0.0.1:${server.address().port}/snapshot`, httpsAgent: new (require('node:https').Agent)({ca: fs.readFileSync(path.join(dir, 'root.pem'))})}],
        ['ERR_TLS_CERT_ALTNAME_INVALID', true, {url: `https://wrong.test:${validServer.address().port}/snapshot`, lookup: (_host, _opts, callback) => callback(null, '127.0.0.1', 4), httpsAgent: new (require('node:https').Agent)({ca: fs.readFileSync(path.join(dir, 'root.pem'))})}],
        ['UNABLE_TO_VERIFY_LEAF_SIGNATURE', false, {url: `https://127.0.0.1:${validServer.address().port}/snapshot`}],
        ...['EAI_AGAIN', 'ENETUNREACH'].map(code => [code, code === 'EAI_AGAIN', {url: 'http://missing.invalid/snapshot', lookup: (_host, _opts, callback) => callback(Object.assign(new Error(code), {code}))}]),
      ];
      for (const [code, retryable, config] of cases) {
        const error = await axios.get(config.url, {...config, proxy: false, timeout: 2000}).then(() => assert.fail('must reject'), error => error);
        assert.equal(error.code, code);
        assert.equal(exports.networkErrorOnRead()(error), retryable, `maintained retry refusal for ${code}`);
      }
      console.log('NATIVE-NETWORK source: 6 actual Axios failures, temporary DNS and hostname mismatch remain retryable');
    } finally {
      if (server) await new Promise(resolve => server.close(resolve));
      if (validServer) await new Promise(resolve => validServer.close(resolve));
      fs.rmSync(dir, {recursive: true});
    }
  })().catch(error => { console.error(error); process.exitCode = 1; });
}
