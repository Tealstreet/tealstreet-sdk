import http from 'http';
import open from 'open';
import crypto from 'crypto';
import chalk from 'chalk';
import { setProxyToken } from '@tealstreet/safe-cex/src/accessToken';
import { loadConfig, saveConfig } from './config';
import { BRAND } from './brand';

/** Set by whichever path warns first; see `checkAndRefreshToken`. */
export let warnedTokenExpiryThisProcess = false;
export const markTokenExpiryWarned = (): void => {
  warnedTokenExpiryThisProcess = true;
};

// Tealstreet web app handles OAuth with Auth0
// Default to localhost:3000 for local development
// Release builds set TEALSTREET_API to preview.tealstreet.io
const TEALSTREET_API = process.env.TEALSTREET_API || 'http://localhost:3000';
const CALLBACK_PORT = 9876;
const CALLBACK_URL = `http://localhost:${CALLBACK_PORT}/callback`;

export function getTealstreetApiBaseUrl(): string {
  return TEALSTREET_API;
}

interface CallbackResult {
  token: string;
  state: string;
  email?: string;
  userId?: string;
}

/**
 * Generate random state for CSRF protection
 */
function generateState(): string {
  return crypto.randomBytes(16).toString('hex');
}

/**
 * Start local HTTP server to receive OAuth callback from web app
 */
function startCallbackServer(expectedState: string): Promise<CallbackResult> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let timeoutHandle: NodeJS.Timeout | null = null;
    const settle = (fn: () => void) => {
      if (settled) return;
      settled = true;
      if (timeoutHandle) clearTimeout(timeoutHandle);
      fn();
    };

    const server = http.createServer(async (req, res) => {
      const url = new URL(req.url || '', `http://localhost:${CALLBACK_PORT}`);

      if (url.pathname === '/callback') {
        const token = url.searchParams.get('token');
        const state = url.searchParams.get('state');
        const email = url.searchParams.get('email') || undefined;
        const userId = url.searchParams.get('userId') || undefined;
        const error = url.searchParams.get('error');

        if (error) {
          res.writeHead(400, { 'Content-Type': 'text/html' });
          res.end(`
            <html>
              <body style="font-family: system-ui; padding: 40px; text-align: center;">
                <h1 style="color: #ef4444;">Authentication Failed</h1>
                <p>${error}</p>
                <p>You can close this window.</p>
              </body>
            </html>
          `);
          server.close();
          settle(() => reject(new Error(error)));
          return;
        }

        if (state !== expectedState) {
          res.writeHead(400, { 'Content-Type': 'text/html' });
          res.end(`
            <html>
              <body style="font-family: system-ui; padding: 40px; text-align: center;">
                <h1 style="color: #ef4444;">Authentication Failed</h1>
                <p>Invalid state parameter. Please try again.</p>
                <p>You can close this window.</p>
              </body>
            </html>
          `);
          server.close();
          settle(() => reject(new Error('Invalid state parameter')));
          return;
        }

        if (!token) {
          res.writeHead(400, { 'Content-Type': 'text/html' });
          res.end(`
            <html>
              <body style="font-family: system-ui; padding: 40px; text-align: center;">
                <h1 style="color: #ef4444;">Authentication Failed</h1>
                <p>No token received.</p>
                <p>You can close this window.</p>
              </body>
            </html>
          `);
          server.close();
          settle(() => reject(new Error('No token received')));
          return;
        }

        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(`
          <html>
            <body style="font-family: system-ui; padding: 40px; text-align: center;">
              <h1 style="color: #22c55e;">Authentication Successful!</h1>
              <p>You can close this window and return to the CLI.</p>
            </body>
          </html>
        `);

        server.close();
        settle(() => resolve({ token, state, email, userId }));
      }
    });

    server.on('error', (err: NodeJS.ErrnoException) => {
      if (err.code === 'EADDRINUSE') {
        settle(() =>
          reject(
            new Error(`Port ${CALLBACK_PORT} is already in use. Please close other applications using this port.`),
          ),
        );
      } else {
        settle(() => reject(err));
      }
    });

    server.listen(CALLBACK_PORT, () => {
      console.log(chalk.gray(`Listening for callback on port ${CALLBACK_PORT}...`));
    });

    // Timeout after 5 minutes
    timeoutHandle = setTimeout(
      () => {
        server.close();
        settle(() => reject(new Error('Authentication timed out')));
      },
      5 * 60 * 1000,
    );
  });
}

/**
 * Decode JWT to get expiration
 */
function decodeJwt(token: string): any {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  try {
    // Base64url to base64: replace - with +, _ with /, add padding
    // This is more compatible than 'base64url' encoding which may not work in all runtimes
    let payload = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    while (payload.length % 4) {
      payload += '=';
    }
    return JSON.parse(Buffer.from(payload, 'base64').toString());
  } catch {
    return null;
  }
}

/**
 * Login to Tealstreet via web app OAuth flow
 *
 * Flow:
 * 1. CLI starts local server on localhost:9876
 * 2. CLI opens browser to tealstreet.io/api/cli/login?callback=localhost:9876&state=xxx
 * 3. Web app handles Auth0 login (with client secret server-side)
 * 4. Web app redirects to localhost:9876/callback?token=xxx&state=xxx
 * 5. CLI receives token and saves it
 */
export async function login(): Promise<boolean> {
  console.log(chalk.cyan(`Starting ${BRAND.displayName} authentication...`));

  const state = generateState();

  // Build login URL pointing to web app
  const loginUrl = new URL(`${TEALSTREET_API}/api/cli/login`);
  loginUrl.searchParams.set('callback', CALLBACK_URL);
  loginUrl.searchParams.set('state', state);

  // Start callback server first
  const callbackPromise = startCallbackServer(state);

  // Open browser
  console.log(chalk.gray('Opening browser for authentication...'));
  console.log(chalk.gray(`If the browser doesn't open, visit: ${loginUrl.toString()}`));
  await open(loginUrl.toString());

  try {
    // Wait for callback with token
    const result = await callbackPromise;

    // Decode token to get expiration
    const tokenPayload = decodeJwt(result.token);
    const expiresAt = tokenPayload?.exp ? tokenPayload.exp * 1000 : Date.now() + 7 * 24 * 60 * 60 * 1000;

    // Save to config - store the Auth0 access token, not the proxy token
    // The proxy token will be fetched on-demand when needed
    const config = loadConfig();
    config.auth = {
      accessToken: result.token,
      expiresAt,
      userId: result.userId,
    };
    saveConfig(config);

    // Prime safe-cex's in-memory cache so any in-flight HOW retry picks up the
    // new token without waiting for the next refresh cycle.
    setProxyToken(result.token);

    console.log(chalk.green(`Successfully logged in as ${result.email || result.userId || 'user'}`));
    return true;
  } catch (error: any) {
    console.log(chalk.red(`Authentication failed: ${error.message}`));
    return false;
  }
}

// Token refresh threshold - refresh if less than 10 days remaining
const TOKEN_REFRESH_THRESHOLD_DAYS = 10;

/**
 * Check if user is logged in with valid tokens
 */
export function isLoggedIn(): boolean {
  const config = loadConfig();
  if (!config.auth?.accessToken) return false;

  // Check actual JWT expiry (authoritative source)
  const tokenPayload = decodeJwt(config.auth.accessToken);
  if (tokenPayload?.exp) {
    const jwtExpiresAt = tokenPayload.exp * 1000;
    if (jwtExpiresAt < Date.now()) return false;
  } else if (config.auth.expiresAt && config.auth.expiresAt < Date.now()) {
    // Fallback to stored expiresAt if JWT can't be decoded
    return false;
  }

  return true;
}

/**
 * Check if token needs refresh (less than threshold days remaining)
 */
export function tokenNeedsRefresh(): boolean {
  const config = loadConfig();
  if (!config.auth?.accessToken) return false;

  // Use actual JWT expiry
  const tokenPayload = decodeJwt(config.auth.accessToken);
  const expiresAt = tokenPayload?.exp ? tokenPayload.exp * 1000 : config.auth.expiresAt;
  if (!expiresAt) return false;

  const daysRemaining = (expiresAt - Date.now()) / (24 * 60 * 60 * 1000);
  return daysRemaining < TOKEN_REFRESH_THRESHOLD_DAYS && daysRemaining > 0;
}

/**
 * Get days until token expires
 */
export function getTokenExpiryDays(): number {
  const config = loadConfig();
  if (!config.auth?.accessToken) return 0;

  // Use actual JWT expiry
  const tokenPayload = decodeJwt(config.auth.accessToken);
  const expiresAt = tokenPayload?.exp ? tokenPayload.exp * 1000 : config.auth.expiresAt;
  if (!expiresAt) return 0;

  return Math.max(0, Math.round((expiresAt - Date.now()) / (24 * 60 * 60 * 1000)));
}

/**
 * Check token status and prompt for refresh if needed
 * Returns true if auth is valid, false if user needs to login
 */
export async function checkAndRefreshToken(): Promise<boolean> {
  const config = loadConfig();

  // No token at all
  if (!config.auth?.accessToken) {
    return false;
  }

  // Check actual JWT expiry (authoritative source)
  const tokenPayload = decodeJwt(config.auth.accessToken);
  const jwtExpiresAt = tokenPayload?.exp ? tokenPayload.exp * 1000 : config.auth.expiresAt;

  // Token expired
  if (jwtExpiresAt && jwtExpiresAt < Date.now()) {
    console.log(chalk.yellow('Your authentication token has expired.'));
    console.log(chalk.gray('Run "login" to re-authenticate.'));
    return false;
  }

  // Token expiring soon - offer to refresh. Once per process: the REPL and the
  // one-shot path both warn at startup, and this fires again per exchange
  // created, so a single `balance` printed the same notice twice.
  if (tokenNeedsRefresh() && !warnedTokenExpiryThisProcess) {
    warnedTokenExpiryThisProcess = true;
    const daysLeft = getTokenExpiryDays();
    console.log(chalk.yellow(`Your authentication token expires in ${daysLeft} days.`));
    console.log(chalk.gray('Run "login" to refresh your token.'));
  }

  return true;
}

/**
 * Get proxy token from config.
 * Note: The CLI login returns a proxy token directly (signed with privateProxyKey),
 * not an Auth0 access token. The token is valid for 7 days.
 */
export function getProxyTokenFromConfig(): string | null {
  const config = loadConfig();
  return config.auth?.accessToken || null;
}

/**
 * Logout - clear stored tokens
 */
export function logout(): void {
  const config = loadConfig();
  delete config.auth;
  saveConfig(config);
  console.log(chalk.green('Logged out successfully.'));
}

export type AuthState = {
  state: 'signed_in' | 'not_logged_in' | 'expired';
  userId?: string;
  /** Whole days left on the earlier of the stored and the token's own expiry. */
  expiresInDays?: number;
};

/** Machine-readable sign-in state: expired once the EARLIER of the two expiry times has passed. */
export function getAuthState(): AuthState {
  const config = loadConfig();
  if (!config.auth?.accessToken) return { state: 'not_logged_in' };
  const tokenPayload = decodeJwt(config.auth.accessToken);
  const expiries = [tokenPayload?.exp ? tokenPayload.exp * 1000 : undefined, config.auth.expiresAt].filter(
    (value): value is number => typeof value === 'number',
  );
  const expiresAt = expiries.length ? Math.min(...expiries) : undefined;
  const userId = config.auth.userId || tokenPayload?.sub;
  if (expiresAt !== undefined && expiresAt <= Date.now()) return { state: 'expired', userId };
  return {
    state: 'signed_in',
    userId,
    expiresInDays: expiresAt === undefined ? undefined : Math.floor((expiresAt - Date.now()) / 86_400_000),
  };
}

/**
 * Show current auth status
 */
export function showAuthStatus(): void {
  const config = loadConfig();
  if (!config.auth?.accessToken) {
    console.log(chalk.yellow('Not logged in.'));
    console.log(chalk.gray(`Run "login" to authenticate with ${BRAND.displayName}.`));
    return;
  }

  const tokenPayload = decodeJwt(config.auth.accessToken);

  // Calculate expiry from both sources
  const configExpiresIn = config.auth.expiresAt
    ? Math.max(0, Math.round((config.auth.expiresAt - Date.now()) / 1000 / 60 / 60 / 24))
    : 0;

  // Check actual JWT expiry
  const jwtExpiresAt = tokenPayload?.exp ? tokenPayload.exp * 1000 : null;
  const jwtExpiresIn = jwtExpiresAt
    ? Math.max(0, Math.round((jwtExpiresAt - Date.now()) / 1000 / 60 / 60 / 24))
    : null;

  // Detect mismatch between stored expiresAt and actual JWT exp
  const hasMismatch =
    jwtExpiresAt && config.auth.expiresAt && Math.abs(jwtExpiresAt - config.auth.expiresAt) > 60000; // More than 1 minute difference

  console.log(chalk.green('Logged in'));
  console.log(chalk.gray(`  User ID: ${config.auth.userId || tokenPayload?.sub || 'Unknown'}`));

  if (hasMismatch) {
    // Show JWT expiry (the authoritative source)
    if (jwtExpiresIn !== null && jwtExpiresIn <= 0) {
      console.log(chalk.red(`  Token EXPIRED (JWT shows expired)`));
      console.log(chalk.yellow(`  Run "login" to refresh your token.`));
    } else {
      console.log(chalk.gray(`  Token expires in: ${jwtExpiresIn} days`));
    }
  } else {
    console.log(chalk.gray(`  Token expires in: ${configExpiresIn} days`));
  }
}
