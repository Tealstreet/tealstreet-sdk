const { spawn, execFileSync } = require('node:child_process');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const { constants } = require('node:os');
const { StringDecoder } = require('node:string_decoder');
const phases = new Map([['Validating package.json', 1], ['Resolving packages', 2], ['Fetching packages', 3]]);

function retrySafeSources(lockfile) {
  // Git fetchers can run prepare scripts during fetching, before the ordinary
  // build phase. New non-registry source kinds require an explicit re-audit.
  return lockfile.split('\n').filter(line => /^\s*resolved\b/.test(line))
    .every(line => /^\s+resolved "https?:\/\/[^"\s]+\.tgz(?:#[a-fA-F0-9]+)?"\s*$/.test(line));
}

function networkError(message) {
  const first = message.split('\n', 1)[0].replace(/^Error: /, '');
  // Yarn's own request errors include the URL. Do not match arbitrary mentions
  // of 503/timeouts in lockfile, checksum or lifecycle errors.
  if (/^https?:\/\/\S+: Request failed "(?:408|429|5\d\d)(?: [^"\r\n]+)?"$/.test(first)) return true;
  const transport = first.replace(/^https?:\/\/\S+: /, '');
  return /^(?:(?:getaddrinfo|connect|read|write) )?(?:EAI_AGAIN|ENOTFOUND|ECONNRESET|ECONNREFUSED|ETIMEDOUT|ESOCKETTIMEDOUT|ENETUNREACH|EHOSTUNREACH)(?:[ :].*)?$/.test(transport)
    || (/^https?:\/\//.test(first) && transport === 'socket hang up');
}

class InstallEvidence {
  constructor(manifest) {
    // Yarn runs root preinstall before reporting any install phase. Never
    // replay that hook, including one that prints network-looking JSON.
    this.blocked = Boolean(manifest.scripts?.preinstall);
    this.fetching = false;
    this.stage = 0;
    this.networkErrors = 0;
  }

  line(line) {
    if (!line.trim()) return;
    let event;
    try { event = JSON.parse(line); } catch {
      // Node's nonfatal warnings may accompany otherwise structured Yarn logs.
      if (!/^\(node:\d+\) (?:\[[A-Z0-9]+\] )?(?:DeprecationWarning|ExperimentalWarning|Warning):/.test(line)
          && !/^\(Use .+ to show where the warning was created\)$/.test(line)
          && !/^\s+at /.test(line)) this.blocked = true;
      return;
    }
    if (!event || typeof event !== 'object' || typeof event.type !== 'string') { this.blocked = true; return; }
    if (event.type === 'step') {
      const message = event.data?.message;
      const stage = phases.get(typeof message === 'string' ? message.replace(/\.\.\.$/, '') : '');
      if (!stage || stage < this.stage) this.blocked = true;
      this.stage = stage ?? this.stage;
      this.fetching = stage === 2 || stage === 3;
    } else if (event.type === 'command') {
      this.blocked = true;
    } else if (event.type === 'error') {
      if (this.fetching && typeof event.data === 'string' && networkError(event.data)) this.networkErrors++;
      else this.blocked = true;
    }
  }

  retryable(exit) { return exit === 1 && !this.blocked && this.networkErrors > 0; }
}

async function attempt(manifest, safeSources) {
  const evidence = new InstallEvidence(manifest);
  if (!safeSources) evidence.blocked = true;
  // A single ordered pipe matters: lifecycle/phase events and fatal errors
  // must be classified in the same order they appeared, across both streams.
  const child = spawn('bash', ['-c', 'exec corepack yarn install --frozen-lockfile --network-timeout 600000 --prefer-offline --json 2>&1'], {
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const decoder = new StringDecoder('utf8');
  let pending = '';
  child.stdout.on('data', data => {
    process.stdout.write(data);
    pending += decoder.write(data);
    let end;
    while ((end = pending.indexOf('\n')) !== -1) {
      evidence.line(pending.slice(0, end));
      pending = pending.slice(end + 1);
    }
    if (pending.length > 1024 * 1024) { evidence.blocked = true; pending = ''; }
  });
  return new Promise((resolveAttempt, reject) => {
    child.on('error', reject);
    child.stdout.on('error', reject);
    child.on('close', (code, signal) => {
      evidence.line(pending + decoder.end());
      const exit = code ?? (128 + (constants.signals[signal] ?? 1));
      resolveAttempt({ exit, retryable: evidence.retryable(exit) });
    });
  });
}

async function main() {
  process.chdir(resolve(__dirname, '../../..'));
  const manifest = JSON.parse(readFileSync('package.json', 'utf8'));
  const safeSources = retrySafeSources(readFileSync('yarn.lock', 'utf8'));
  for (let n = 1; n <= 3; n++) {
    const result = await attempt(manifest, safeSources);
    if (!result.retryable || n === 3) return result.exit;
    const delay = n * 5;
    console.error(`Yarn dependency download failed (attempt ${n}/3); retrying in ${delay}s`);
    execFileSync('sleep', [String(delay)], { stdio: 'inherit' });
  }
}

module.exports = { InstallEvidence, networkError, retrySafeSources };
if (require.main === module) {
  main().then(exit => { process.exitCode = exit; }, error => { console.error(error); process.exitCode = 1; });
}
