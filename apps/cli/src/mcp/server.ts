import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z, type ZodRawShape } from 'zod';

import { checkTopLevelEffects, readScriptMeta, validate, type ValidationIssue } from '@tealstreet/script-authoring';

import { recordMcpScriptAction } from '../audit-log';
import { isLoggedIn } from '../auth';
import { CLI_TYPES_DTS, CORE_TYPES_DTS, WEB_TYPES_DTS } from '@tealstreet/script-authoring/src/script-types-bundle';
import { SCRIPT_TYPES_BUNDLE } from '@tealstreet/script-authoring/src/scripts-templates';
import {
  deleteScript,
  listScriptFiles,
  listScripts,
  loadScripts,
  lookupScript,
  readScriptSource,
  scriptPath,
  VALID_SCRIPT_NAME,
  writeScript,
} from '../scripts-store';
import { createDryRunAdapter } from './dryRunAdapter';
import { disconnectAll, ensureConnected, getExchangeManager, listAccountSummaries } from './liveState';
import { getProtocolStdout } from './stdoutGuard';
import type { CLISession } from '@tealstreet/cli-core';

/** The exact text seeded into the user's `types.d.ts` — see
 *  `SCRIPT_TYPES_BUNDLE`. An agent must not author against a different
 *  surface than the one in the user's editor. */
const TYPES_BUNDLE = SCRIPT_TYPES_BUNDLE;

const SERVER_INSTRUCTIONS = `tealstreet-scripts — author and inspect Tealstreet custom scripts.

Scripts are headless TypeScript that automate trading actions. They live one-file-per-script
in ~/.tealstreet/scripts/ and are invoked from the Tealstreet CLI as \`script:<name>\`.

HOW TO WRITE ONE — the constraints are unusual, read these before generating code:
- Runtime values come ONLY from the \`api\` parameter. NEVER use a value import:
  \`import { OrderSide } from '@tealstreet/script-api-core'\` COMPILES but FAILS AT LOAD,
  because scripts run from a directory with no node_modules. Use \`import type\` for types
  and \`api.constants.*\` for runtime values.
- One file, one default export: \`export default async function (args, api) { ... }\`.
  No local imports, no dynamic import(), no re-exports.
- Export \`meta: ScriptMeta\`. \`capabilities.writes\` MUST list every api.exchange.* method
  the body calls, or the dispatcher throws at runtime. \`closePosition\` additionally
  requires \`placeOrder\` (it is synthesized via placeOrder under the hood).
- Omit \`capabilities.reads\` to allow all reads; list it only to restrict.
- Put ALL logic inside the default function. Top-level CALLS are REJECTED by validation:
  they run at import time whenever the CLI loads the scripts directory, not when the
  script is invoked. Top-level declarations (types, helper functions, const config) are fine.

WORKFLOW:
1. \`get_api_types\` to see the exact available surface. Do not guess method names.
2. \`validate_script\` freely — it is fast, local, and costs nothing. Iterate against it.
3. \`write_script\` only once validation passes. It re-validates and refuses invalid source,
   because a broken file in that directory degrades the user's next CLI session.

RUNNING SCRIPTS:
\`run_script\` is DRY RUN ONLY. Reads hit live market data; every write
(placeOrder, cancelOrder, runCommand, …) is RECORDED and returned as an intent
rather than executed. Nothing this server can do places a real order. To run a
script for real the user runs \`script:<name>\` in their own REPL.

SHARING:
\`export_scripts\` produces a portable bundle of the user's OWN scripts, for backup
or moving machines. There is deliberately no import tool: installing scripts written
by someone else requires the worker sandbox, which is not shipped yet. Until then a
third-party script would run on the same boundary as the user's own code.

Do not invent API methods. If \`validate_script\` says a method does not exist, it does not exist.`;

const server = new McpServer({ name: 'tealstreet-scripts', version: '0.1.0' }, { instructions: SERVER_INSTRUCTIONS });

type ToolResult = { content: { type: 'text'; text: string }[] };

const text = (body: string): ToolResult => ({ content: [{ type: 'text', text: body }] });

/** Mirrors the helper in services/discord-support/src/mcp.ts: the cast through
 *  `unknown` breaks the SDK's recursive zod inference, which otherwise trips a
 *  deep-instantiation error in `.tool()`. Runtime behavior is unchanged. */
function defineTool<S extends ZodRawShape>(
  name: string,
  description: string,
  shape: S,
  handler: (args: z.infer<z.ZodObject<S>>) => Promise<ToolResult>,
): void {
  const tool = server.tool.bind(server) as unknown as (
    n: string,
    d: string,
    s: ZodRawShape,
    h: (args: unknown) => Promise<ToolResult>,
  ) => void;
  tool(name, description, shape, handler as (args: unknown) => Promise<ToolResult>);
}

const formatIssues = (issues: readonly ValidationIssue[]): string =>
  issues
    .map((i) => {
      const at = i.line === undefined ? '' : ` (line ${i.line}${i.column ? `:${i.column}` : ''})`;
      return `- ${i.message}${at}`;
    })
    .join('\n');

/**
 * The full authoring gate: the shared `validate()` plus the top-level-effects
 * check. The latter is MCP-specific on purpose — top-level side effects are a
 * documented accepted risk for a human editing their own file, and a different
 * proposition when an agent writes it.
 */
const fullCheck = (source: string): ValidationIssue[] => [
  ...validate(source, { typesBundle: TYPES_BUNDLE }).errors,
  ...checkTopLevelEffects(source),
];

defineTool(
  'list_scripts',
  'List all custom scripts on disk, including any that currently fail to load.',
  {},
  async () => {
    await loadScripts();
    const loaded = new Map(listScripts().map((s) => [s.name, s]));
    // Enumerate the DIRECTORY, not the loader cache: the loader skips files
    // that throw at import, which would hide exactly the broken script someone
    // wants fixed.
    const files = listScriptFiles();
    if (files.length === 0) return text('No scripts found in ~/.tealstreet/scripts/.');

    const lines = files.map(({ name, filePath }) => {
      const script = loaded.get(name);
      if (!script) {
        return `${name}  [DOES NOT LOAD — read_script still works; validate_script will say why]\n  file: ${filePath}`;
      }
      const targets = script.targets?.join(', ') ?? 'core';
      const writes = script.capabilities?.writes?.join(', ') || 'none';
      return `${name}\n  targets: ${targets}\n  writes: ${writes}\n  file: ${filePath}`;
    });
    return text(lines.join('\n\n'));
  },
);

defineTool(
  'read_script',
  "Read one script's source. Works even when the script fails to load.",
  { name: z.string().describe('Script name without the .ts extension.') },
  async ({ name }) => {
    // Read from DISK, not the loader cache — the loader skips files that throw
    // at import, so a broken script would be unreadable through the cache, and
    // that is exactly the script someone wants to read in order to fix it.
    const source = readScriptSource(name);
    if (source === undefined) return text(`No script named '${name}'. Use list_scripts to see what exists.`);
    return text(source);
  },
);

defineTool(
  'get_api_types',
  'The complete TypeScript surface available to scripts. Read this before writing one — do not guess method names.',
  {
    target: z
      .enum(['core', 'cli', 'web', 'all'])
      .optional()
      .describe("Which tier. 'core' is what most scripts need; defaults to 'core'."),
  },
  async ({ target }) => {
    switch (target ?? 'core') {
      case 'cli':
        return text(CLI_TYPES_DTS);
      case 'web':
        return text(WEB_TYPES_DTS);
      case 'all':
        return text(TYPES_BUNDLE);
      default:
        return text(CORE_TYPES_DTS);
    }
  },
);

defineTool(
  'validate_script',
  'Typecheck a script against the real API surface and check its declared capabilities. Fast, local, free — iterate against it before writing.',
  { source: z.string().describe('Complete script source.') },
  async ({ source }) => {
    const issues = fullCheck(source);
    if (issues.length === 0) return text('VALID — no issues.');
    return text(`INVALID — ${issues.length} issue(s):\n${formatIssues(issues)}`);
  },
);

defineTool(
  'write_script',
  'Create or replace a script. Validates first and REFUSES to write invalid source.',
  {
    name: z.string().describe('Script name without .ts. Letters, digits, _ and - only.'),
    source: z.string().describe('Complete script source.'),
  },
  async ({ name, source }) => {
    if (!VALID_SCRIPT_NAME.test(name)) {
      return text(`Refused: invalid script name '${name}'. Use letters, digits, _ and - only (no dots or spaces).`);
    }

    const issues = fullCheck(source);
    if (issues.length > 0) {
      // Never write invalid source: the CLI loads this directory at startup,
      // so a broken file degrades the user's NEXT session, not just this call.
      recordMcpScriptAction(name, 'write', { ok: false, error: `refused: ${issues.length} validation issue(s)` });
      return text(`Refused to write — ${issues.length} issue(s):\n${formatIssues(issues)}`);
    }

    try {
      const filePath = await writeScript(name, source);
      recordMcpScriptAction(name, 'write', { ok: true });
      const meta = readScriptMeta(source);
      const writes = meta ? [...meta.writes].join(', ') || 'none' : 'unknown (meta not statically readable)';
      return text(`Wrote ${filePath}\nDeclared writes: ${writes}\nInvoke from the CLI with: script:${name}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      recordMcpScriptAction(name, 'write', { ok: false, error: message });
      return text(`Write failed: ${message}`);
    }
  },
);

defineTool(
  'delete_script',
  'Delete a script.',
  { name: z.string().describe('Script name without .ts.') },
  async ({ name }) => {
    try {
      const result = await deleteScript(name);
      recordMcpScriptAction(name, 'delete', { ok: result !== 'absent' });
      if (result === 'absent') return text(`No script named '${name}'.`);
      if (result === 'reseeded') {
        return text(
          `Deleted ${scriptPath(name)}, but it was immediately re-created: '${name}' is a seeded example that the CLI rewrites when missing. Rename it instead if you want it gone.`,
        );
      }
      return text(`Deleted ${scriptPath(name)}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      recordMcpScriptAction(name, 'delete', { ok: false, error: message });
      return text(`Delete failed: ${message}`);
    }
  },
);

defineTool(
  'list_accounts',
  'List configured accounts (names and exchanges only — never credentials).',
  {},
  async () => {
    const accounts = listAccountSummaries();
    if (accounts.length === 0) return text('No accounts configured.');
    return text(
      accounts
        .map((a) => `${a.name}\n  exchange: ${a.exchange}${a.testnet ? ' (testnet)' : ''}\n  connected: ${a.connected}`)
        .join('\n\n'),
    );
  },
);

const withAccount = async (account: string, read: () => string): Promise<ToolResult> => {
  const connected = await ensureConnected(account);
  if (!connected.ok) return text(connected.error);
  return text(read());
};

/** Stores hydrate asynchronously after connect, so an empty result right after
 *  a cold connect is ambiguous. Say so rather than reporting "none". */
const orEmptyNote = (body: string, what: string): string =>
  body.trim() === ''
    ? `No ${what} reported. If this account just connected, its data may still be loading — try again in a moment.`
    : body;

defineTool(
  'get_positions',
  'Open positions for an account.',
  { account: z.string().describe('Account name.'), symbol: z.string().optional() },
  async ({ account, symbol }) =>
    withAccount(account, () => {
      const store = getExchangeManager().getExchange(account)?.store;
      const positions = (store?.positions ?? []).filter((p) => !symbol || p.symbol === symbol);
      return orEmptyNote(
        positions
          .filter((p) => p.contracts !== 0)
          .map((p) => `${p.symbol} ${p.side} ${p.contracts} @ ${p.entryPrice} (upnl ${p.unrealizedPnl})`)
          .join('\n'),
        'open positions',
      );
    }),
);

defineTool(
  'get_orders',
  'Open orders for an account.',
  { account: z.string().describe('Account name.'), symbol: z.string().optional() },
  async ({ account, symbol }) =>
    withAccount(account, () => {
      const store = getExchangeManager().getExchange(account)?.store;
      const orders = (store?.orders ?? []).filter((o) => !symbol || o.symbol === symbol);
      return orEmptyNote(
        orders.map((o) => `${o.id} ${o.symbol} ${o.side} ${o.type} ${o.amount} @ ${o.price}`).join('\n'),
        'open orders',
      );
    }),
);

defineTool(
  'get_balance',
  'Balance for an account.',
  { account: z.string().describe('Account name.') },
  async ({ account }) =>
    withAccount(account, () => {
      const balance = getExchangeManager().getExchange(account)?.store?.balance;
      if (!balance) return 'No balance reported yet — the account may still be loading.';
      return `total: ${balance.total}\nfree: ${balance.free}\nused: ${balance.used}\nupnl: ${balance.upnl}`;
    }),
);

defineTool(
  'get_ticker',
  'Current ticker for a symbol.',
  { account: z.string().describe('Account name.'), symbol: z.string().describe('e.g. BTCUSDT') },
  async ({ account, symbol }) =>
    withAccount(account, () => {
      const ticker = getExchangeManager()
        .getExchange(account)
        ?.store?.tickers?.find((t) => t.symbol === symbol);
      if (!ticker) return `No ticker for '${symbol}' on '${account}'.`;
      return `${ticker.symbol}\n  last: ${ticker.last}\n  bid: ${ticker.bid}\n  ask: ${ticker.ask}\n  24h%: ${ticker.percentage}`;
    }),
);

defineTool(
  'run_script',
  'DRY RUN a script against live market data. Reads are real; every write is recorded as an intent and NEVER executed. This server cannot place real orders.',
  {
    name: z.string().describe('Script name without .ts.'),
    account: z.string().describe('Account to run against.'),
    args: z.string().optional().describe('Arguments, exactly as typed after the script name in the REPL.'),
  },
  async ({ name, account, args }) => {
    const source = readScriptSource(name);
    if (source === undefined) return text(`No script named '${name}'.`);

    // Re-gate on the full authoring check: dry run bounds an HONEST script's
    // api.* calls, but a script also executes with full Bun globals, so a
    // top-level effect or a stray import could act outside the adapter
    // entirely. Refusing to run unvalidated source is what keeps that closed.
    const issues = fullCheck(source);
    if (issues.length > 0) {
      return text(`Refused to run — the script does not validate:\n${formatIssues(issues)}`);
    }

    const connected = await ensureConnected(account);
    if (!connected.ok) return text(connected.error);

    await loadScripts();
    const record = lookupScript(name);
    if (!record) return text(`Script '${name}' is on disk but did not load. Run validate_script on it to see why.`);

    const { createStandaloneCoreAdapter } = await import('../script-adapter');
    const { runScriptFromHostWithAdapter } = await import('../script-runner');
    const { getCliTranspile } = await import('../bun-transpile');
    const { createStandalonePlatformAdapter } = await import('../platform-adapter');

    const manager = getExchangeManager();
    const session: CLISession = {
      currentExchange: manager.getExchange(account) ?? null,
      currentAccountName: account,
      currentSymbol: '',
    };
    const real = createStandaloneCoreAdapter(manager, session, createStandalonePlatformAdapter());
    const { adapter, intents, output } = createDryRunAdapter(real);

    let error: string | undefined;
    try {
      const result = await runScriptFromHostWithAdapter(
        { adapter, transpile: getCliTranspile() },
        { record, rawArgs: args ?? '', invokedFrom: 'cli' },
      );
      if (!result.ok) error = result.error;
      for (const line of result.output ?? []) output.push(typeof line === 'string' ? line : line.text);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }

    recordMcpScriptAction(name, 'dry-run', {
      origin: account,
      ok: !error,
      detail: intents.length === 0 ? 'no write intents' : `${intents.length} write intent(s)`,
      ...(error ? { error } : {}),
    });

    const lines = [
      `DRY RUN of '${name}' against '${account}' — nothing was executed.`,
      '',
      intents.length === 0
        ? 'Writes attempted: none.'
        : `Writes attempted (${intents.length}):\n${intents.map((i) => `  ${i.method} [${i.account}] ${i.detail}`).join('\n')}`,
    ];
    if (output.length > 0) lines.push('', `Output:\n${output.map((o) => `  ${o}`).join('\n')}`);
    if (error) lines.push('', `Script threw: ${error}`);
    return text(lines.join('\n'));
  },
);

defineTool(
  'export_scripts',
  'Export your scripts as a portable JSON bundle (backup, or moving to another machine).',
  {
    names: z.array(z.string()).optional().describe('Specific scripts to export. Omit to export everything on disk.'),
  },
  async ({ names }) => {
    const wanted = new Set(names ?? []);
    const files = listScriptFiles().filter((f) => wanted.size === 0 || wanted.has(f.name));
    if (files.length === 0) return text('Nothing to export.');

    const scripts = files
      .map(({ name }) => ({ name, source: readScriptSource(name) }))
      .filter((entry): entry is { name: string; source: string } => entry.source !== undefined)
      .map(({ name, source }) => {
        const meta = readScriptMeta(source);
        return {
          name,
          source,
          // Recorded for the reader's benefit only. On import the capabilities
          // are re-read from the SOURCE — a bundle field could be edited to
          // understate what the code does.
          declaredWrites: meta ? [...meta.writes] : null,
        };
      });

    return text(
      JSON.stringify(
        { format: 'tealstreet-scripts', version: 1, exportedAt: new Date().toISOString(), scripts },
        null,
        2,
      ),
    );
  },
);

/**
 * Start the stdio server.
 *
 * stdio MCP requires a pristine stdout — it carries JSON-RPC and nothing else.
 * The CLI's own startup is chatty (the update notifier prints a chalk box
 * asynchronously via console.log), so the caller MUST have suppressed that
 * before reaching here. See `installStdoutGuard`.
 */
export const startMcpServer = async (): Promise<void> => {
  // Refuse to start unauthenticated rather than letting every account-touching
  // tool fail separately. An MCP client surfaces per-tool errors as model-facing
  // text, so the agent would burn turns retrying instead of telling the user to
  // log in. stderr, never stdout — stdout is the JSON-RPC channel.
  if (!isLoggedIn()) {
    // `login` is a REPL command, not a top-level one — so the instruction is
    // to launch the CLI and type it, not to run `tealstreet login`.
    process.stderr.write('tealstreet mcp: not logged in. Start the CLI (`tealstreet`) and run `login` first.\n');
    process.exit(1);
  }

  // The guard has redirected `process.stdout.write` to stderr, so the
  // transport must be handed the captured original — otherwise it would frame
  // JSON-RPC onto stderr and the client would see nothing at all.
  const protocolStdout = getProtocolStdout();
  const transport = protocolStdout
    ? new StdioServerTransport(process.stdin, protocolStdout as unknown as NodeJS.WriteStream)
    : new StdioServerTransport();
  await server.connect(transport);

  // A closed session must not leave websockets reconnecting forever in a
  // process nobody is watching.
  const shutdown = () => {
    void disconnectAll().finally(() => process.exit(0));
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
  process.stdin.on('close', shutdown);
};
