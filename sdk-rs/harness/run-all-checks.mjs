// Fail-complete native CI runner: every command line runs after earlier failures,
// then one summary names each failing test per crate. PR #3640 surfaced ~124
// failures across 14 crates over four runs because the first failure hid the rest.
import { spawn } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const ANSI = /\u001b\[[0-9;]*m/g;

// One command per non-blank, non-comment line; continuations would be split silently.
export function commandLines(script) {
  const lines = script.split('\n').map((line) => line.trim()).filter((line) => line && !line.startsWith('#'));
  for (const line of lines) {
    if (line.endsWith('\\')) throw new Error(`line continuation is not supported: ${line}`);
  }
  if (!lines.length) throw new Error('no commands to run');
  return lines;
}

// Folds cargo/libtest output into failing tests per crate and target, keyed by
// cargo's own `to rerun pass` hint, plus compile errors that ran no tests.
export function cargoFailures(output) {
  const crates = new Map();
  const compile = [];
  let pending = [];
  const add = (crate, target, names) => {
    if (!crates.has(crate)) crates.set(crate, new Map());
    const targets = crates.get(crate);
    const list = targets.get(target) ?? [];
    for (const name of names) if (!list.includes(name)) list.push(name);
    targets.set(target, list);
  };
  for (const raw of output.split('\n')) {
    const line = raw.replace(ANSI, '').trimEnd();
    if (/^\s+(Running|Doc-tests) /.test(line)) pending = [];
    const failed = /^test (.+) \.\.\. FAILED$/.exec(line);
    if (failed) pending.push(failed[1]);
    const rerun = /to rerun pass `([^`]+)`/.exec(line);
    if (rerun) {
      const pkg = /(?:^|\s)-p (\S+)/.exec(rerun[1]);
      const target = rerun[1].replace(/(?:^|\s)-p \S+/, '').trim() || '(all targets)';
      add(pkg ? pkg[1] : '(root package)', target, pending.length ? pending : ['(binary exited without per-test results)']);
      pending = [];
    }
    const broken = /^error: could not compile `([^`]+)`(.*)$/.exec(line);
    if (broken) compile.push(`${broken[1]}${broken[2].replace(/ due to .*$/, '')}`);
  }
  return { crates, compile };
}

export function summarize(results) {
  const failed = results.filter((result) => result.code !== 0);
  const lines = [`Fail-complete summary: ${failed.length} of ${results.length} command(s) failed`];
  for (const result of failed) {
    lines.push('', `FAILED (exit ${result.code}): ${result.command}`);
    const { crates, compile } = cargoFailures(result.output);
    for (const entry of compile) lines.push(`  compile error: ${entry}`);
    for (const [crate, targets] of crates) {
      for (const [target, names] of targets) {
        lines.push(`  ${crate} ${target}: ${names.length} failing`);
        for (const name of names) lines.push(`    ${name}`);
      }
    }
    if (!crates.size && !compile.length) lines.push('  (no cargo test failures parsed; see this command\'s log above)');
  }
  return lines.join('\n');
}

function run(command) {
  return new Promise((resolve) => {
    // One pipe keeps cargo's stderr headers ordered with libtest's stdout.
    const child = spawn('bash', ['-o', 'pipefail', '-ec', `exec 2>&1\n${command}`], { stdio: ['ignore', 'pipe', 'inherit'] });
    const chunks = [];
    child.stdout.on('data', (chunk) => { process.stdout.write(chunk); chunks.push(chunk); });
    child.on('close', (code, signal) => resolve({ command, code: code ?? (signal ? 128 : 1), output: Buffer.concat(chunks).toString('utf8') }));
  });
}

async function main() {
  const script = process.env.CHECK_COMMANDS;
  if (!script) throw new Error('CHECK_COMMANDS is required');
  const results = [];
  for (const command of commandLines(script)) {
    console.log(`::group::${command}`);
    results.push(await run(command));
    console.log('::endgroup::');
  }
  const summary = summarize(results);
  console.log(`\n${summary}`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    const title = process.env.CHECK_NAME ? `### ${process.env.CHECK_NAME}\n\n` : '';
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${title}\`\`\`\n${summary}\n\`\`\`\n`);
  }
  if (results.some((result) => result.code !== 0)) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error); process.exitCode = 1; });
}
