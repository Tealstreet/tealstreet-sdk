import { existsSync, readFileSync, writeFileSync, mkdirSync, rmSync } from 'fs';
import { join } from 'path';
import { execSync } from 'child_process';
import chalk from 'chalk';
import { getConfigDir } from './config';
import { swapInPlace, updatePaths } from './windows-update';
import packageJson from '../package.json';
import { BRAND, releaseAssetName } from './brand';

const PACKAGE_NAME = BRAND.npmPackage;
const PUBLIC_REPO = BRAND.releaseRepo;
const GITHUB_API_URL = `https://api.github.com/repos/${PUBLIC_REPO}/releases/latest`;
const GITHUB_RELEASES_URL = `https://github.com/${PUBLIC_REPO}/releases/download`;
const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000; // 24 hours
const UPDATE_CHECK_FILE = join(getConfigDir(), 'update-check.json');

interface UpdateCheckState {
  lastCheck: number;
  latestVersion: string | null;
  notifiedVersion: string | null;
  releaseNotes: string | null;
}

function getUpdateState(): UpdateCheckState {
  try {
    if (existsSync(UPDATE_CHECK_FILE)) {
      return JSON.parse(readFileSync(UPDATE_CHECK_FILE, 'utf-8'));
    }
  } catch {}
  return { lastCheck: 0, latestVersion: null, notifiedVersion: null, releaseNotes: null };
}

function saveUpdateState(state: UpdateCheckState): void {
  try {
    const dir = getConfigDir();
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    writeFileSync(UPDATE_CHECK_FILE, JSON.stringify(state, null, 2));
  } catch {}
}

export function getCurrentVersion(): string {
  // Release builds inline the real version via `--define TEALSTREET_CLI_VERSION`
  // (see scripts/build.sh). In dev (`yarn dev`) that define is absent, so fall
  // back to package.json's version instead of a frozen literal that drifts.
  return process.env.TEALSTREET_CLI_VERSION || packageJson.version;
}

/** Is `version` actually ahead of the binary we are running? */
export function isNewerThanCurrent(version: string, current: string = getCurrentVersion()): boolean {
  return compareVersions(version, current) > 0;
}

function compareVersions(a: string, b: string): number {
  const aParts = a.replace(/^v/, '').split('.').map(Number);
  const bParts = b.replace(/^v/, '').split('.').map(Number);

  for (let i = 0; i < Math.max(aParts.length, bParts.length); i++) {
    const aVal = aParts[i] || 0;
    const bVal = bParts[i] || 0;
    if (aVal > bVal) return 1;
    if (aVal < bVal) return -1;
  }
  return 0;
}

interface ReleaseInfo {
  version: string;
  releaseNotes: string | null;
}

/**
 * Extract the "What's New" section from release body
 */
function extractWhatsNew(body: string | null | undefined): string | null {
  if (!body) return null;

  // Look for "## What's New" section and extract content until next "##" section
  const match = body.match(/## What's New\s*\n([\s\S]*?)(?=\n## |$)/i);
  if (match && match[1]) {
    const notes = match[1].trim();
    // Skip if it's just a placeholder dash
    if (notes && notes !== '-') {
      return notes;
    }
  }
  return null;
}

async function fetchLatestVersion(): Promise<ReleaseInfo | null> {
  try {
    // Use public GitHub API - no authentication required
    const response = await fetch(GITHUB_API_URL, {
      headers: {
        Accept: 'application/vnd.github.v3+json',
        'User-Agent': `${BRAND.binName}-cli`,
      },
    });

    if (!response.ok) {
      if (response.status === 404) {
        // No releases yet
        return null;
      }
      throw new Error(`GitHub API returned ${response.status}`);
    }

    const data = (await response.json()) as { tag_name?: string; body?: string };
    const tagName = data.tag_name;

    if (!tagName) {
      return null;
    }

    // Tag format is "v1.2.3", strip the "v" prefix
    return {
      version: tagName.replace(/^v/, ''),
      releaseNotes: extractWhatsNew(data.body),
    };
  } catch {
    // Silent fail - don't interrupt user for network issues
    return null;
  }
}

/**
 * Check for updates in the background (non-blocking)
 * Shows a notification if a new version is available
 */
export function checkForUpdatesBackground(): void {
  // Run check in background without blocking
  setImmediate(async () => {
    try {
      const state = getUpdateState();
      const now = Date.now();

      // Skip if we checked recently
      if (now - state.lastCheck < CHECK_INTERVAL_MS) {
        // But still show notification if we found an update previously.
        // `notifiedVersion !== latestVersion` is NOT enough on its own: after
        // the user updates, the cached `latestVersion` is the version they are
        // now running while `notifiedVersion` still holds the older one, so the
        // banner offered an update from 0.10.6 to 0.10.6 until the cache
        // expired. Compare against what is actually running.
        if (
          state.latestVersion &&
          state.notifiedVersion !== state.latestVersion &&
          isNewerThanCurrent(state.latestVersion)
        ) {
          showUpdateNotification(state.latestVersion, state.releaseNotes);
          state.notifiedVersion = state.latestVersion;
          saveUpdateState(state);
        }
        return;
      }

      const releaseInfo = await fetchLatestVersion();
      state.lastCheck = now;

      if (releaseInfo) {
        state.latestVersion = releaseInfo.version;
        state.releaseNotes = releaseInfo.releaseNotes;
        const currentVersion = getCurrentVersion();

        if (compareVersions(releaseInfo.version, currentVersion) > 0) {
          showUpdateNotification(releaseInfo.version, releaseInfo.releaseNotes);
          state.notifiedVersion = releaseInfo.version;
        }
      }

      saveUpdateState(state);
    } catch {
      // Silent fail - don't interrupt user
    }
  });
}

function showUpdateNotification(latestVersion: string, releaseNotes: string | null): void {
  const currentVersion = getCurrentVersion();
  console.log('');
  console.log(chalk.yellow('┌─────────────────────────────────────────────────┐'));
  console.log(
    chalk.yellow('│') +
      chalk.bold('  Update available! ') +
      chalk.gray(`${currentVersion} → `) +
      chalk.green(latestVersion) +
      chalk.yellow('            │'),
  );
  console.log(
    chalk.yellow('│') +
      chalk.gray('  Run: ') +
      chalk.cyan(`${BRAND.binName} update`) +
      chalk.gray(' to update') +
      chalk.yellow('              │'),
  );
  console.log(chalk.yellow('└─────────────────────────────────────────────────┘'));

  if (releaseNotes) {
    console.log('');
    console.log(chalk.cyan("What's New:"));
    // Format release notes - handle markdown bullet points
    const lines = releaseNotes.split('\n');
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed) {
        // Convert markdown bullets to nice format
        if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
          console.log(chalk.gray('  •') + ' ' + trimmed.slice(2));
        } else {
          console.log(chalk.gray('  ') + trimmed);
        }
      }
    }
  }
  console.log('');
}

/**
 * Perform the actual update
 */
export async function performUpdate(): Promise<boolean> {
  console.log(chalk.cyan('Checking for updates...'));

  const releaseInfo = await fetchLatestVersion();
  const currentVersion = getCurrentVersion();

  if (!releaseInfo) {
    console.log(chalk.yellow('Could not determine latest version.'));
    console.log(chalk.gray(`Current version: v${currentVersion}`));
    return false;
  }

  if (compareVersions(releaseInfo.version, currentVersion) <= 0) {
    console.log(chalk.green(`Already up to date! (v${currentVersion})`));
    return true;
  }

  console.log(chalk.cyan(`Updating ${currentVersion} → ${releaseInfo.version}...`));

  // Show release notes before updating
  if (releaseInfo.releaseNotes) {
    console.log('');
    console.log(chalk.cyan("What's New:"));
    const lines = releaseInfo.releaseNotes.split('\n');
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed) {
        if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
          console.log(chalk.gray('  •') + ' ' + trimmed.slice(2));
        } else {
          console.log(chalk.gray('  ') + trimmed);
        }
      }
    }
    console.log('');
  }

  try {
    // Detect how CLI was installed and update accordingly
    const installMethod = detectInstallMethod();

    switch (installMethod) {
      case 'npm':
        console.log(chalk.gray('Updating via npm...'));
        execSync(`npm update -g ${PACKAGE_NAME}`, { stdio: 'inherit' });
        break;

      case 'homebrew':
        console.log(chalk.gray('Updating via Homebrew...'));
        execSync(`brew upgrade ${BRAND.brewFormula}`, { stdio: 'inherit' });
        break;

      case 'binary':
        console.log(chalk.gray('Downloading latest binary...'));
        await downloadLatestBinary(releaseInfo.version);
        break;

      default:
        console.log(chalk.yellow('Could not determine install method.'));
        console.log(chalk.gray('Try one of:'));
        if (PACKAGE_NAME) console.log(chalk.gray(`  npm update -g ${PACKAGE_NAME}`));
        if (BRAND.brewFormula) console.log(chalk.gray(`  brew upgrade ${BRAND.brewFormula}`));
        console.log(chalk.gray(`  https://github.com/${PUBLIC_REPO}/releases/latest`));
        return false;
    }

    console.log(chalk.green(`Successfully updated to v${releaseInfo.version}!`));

    // Clear update state
    saveUpdateState({
      lastCheck: Date.now(),
      latestVersion: releaseInfo.version,
      notifiedVersion: releaseInfo.version,
      releaseNotes: releaseInfo.releaseNotes,
    });

    return true;
  } catch (error) {
    console.log(chalk.red('Update failed:'), error);
    return false;
  }
}

function detectInstallMethod(): 'npm' | 'homebrew' | 'binary' | 'unknown' {
  // `2>/dev/null` is a POSIX shell redirect. execSync runs through cmd.exe on
  // Windows, which reads it as a path under a non-existent `/dev`, so BOTH
  // probes threw and detection fell through to 'binary' — sending an
  // npm-installed Windows user down the self-replace path that cannot work
  // there. `stdio: 'pipe'` suppresses the output cross-platform instead.
  try {
    // Check if installed via Homebrew
    const brewList = BRAND.brewFormula
      ? execSync(`brew list ${BRAND.brewFormula}`, { encoding: 'utf-8', stdio: 'pipe' })
      : '';
    if (brewList) return 'homebrew';
  } catch {}

  try {
    // Check if installed via npm
    const npmList = PACKAGE_NAME ? execSync(`npm list -g ${PACKAGE_NAME}`, { encoding: 'utf-8', stdio: 'pipe' }) : '';
    if (PACKAGE_NAME && npmList.includes(PACKAGE_NAME)) return 'npm';
  } catch {}

  // Check if running as compiled binary
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if ((process as any).pkg || !process.argv[1]?.endsWith('.ts')) {
    return 'binary';
  }

  return 'unknown';
}

/**
 * Windows cannot overwrite a running .exe, so the new binary is staged beside
 * the current one and swapped in by rename. See src/windows-update.ts for why
 * the ordering matters. Uses fetch rather than shelling out to curl: curl.exe
 * exists on Windows 10+, but going through cmd.exe is what broke the update
 * probes already.
 */
async function downloadAndSwapWindows(version: string, binaryName: string): Promise<void> {
  const currentBinary = process.execPath;
  const { next } = updatePaths(currentBinary);
  const downloadUrl = `${GITHUB_RELEASES_URL}/v${version}/${binaryName}`;

  console.log(chalk.gray(`Downloading ${binaryName}...`));

  const response = await fetch(downloadUrl);
  if (!response.ok) {
    throw new Error(`Failed to download from ${downloadUrl} (HTTP ${response.status})`);
  }
  writeFileSync(next, Buffer.from(await response.arrayBuffer()));

  try {
    swapInPlace(currentBinary, next);
  } catch (error) {
    rmSync(next, { force: true });
    throw error;
  }

  console.log(chalk.gray(`Replaced the binary. Restart ${BRAND.binName} to run the new version.`));
}

async function downloadLatestBinary(version: string): Promise<void> {
  const platform = process.platform;
  const arch = process.arch;

  let binaryName = BRAND.binName;
  if (platform === 'darwin') {
    binaryName = releaseAssetName(BRAND, arch === 'arm64' ? 'darwin-arm64' : 'darwin-x64');
  } else if (platform === 'linux') {
    binaryName = releaseAssetName(BRAND, arch === 'arm64' ? 'linux-arm64' : 'linux-x64');
  } else if (platform === 'win32') {
    binaryName = releaseAssetName(BRAND, 'windows-x64');
  } else {
    throw new Error(`Unsupported platform: ${platform}`);
  }

  if (platform === 'win32') {
    await downloadAndSwapWindows(version, binaryName);
    return;
  }

  const downloadUrl = `${GITHUB_RELEASES_URL}/v${version}/${binaryName}`;
  const tempPath = `/tmp/${binaryName}`;

  console.log(chalk.gray(`Downloading ${binaryName}...`));

  // Download using curl (available on macOS and Linux)
  try {
    execSync(`curl -fsSL "${downloadUrl}" -o "${tempPath}"`, { stdio: 'inherit' });
  } catch {
    throw new Error(`Failed to download from ${downloadUrl}`);
  }

  execSync(`chmod +x "${tempPath}"`);

  // Get current binary location
  const currentBinary = process.execPath;

  // Replace current binary (may need sudo)
  try {
    execSync(`mv "${tempPath}" "${currentBinary}"`);
  } catch {
    console.log(chalk.yellow('Need elevated permissions to update. Running with sudo...'));
    execSync(`sudo mv "${tempPath}" "${currentBinary}"`, { stdio: 'inherit' });
  }
}

/**
 * Force a version check (ignores cooldown)
 */
export async function forceCheckForUpdates(): Promise<void> {
  console.log(chalk.cyan('Checking for updates...'));

  const releaseInfo = await fetchLatestVersion();
  const currentVersion = getCurrentVersion();

  if (!releaseInfo) {
    console.log(chalk.yellow('Could not determine latest version.'));
    console.log(chalk.gray(`Current version: v${currentVersion}`));
    return;
  }

  if (compareVersions(releaseInfo.version, currentVersion) > 0) {
    showUpdateNotification(releaseInfo.version, releaseInfo.releaseNotes);
  } else {
    console.log(chalk.green(`You're up to date! (v${currentVersion})`));
  }

  // Update state
  saveUpdateState({
    lastCheck: Date.now(),
    latestVersion: releaseInfo.version,
    notifiedVersion: releaseInfo.version,
    releaseNotes: releaseInfo.releaseNotes,
  });
}
