#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const crypto = require('node:crypto');
const https = require('node:https');
const os = require('node:os');
const path = require('node:path');

const packageJson = require('../package.json');

const MAX_REDIRECTS = 10;
const REQUEST_TIMEOUT_MS = 60_000;

function resolvePlatform() {
  const platformMap = {
    darwin: 'darwin',
    linux: 'linux',
  };
  const archMap = {
    arm64: 'arm64',
    x64: 'x64',
  };

  const platform = platformMap[process.platform];
  const arch = archMap[process.arch];

  if (!platform || !arch) {
    throw new Error(`Unsupported platform: ${process.platform}-${process.arch}`);
  }

  return `${platform}-${arch}`;
}

function getCacheDir() {
  const root =
    process.env.TEALSTREET_CLI_CACHE_DIR ||
    process.env.XDG_CACHE_HOME ||
    path.join(os.homedir(), '.cache');
  return path.join(root, 'tealstreet-cli', packageJson.version);
}

function getBinaryPath() {
  const target = resolvePlatform();
  return path.join(getCacheDir(), `tealstreet-${target}`);
}

function sha256File(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function validateDownloadInputs(url, destination) {
  if (!url || !destination) {
    throw new Error(
      '--download-only requires TEALSTREET_CLI_DOWNLOAD_URL and TEALSTREET_CLI_DOWNLOAD_DESTINATION',
    );
  }

  const parsedUrl = new URL(url);
  if (parsedUrl.protocol !== 'https:') {
    throw new Error(`Unsupported download URL protocol: ${parsedUrl.protocol}`);
  }

  if (!path.isAbsolute(destination)) {
    throw new Error('TEALSTREET_CLI_DOWNLOAD_DESTINATION must be an absolute path');
  }
}

function requestUrl(url, onResponse, redirectDepth = 0) {
  return new Promise((resolve, reject) => {
    if (redirectDepth > MAX_REDIRECTS) {
      reject(new Error(`Too many redirects for ${url}`));
      return;
    }

    const req = https.get(url, (response) => {
      if (
        response.statusCode >= 300 &&
        response.statusCode < 400 &&
        response.headers.location
      ) {
        const nextUrl = new URL(response.headers.location, url).toString();
        response.resume();
        requestUrl(nextUrl, onResponse, redirectDepth + 1).then(resolve, reject);
        return;
      }

      try {
        onResponse(response, resolve, reject);
      } catch (error) {
        reject(error);
      }
    });

    req.setTimeout(REQUEST_TIMEOUT_MS, () => {
      req.destroy(new Error(`Request timeout after ${REQUEST_TIMEOUT_MS}ms: ${url}`));
    });

    req.on('error', (error) => {
      reject(error);
    });
  });
}

function fetchText(url) {
  return requestUrl(url, (response, resolve, reject) => {
    if (response.statusCode !== 200) {
      response.resume();
      reject(new Error(`Download failed with HTTP ${response.statusCode}: ${url}`));
      return;
    }

    response.setEncoding('utf8');
    let body = '';
    response.on('data', (chunk) => {
      body += chunk;
    });
    response.on('end', () => resolve(body));
    response.on('error', reject);
  });
}

function download(url, destination, { expectedSha256 } = {}) {
  return new Promise((resolve, reject) => {
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    const temp = `${destination}.tmp-${process.pid}`;
    let settled = false;

    const cleanup = () => fs.rmSync(temp, { force: true });
    const fail = (error) => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(error);
    };
    const succeed = () => {
      if (settled) return;
      settled = true;
      resolve();
    };

    requestUrl(url, (response, requestResolve, requestReject) => {
      if (response.statusCode !== 200) {
        response.resume();
        requestReject(new Error(`Download failed with HTTP ${response.statusCode}: ${url}`));
        return;
      }

      const file = fs.createWriteStream(temp, { mode: 0o755 });
      response.pipe(file);

      response.on('error', (error) => {
        file.destroy(error);
      });
      file.on('error', requestReject);
      file.on('finish', () => {
        file.close((error) => {
          if (error) {
            requestReject(error);
            return;
          }

          try {
            if (expectedSha256 && sha256File(temp) !== expectedSha256) {
              requestReject(new Error(`Checksum mismatch for ${url}`));
              return;
            }

            fs.chmodSync(temp, 0o755);
            fs.renameSync(temp, destination);
            requestResolve();
          } catch (renameError) {
            requestReject(renameError);
          }
        });
      });
    }).then(succeed, fail);
  });
}

async function fetchExpectedSha256(target) {
  const sumsUrl = `https://github.com/Tealstreet/cli/releases/download/v${packageJson.version}/SHA256SUMS`;
  const sums = await fetchText(sumsUrl);
  for (const line of sums.split(/\r?\n/)) {
    const [hash, file] = line.trim().split(/\s+/);
    if (file === `tealstreet-${target}` && /^[a-f0-9]{64}$/i.test(hash)) {
      return hash;
    }
  }
  throw new Error(`Missing checksum for tealstreet-${target}`);
}

function ensureBinary({ optional = false } = {}) {
  const target = resolvePlatform();
  const binary = getBinaryPath();

  if (fs.existsSync(binary)) {
    return binary;
  }

  const url = `https://github.com/Tealstreet/cli/releases/download/v${packageJson.version}/tealstreet-${target}`;

  try {
    const childProcess = require('node:child_process');
    const expectedSha256 = childProcess
      .execFileSync(process.execPath, [__filename, '--checksum-only'], {
        encoding: 'utf8',
        env: {
          ...process.env,
          TEALSTREET_CLI_CHECKSUM_TARGET: target,
        },
      })
      .trim();
    childProcess.execFileSync(process.execPath, [__filename, '--download-only'], {
      stdio: optional ? 'ignore' : 'inherit',
      env: {
        ...process.env,
        TEALSTREET_CLI_DOWNLOAD_URL: url,
        TEALSTREET_CLI_DOWNLOAD_DESTINATION: binary,
        TEALSTREET_CLI_DOWNLOAD_SHA256: expectedSha256,
      },
    });
    return binary;
  } catch (error) {
    if (optional) {
      console.warn(`tealstreet: skipped binary preinstall (${error.message})`);
      return fs.existsSync(binary) ? binary : null;
    }
    throw error;
  }
}

async function main() {
  const optional = process.argv.includes('--optional');
  const downloadOnly = process.argv.includes('--download-only');
  const checksumOnly = process.argv.includes('--checksum-only');

  try {
    if (checksumOnly) {
      const target = process.env.TEALSTREET_CLI_CHECKSUM_TARGET;
      if (!target) {
        throw new Error('--checksum-only requires TEALSTREET_CLI_CHECKSUM_TARGET');
      }
      process.stdout.write(await fetchExpectedSha256(target));
      return;
    }

    if (downloadOnly) {
      const url = process.env.TEALSTREET_CLI_DOWNLOAD_URL;
      const destination = process.env.TEALSTREET_CLI_DOWNLOAD_DESTINATION;
      const expectedSha256 = process.env.TEALSTREET_CLI_DOWNLOAD_SHA256;
      validateDownloadInputs(url, destination);
      await download(url, destination, { expectedSha256 });
      return;
    }

    ensureBinary({ optional });
  } catch (error) {
    if (optional) {
      console.warn(`tealstreet: skipped binary preinstall (${error.message})`);
      return;
    }
    console.error(error.message);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}

module.exports = {
  ensureBinary,
  getBinaryPath,
  resolvePlatform,
};
