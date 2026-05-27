import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';
import { execSync } from 'child_process';
import chalk from 'chalk';
import { getConfigDir } from './config';
import packageJson from '../package.json';

const PACKAGE_NAME = '@tealstreet/cli';
const PUBLIC_REPO = 'Tealstreet/cli';
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
        'User-Agent': 'tealstreet-cli',
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
        // But still show notification if we found an update previously
        if (state.latestVersion && state.notifiedVersion !== state.latestVersion) {
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
      chalk.cyan('tealstreet update') +
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
        execSync('brew upgrade tealstreet', { stdio: 'inherit' });
        break;

      case 'binary':
        console.log(chalk.gray('Downloading latest binary...'));
        await downloadLatestBinary(releaseInfo.version);
        break;

      default:
        console.log(chalk.yellow('Could not determine install method.'));
        console.log(chalk.gray('Try one of:'));
        console.log(chalk.gray('  npm update -g @tealstreet/cli'));
        console.log(chalk.gray('  brew upgrade tealstreet'));
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
  try {
    // Check if installed via Homebrew
    const brewList = execSync('brew list tealstreet 2>/dev/null', { encoding: 'utf-8' });
    if (brewList) return 'homebrew';
  } catch {}

  try {
    // Check if installed via npm
    const npmList = execSync(`npm list -g ${PACKAGE_NAME} 2>/dev/null`, { encoding: 'utf-8' });
    if (npmList.includes(PACKAGE_NAME)) return 'npm';
  } catch {}

  // Check if running as compiled binary
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if ((process as any).pkg || !process.argv[1]?.endsWith('.ts')) {
    return 'binary';
  }

  return 'unknown';
}

async function downloadLatestBinary(version: string): Promise<void> {
  const platform = process.platform;
  const arch = process.arch;

  let binaryName = 'tealstreet';
  if (platform === 'darwin') {
    binaryName = arch === 'arm64' ? 'tealstreet-darwin-arm64' : 'tealstreet-darwin-x64';
  } else if (platform === 'linux') {
    binaryName = arch === 'arm64' ? 'tealstreet-linux-arm64' : 'tealstreet-linux-x64';
  } else {
    throw new Error(`Unsupported platform: ${platform}`);
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
