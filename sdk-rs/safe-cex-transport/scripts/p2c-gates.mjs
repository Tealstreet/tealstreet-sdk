import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const crate = 'sdk-rs/safe-cex-transport/';
const logs = fs.mkdtempSync(path.join(os.tmpdir(), 'p2c-gates-'));
const cargo = process.env.CARGO ?? 'cargo';
const rust = (...args) => [cargo, ['--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-transport', ...args]];
const command = (subcommand, ...args) => {
  const [bin, rest] = rust(...args);
  return [bin, [subcommand, ...rest]];
};
const yarn = (script) => {
  assert.ok(process.env.npm_execpath, 'Run this gate through yarn p2c:gates');
  return [process.execPath, [process.env.npm_execpath, script]];
};
function execute(name, [bin, args], expectFailure, pattern) {
  const result = spawnSync(bin, args, { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, env: { ...process.env, YARN_IGNORE_PATH: '1' } });
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  fs.writeFileSync(path.join(logs, `${name}.log`), output);
  assert.ifError(result.error);
  assert.notEqual(result.status, null, `${name} terminated by signal`);
  assert.equal(result.status === 0, !expectFailure, `${name}: unexpected exit ${result.status}\n${output}`);
  if (pattern) assert.match(output, pattern, `${name}: failed for the wrong reason`);
  console.log(`${name}: ${expectFailure ? 'negative control rejected' : 'passed'}`);
}
const retryManifest = 'sdk-rs/parity/manifest/utils/retryReadsOnly.ts.toml';
const definitions = [
  ['fmt', command('fmt', '--check'), crate + 'src/polling.rs', s => s + '\nfn format_probe( ) { }\n', /Diff in/],
  ['clippy', command('clippy', '--all-targets', '--', '-D', 'warnings'), crate + 'src/polling.rs', s => s + '\nfn unused_probe() {}\n', /unused_probe.*never used/s],
  ['tests', command('test', '--all-targets'), crate + 'tests/p2c.rs', s => s.replace('"venue already accepted the order"', '"venue already accepted the order"').replace('        1,\n        "venue already accepted the order"', '        2,\n        "venue already accepted the order"'), /lost_order_response_is_never_resent_but_a_read_is.*FAILED/s],
  ['custom-read', command('test', '--test', 'p2c', 'asterdex_suffix_classification_and_method_dependent_5xx_retry'), crate + 'src/retry.rs', s => s.replace('Self::Custom(is_read) => is_read(&request),', 'Self::Custom(_is_read) => false,'), /asterdex_suffix_classification_and_method_dependent_5xx_retry.*FAILED/s],
  ['request-condition', command('test', '--test', 'p2c', 'asterdex_suffix_classification_and_method_dependent_5xx_retry'), crate + 'src/retry.rs', s => s.replace('condition: Box::new(condition),', 'condition: Box::new(move |_, result| condition(&RestRequest { method: "get".into(), ..Default::default() }, result)),'), /asterdex_suffix_classification_and_method_dependent_5xx_retry.*FAILED/s],
  ['retry-delay', command('test', '--test', 'p2c', 'asterdex_linear_delay_precedes_each_retry_and_stops_at_budget'), crate + 'src/retry.rs', s => s.replace('(delay.milliseconds)(attempt + 1)', '(delay.milliseconds)(0)'), /asterdex_linear_delay_precedes_each_retry_and_stops_at_budget.*FAILED/s],
  ['utf8-body', command('test', '--test', 'p2c', 'non_utf8_signing_body_is_rejected_before_signer_or_venue'), crate + 'src/signing.rs', s => s.replace('request.body.map(String::from_utf8)', 'request.body.map(|bytes| Ok::<_, std::string::FromUtf8Error>(String::from_utf8_lossy(&bytes).into_owned()))'), /non_utf8_signing_body_is_rejected_before_signer_or_venue.*FAILED/s],
  ['limiter-independence', command('test', '--test', 'p2c', 'limiter_counts_failed_requests_and_secondary_limits_are_independent'), crate + 'tests/p2c.rs', s => s.replace('let secondary = RateLimitedAdapter::new(venue.clone(), clock.clone(), 2, 1000);', 'let secondary = &limited;'), /limiter_counts_failed_requests_and_secondary_limits_are_independent.*FAILED/s],
  ['signer-input-order', command('test', '--test', 'p2c', 'remote_signature_omissions_are_authoritative_and_transport_auth_survives'), crate + 'src/signing.rs', s => s.replace('return parsed;', 'return if let Value::Object(object) = parsed { serde_json::to_value(object.into_iter().collect::<BTreeMap<_, _>>()).unwrap() } else { parsed };'), /remote_signature_omissions_are_authoritative_and_transport_auth_survives.*FAILED/s],
  ['signed-order', command('test', '--test', 'p2c', 'browser_signed_body_and_params_keep_wire_insertion_order'), crate + 'Cargo.toml', s => s.replace('serde_json = { workspace = true, features = ["preserve_order"] }', 'serde_json.workspace = true'), /browser_signed_body_and_params_keep_wire_insertion_order.*FAILED/s],
  ['doc', [process.execPath, [crate + 'scripts/retry-type-gates.mjs']], crate + 'src/retry.rs', s => s.replace('/// fn adapter(policy: &RetryPolicy) -> &dyn RestAdapter { policy }', '/// fn adapter(policy: &RetryPolicy) -> &dyn RestAdapter { policy }\n/// let _: P2cUnrelatedMissingType;'), /invariant removal was masked by an unrelated error/],
  ['wasm', command('build', '--target', 'wasm32-unknown-unknown'), crate + 'src/lib.rs', s => s + '\n#[cfg(target_arch = "wasm32")]\ncompile_error!("P2c wasm negative control");\n', /P2c wasm negative control/],
  ['coverage', yarn('parity:extract:check'), retryManifest, s => s.replace(/\[\[symbol\]\]\nts = "networkErrorOnRead"[\s\S]*?(?=\[\[symbol\]\]|$)/, ''), /networkErrorOnRead.*(?:added|add)|(?:added|add).*networkErrorOnRead/s],
  ['invention', yarn('parity:check'), crate + 'src/polling.rs', s => s + '\npub fn p2c_invention_probe() {}\n', /invention:.*p2c_invention_probe/],
  ['status', yarn('parity:check'), retryManifest, s => s.replace(/note = "Read classification[^\n]*"/, 'note = ""'), /array rs requires.*note|split mapping requires.*note|note.*array/s],
  ['provenance', yarn('parity:check'), retryManifest, s => s.replace(/^rust_files = [^\n]*\n/m, ''), /Rust source file is not declared/],
  ['rename', yarn('parity:check'), retryManifest, s => s.replace(/ Adapted targets: [^"\n]*/g, ''), /a rename note must name this exact Rust target/],
  ['split-explanation', yarn('parity:check'), retryManifest, s => s.replace(/note = "Read classification[^\n]*"/, 'note = ' + JSON.stringify(JSON.parse(s.match(/ts = "IsReadRequest"[\s\S]*?rs = (\[[^\n]+\])/)[1]).map(target => '`' + target + '`').join(', '))), /array rs requires note explaining the split beyond Rust target tokens/],
  ['report', yarn('parity:report'), retryManifest, s => s.replace('::is_read_only_method', '::missing_read_method'), /unresolved rs.*missing_read_method|does not resolve.*missing_read_method/s],
  ['routing', [process.execPath, [crate + 'scripts/routing.mjs', '--check', '--equivalence']], crate + 'src/routing_generated.rs', s => s.replace('rest_proxy_host: "', 'rest_proxy_host: "negative-control.'), /generated routing is stale/],
];
console.log(`Logs: ${logs}`);
for (const [name, gate, relative, mutate, pattern] of definitions) {
  if (process.argv.includes('--only') && process.argv[process.argv.indexOf('--only') + 1] !== name) continue;
  execute(`${name}-green`, gate, false);
  const filename = path.join(root, relative);
  const original = fs.readFileSync(filename, 'utf8');
  const broken = mutate(original);
  assert.notEqual(broken, original, `${name}: mutation did not apply`);
  try {
    fs.writeFileSync(filename, broken);
    execute(`${name}-negative`, gate, true, pattern);
  } finally {
    fs.writeFileSync(filename, original);
  }
  execute(`${name}-restored`, gate, false);
}
