// Run the unchanged parity checker against the actual Coinbase sources and metadata.
// The temporary workspace narrows inventory scope; it does not alter source, rows, or exemptions.
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const {parse, stringify} = require('smol-toml');
const {spawnSync} = require('node:child_process');
const sdk = path.resolve(__dirname, '../..'), crate = 'safe-cex-venue-coinbase';
const scope = fs.mkdtempSync(path.join(os.tmpdir(), 'coinbase-parity-'));
try {
 const config = parse(fs.readFileSync(path.join(sdk, 'Cargo.toml'), 'utf8'));
 config.workspace.members = [crate];
 fs.writeFileSync(path.join(scope, 'Cargo.toml'), stringify(config));
 for (const entry of fs.readdirSync(sdk, {withFileTypes:true})) if (entry.isDirectory())
  fs.symlinkSync(path.join(sdk, entry.name), path.join(scope, entry.name), 'dir');
 const exemptions = parse(fs.readFileSync(path.join(sdk, 'parity/exempt.toml'), 'utf8'));
 const selected = {exempt:exemptions.exempt.filter(row => row.rs.startsWith(crate + '/'))};
 const exemptPath = path.join(scope, 'coinbase-exempt.toml');
 fs.writeFileSync(exemptPath, stringify(selected));
 const manifests = path.join(sdk, 'parity/manifest/exchanges/coinbase');
 const rows = fs.readdirSync(manifests).filter(file=>file.endsWith('.toml'))
  .flatMap(file=>parse(fs.readFileSync(path.join(manifests,file),'utf8')).symbol);
 if (rows.length !== 426) throw Error('Independent Coinbase inventory floor changed: expected 426');
 const binary = path.resolve(process.env.CARGO_TARGET_DIR ?? path.join(sdk, 'target'), 'debug/parity-check');
 const result = spawnSync(binary, ['--workspace',scope,'--manifest',manifests,'--exempt',exemptPath], {stdio:'inherit'});
 if (result.error) throw result.error;
 process.exitCode = result.status ?? 1;
} finally {fs.rmSync(scope, {recursive:true, force:true});}
