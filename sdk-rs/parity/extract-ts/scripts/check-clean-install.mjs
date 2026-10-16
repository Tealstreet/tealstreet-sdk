import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const providerNames = ['typescript', 'tsx', 'smol-toml'];
const extractorRelative = 'sdk-rs/parity/extract-ts';
const guardRelative = 'sdk-rs/parity/extract-ts/scripts/check-dependency.mjs';
const fixtureText = 'export function installedControl(value: number): number { return value + 7; }\n';

function contained(root, target) {
  const relative = path.relative(fs.realpathSync(root), fs.realpathSync(target));
  assert.ok(relative && relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative), 'extractor isolation: dependency escaped install root');
  return relative.split(path.sep).join('/');
}
function cleanEnvironment() {
  const env = {...process.env, TSX_DISABLE_CACHE: '1', YARN_IGNORE_PATH: '1'};
  for (const name of ['NODE_PATH', 'NODE_OPTIONS', 'SAFE_CEX_TYPESCRIPT_PATH', 'TYPESCRIPT_PATH', 'TSX_TSCONFIG_PATH', 'ESBUILD_BINARY_PATH']) delete env[name];
  env.NODE_OPTIONS = '--no-global-search-paths';
  env.PATH = path.dirname(process.execPath) + path.delimiter + (env.PATH ?? '');
  return env;
}
function execute(command, args, cwd, env) {
  const result = spawnSync(command, args, {cwd, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
  assert.ok(!result.error, String(result.error));
  assert.equal(result.signal, null, 'extractor proof child must exit normally');
  assert.ok(Number.isInteger(result.status), 'extractor proof child needs an actual numeric exit');
  return {...result, output: `${result.stdout}\n${result.stderr}`};
}
function copy(root, isolate, relative) {
  const source = path.join(root, relative);
  assert.ok(!fs.lstatSync(source).isSymbolicLink(), 'extractor isolation: tracked source must be copied without links');
  const target = path.join(isolate, relative);
  fs.mkdirSync(path.dirname(target), {recursive: true});
  fs.copyFileSync(source, target);
}
function lockedVersion(lock, name, selector) {
  const expected = `${name}@${selector}`;
  const entries = [...lock.matchAll(/^([^ \t#][^\n]*):\r?\n((?:[ \t]+[^\n]*\r?\n|\r?\n)*)/gm)]
    .filter(match => match[1].split(/,\s*/).some(part => part.replace(/^"|"$/g, '') === expected));
  assert.equal(entries.length, 1, `extractor dependency: unique maintained lock entry required for ${name}`);
  const value = entries[0][2].match(/^  version "([^"]+)"$/m)?.[1];
  assert.ok(value, `extractor dependency: maintained lock version required for ${name}`);
  return value;
}

export function proveExtractorInstall(root) {
  root = fs.realpathSync(root);
  const env = cleanEnvironment();
  const receipts = [];
  const record = (label, result) => { receipts.push({label, exit: result.status}); return result; };
  const rootPackage = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  const extractorPackage = JSON.parse(fs.readFileSync(path.join(root, extractorRelative, 'package.json'), 'utf8'));
  const lock = fs.readFileSync(path.join(root, 'yarn.lock'), 'utf8');
  const selectors = Object.fromEntries(providerNames.map(name => {
    const value = extractorPackage.dependencies?.[name] ?? extractorPackage.devDependencies?.[name];
    assert.ok(typeof value === 'string' && value, `extractor dependency: maintained selector required for ${name}`);
    return [name, value];
  }));
  const versions = Object.fromEntries(providerNames.map(name => [name, lockedVersion(lock, name, selectors[name])]));
  const yarn = (cwd, args) => execute('corepack', ['yarn', ...args], cwd, env);
  const yarnVersion = record('maintained Yarn version', yarn(root, ['--version']));
  assert.equal(yarnVersion.status, 0, yarnVersion.output);
  assert.equal(`yarn@${yarnVersion.stdout.trim()}`, rootPackage.packageManager, 'extractor isolation: use maintained Yarn Classic version');
  const isolate = fs.realpathSync(fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'safe-cex-extractor-install-')));
  try {
    for (let ancestor = path.dirname(isolate); ; ancestor = path.dirname(ancestor)) {
      assert.ok(!fs.existsSync(path.join(ancestor, 'node_modules')), 'extractor isolation: ancestor node_modules would invalidate control');
      if (path.dirname(ancestor) === ancestor) break;
    }
    fs.writeFileSync(path.join(isolate, 'package.json'), JSON.stringify({private: true, packageManager: rootPackage.packageManager, dependencies: selectors}, null, 2) + '\n');
    fs.writeFileSync(path.join(isolate, 'yarn.lock'), lock);
    const install = record('frozen runtime install', yarn(isolate, ['install', '--frozen-lockfile', '--ignore-scripts', '--non-interactive', '--modules-folder', path.join(isolate, 'node_modules')]));
    assert.equal(install.status, 0, install.output);
    assert.equal(fs.readFileSync(path.join(isolate, 'yarn.lock'), 'utf8'), lock, 'frozen install must preserve copied maintained lock bytes');

    // Install only the maintained runtime closure; restore actual root and tool
    // metadata before running the maintained commands. This is not a full Yarn
    // workspace install and does not claim unrelated development tools exist.
    copy(root, isolate, 'package.json');
    copy(root, isolate, guardRelative);
    copy(root, isolate, `${extractorRelative}/package.json`);
    copy(root, isolate, `${extractorRelative}/tsconfig.json`);
    for (const name of ['cli.ts', 'extract.ts', 'manifest.ts']) copy(root, isolate, `${extractorRelative}/src/${name}`);
    const providerProbe = record('installed provider identity', execute(process.execPath, ['--input-type=module', '-e', `import fs from 'node:fs';import path from 'node:path';import {fileURLToPath} from 'node:url';const out={};for(const name of ${JSON.stringify(providerNames)}){const entry=fs.realpathSync(fileURLToPath(import.meta.resolve(name)));let directory=path.dirname(entry);while(true){const candidate=path.join(directory,'package.json');if(fs.existsSync(candidate)){const value=JSON.parse(fs.readFileSync(candidate,'utf8'));if(value.name===name){out[name]={entry,name,version:value.version};break;}}const parent=path.dirname(directory);if(parent===directory)throw Error('provider package identity missing');directory=parent;}}process.stdout.write(JSON.stringify(out));`], isolate, env));
    assert.equal(providerProbe.status, 0, providerProbe.output);
    const providers = JSON.parse(providerProbe.stdout);
    for (const name of providerNames) {
      assert.equal(providers[name].version, versions[name], `installed ${name} must match independent maintained lock expectation`);
      providers[name].entry = contained(isolate, providers[name].entry);
    }
    const cli = args => execute(process.execPath, ['--import', 'tsx', `${extractorRelative}/src/cli.ts`, ...args], isolate, env);
    const maintained = script => yarn(isolate, ['run', script]);
    const write = () => { const result = record('maintained writer', maintained('parity:extract')); assert.equal(result.status, 0, result.output); return result; };
    const check = () => { const result = record('restored/baseline maintained checker', maintained('parity:extract:check')); assert.equal(result.status, 0, result.output); assert.match(result.output, /^Would update 0 manifests; 0 added, 0 orphaned symbols\.$/m, 'fixed fixture must have zero drift'); return result; };
    fs.mkdirSync(path.join(isolate, 'packages/safe-cex/src'), {recursive: true});
    fs.writeFileSync(path.join(isolate, 'packages/safe-cex/tsconfig.json'), JSON.stringify({compilerOptions: {strict: true, target: 'ES2022', module: 'ESNext', moduleResolution: 'bundler', types: []}, include: ['src/**/*.ts']}));
    fs.writeFileSync(path.join(isolate, 'packages/safe-cex/src/control.ts'), fixtureText);
    const written = write();
    const manifest = 'sdk-rs/parity/manifest/control.ts.toml';
    const decode = () => {
      const result = record('decoded fixture manifest', execute(process.execPath, ['--input-type=module', '-e', `import fs from 'node:fs';import {parse} from 'smol-toml';process.stdout.write(JSON.stringify(parse(fs.readFileSync(${JSON.stringify(manifest)},'utf8'))));`], isolate, env));
      assert.equal(result.status, 0, result.output);
      return JSON.parse(result.stdout);
    };
    // Independent native expectation from the fixed fixture, before stdout/wire checks.
    const expectedRow = {ts: 'installedControl', kind: 'function', layer: '', rs: '', status: 'pending', note: ''};
    assert.deepEqual(decode(), {file: {ts: 'packages/safe-cex/src/control.ts'}, symbol: [expectedRow]});
    assert.match(written.output, /Updated 1 manifests; 1 added, 0 orphaned symbols/);
    check();
    const annotation = record('fixture annotation', execute(process.execPath, ['--input-type=module', '-e', `import fs from 'node:fs';import {parse,stringify} from 'smol-toml';const file=${JSON.stringify(manifest)};const value=parse(fs.readFileSync(file,'utf8'));value.symbol[0].note='isolated annotation survives reconciliation';fs.writeFileSync(file,stringify(value));`], isolate, env));
    assert.equal(annotation.status, 0, annotation.output);
    write();
    assert.deepEqual(decode().symbol, [{...expectedRow, note: 'isolated annotation survives reconciliation'}]);
    check();
    fs.writeFileSync(path.join(isolate, 'packages/safe-cex/src/control.ts'), fixtureText + 'export function mustMoveRow(): string { return "new"; }\n');
    const drift = record('new-symbol named assertion', maintained('parity:extract:check'));
    assert.notEqual(drift.status, 0, drift.output);
    assert.match(drift.output, /added: packages\/safe-cex\/src\/control\.ts::mustMoveRow \(function\)/);
    fs.writeFileSync(path.join(isolate, 'packages/safe-cex/src/control.ts'), fixtureText);
    check();
    for (const name of providerNames) {
      const directory = path.join(isolate, 'node_modules', name);
      const hidden = `${directory}.missing-control`;
      fs.renameSync(directory, hidden);
      try {
        for (const [label, result] of [[`missing ${name} root checker`, maintained('parity:extract:check')], [`missing ${name} direct CLI`, cli(['--check'])]]) {
          record(label, result);
          assert.notEqual(result.status, 0, `missing ${name} must fail\n${result.output}`);
          assert.match(result.output, new RegExp(name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
          assert.match(result.output, /ERR_MODULE_NOT_FOUND|MODULE_NOT_FOUND|parity extractor dependency/);
        }
      } finally { fs.renameSync(hidden, directory); }
      check();
    }

    assert.equal(receipts.length, 21, 'fixed runtime proof must record all twenty-one child exits');
    assert.equal(receipts.filter(item => item.exit !== 0).length, 7, 'one new-symbol and six missing-provider child failures are required');
    return {scope: 'maintained extractor runtime closure and root commands against fixed isolated fixture', packageManager: rootPackage.packageManager, providers: Object.fromEntries(providerNames.map(name => [name, {name: providers[name].name, version: providers[name].version}])), receipts, controls: ['new fixture symbol', ...providerNames.map(name => `missing ${name}`)], wholeWorkspaceInstall: false, maintainedSourceSemanticClosure: false};
  } finally { fs.rmSync(isolate, {recursive: true, force: true}); }
}

// Importing this module is inert. CI invokes this actual entrypoint, which must
// execute the install proof; failures never print a successful receipt.
if (process.argv[1] && fs.existsSync(process.argv[1]) && fs.realpathSync(process.argv[1]) === fs.realpathSync(fileURLToPath(import.meta.url))) {
  try {
    assert.equal(process.argv.length, 2, 'extractor clean-install proof accepts no arguments');
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
    const receipt = proveExtractorInstall(root);
    process.stdout.write(`${JSON.stringify(receipt)}\n`);
  } catch (error) {
    // Child diagnostics may contain machine paths. Preserve them for a trusted
    // caller of the exported function; the public CLI emits a path-free failure.
    console.error(`extractor clean-install proof failed (${error instanceof assert.AssertionError ? 'AssertionError' : 'Error'}); no pass receipt`);
    process.exitCode = 1;
  }
}
