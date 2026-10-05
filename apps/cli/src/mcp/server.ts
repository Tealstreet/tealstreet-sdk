import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z, type ZodRawShape } from 'zod';

import { checkTopLevelEffects, readScriptMeta, validate, type ValidationIssue } from '@tealstreet/script-authoring';

import { recordMcpScriptAction } from '../audit-log';
import { isLoggedIn } from '../auth';
import { CORE_TYPES_DTS, WEB_TYPES_DTS } from '@tealstreet/script-authoring/src/script-types-bundle';
import { SCRIPT_TYPES_BUNDLE } from '@tealstreet/script-authoring/src/scripts-templates';
import { RUNTIME_SURFACES, getAllowedApiPathsForTargets, type TargetName } from '@tealstreet/script-api-core';
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
import { CORE_READ_BINDINGS } from '@tealstreet/script-api-core/src/runtime/readBindings';
import { createDryRunAdapter } from './dryRunAdapter';
import { CLI_READ_TOOLS, readToolShape, renderReadResult } from './readTools';
import { cliDataTools } from './dataTools';
import { disconnectAll, ensureConnected, getExchangeManager, listAccountSummaries } from './liveState';
import { getProtocolStdout } from './stdoutGuard';
import type { CLISession } from '@tealstreet/cli-core';

/** The exact text seeded into the user's `types.d.ts` — see
 *  `SCRIPT_TYPES_BUNDLE`. An agent must not author against a different
 *  surface than the one in the user's editor. */
const TYPES_BUNDLE = SCRIPT_TYPES_BUNDLE;

const SERVER_INSTRUCTIONS = `tealstreet-scripts — author and inspect Tealstreet custom scripts, local account state, and Tealstreet data.

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
- \`meta.targets\` is a SUPPORT SET. Use \`['core', 'cli']\` for CLI-only,
  \`['core', 'web']\` for web-only, \`['core', 'mobile']\` for mobile-only, or include
  several runtimes when the script is intentionally cross-target. Multi-target scripts
  may only use the API paths every declared runtime actually provides.
- Mobile is read-only today: no \`api.exchange.*\`, no \`api.exchange.runCommand\`,
  no storage, no candles, no historical positions.
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

DATA TOOLS:
The public market-data tools read Tealstreet publicdata, exchange-services candles, and
public DEX routes over outbound HTTPS. Hyperliquid analytics tools read Tealstreet's
crawled analytics database when the local process has HYPERLIQUID_ANALYTICS_DATABASE_URL
or HYPERLIQUID_ANALYTICS_POSTGRES_* configured. These tools are read-only and do not use
exchange API credentials.

SHARING:
\`export_scripts\` produces a portable bundle of the user's OWN scripts, for backup
or moving machines. There is deliberately no import tool: installing scripts written
by someone else requires the worker sandbox, which is not shipped yet. Until then a
third-party script would run on the same boundary as the user's own code.

Do not invent API methods. If \`validate_script\` says a method does not exist, it does not exist.`;

const server = new McpServer({ name: 'tealstreet-scripts', version: '0.1.0' }, { instructions: SERVER_INSTRUCTIONS });

type ToolResult = { content: { type: 'text'; text: string }[] };

const text = (body: string): ToolResult => ({ content: [{ type: 'text', text: body }] });

/** The cast through `unknown` breaks the SDK's recursive zod inference, which
 *  otherwise trips a deep-instantiation error in `.tool()`. Runtime behavior is
 *  unchanged. */
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

const formatApiPaths = (paths: readonly string[]): string =>
  paths
    .slice()
    .sort()
    .map((path) => `- ${path}`)
    .join('\n');

const parseTargets = (target: string | undefined): readonly TargetName[] => {
  switch (target ?? 'core') {
    case 'cli':
      return ['core', 'cli'];
    case 'web':
      return ['core', 'web'];
    case 'mobile':
      return ['core', 'mobile'];
    case 'all':
      return ['core', 'web', 'mobile', 'cli'];
    default:
      return ['core'];
  }
};

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

for (const tool of cliDataTools) {
  defineTool(tool.name, tool.description, tool.shape, tool.handler);
}

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
      .enum(['core', 'cli', 'web', 'mobile', 'all'])
      .optional()
      .describe("Which target view. 'core' is the portable intersection; defaults to 'core'."),
  },
  async ({ target }) => {
    switch (target ?? 'core') {
      case 'cli':
        return text(
          [
            'The CLI has no extension package; it runs the CoreApi subset below.',
            'CLI runtime API paths:',
            formatApiPaths(RUNTIME_SURFACES.cli.apiPaths),
          ].join('\n'),
        );
      case 'web':
        return text([WEB_TYPES_DTS, '', 'Web runtime API paths:', formatApiPaths(RUNTIME_SURFACES.web.apiPaths)].join('\n'));
      case 'mobile':
        return text(
          [
            "Mobile has no extension package yet; it runs the CoreApi subset below.",
            'Mobile runtime API paths:',
            formatApiPaths(RUNTIME_SURFACES.mobile.apiPaths),
          ].join('\n'),
        );
      case 'all':
        return text(
          [
            TYPES_BUNDLE,
            '',
            "Portable web+mobile+CLI intersection for meta.targets ['core', 'web', 'mobile', 'cli']:",
            formatApiPaths([...getAllowedApiPathsForTargets(parseTargets('all'))]),
          ].join('\n'),
        );
      default:
        return text(
          [
            CORE_TYPES_DTS,
            '',
            'Portable core API paths available across web, mobile and CLI:',
            formatApiPaths([...getAllowedApiPathsForTargets(parseTargets('core'))]),
          ].join('\n'),
        );
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

/**
 * Live account reads, GENERATED from the SDK read manifest.
 *
 * These used to reach into `getExchangeManager().getExchange(a)?.store` and
 * hand-format a string — while `createDryRunAdapter`, in the file beside them,
 * correctly wrapped `CoreAdapter`. The bypass and the sanctioned path shipped
 * in the same directory, and the result was that this surface answered "what
 * are my positions" from a different source, in a different shape, under a
 * different name than the script API and the web agent did.
 *
 * Definitions now come from `readBindings.ts`; only the names and the
 * rendering are ours. See `docs/plans/SDK_SURFACE_VISION_v1.md`.
 *
 * A fresh adapter per call is cheap: `createCoreAdapter` returns a plain object
 * and registers nothing, and the orderbook cache is keyed on the exchange
 * manager, so per-call adapters share subscriptions rather than duplicating
 * them. No `platformAdapter` is passed — a read has no business printing, and
 * without one a stray `print` throws loudly instead of corrupting the stdio
 * transport.
 */
const sessionFor = (account: string): CLISession => ({
  currentExchange: getExchangeManager().getExchange(account) ?? null,
  currentAccountName: account,
  currentSymbol: '',
});

for (const [toolName, method] of Object.entries(CLI_READ_TOOLS)) {
  const binding = CORE_READ_BINDINGS[method];
  defineTool(toolName, binding.summary, readToolShape(method), async (args) => {
    const account = String((args as { account?: unknown }).account ?? '');
    // Still gated on `ensureConnected` first: an adapter over a disconnected
    // account answers empty, which reads as "you hold nothing".
    const connected = await ensureConnected(account);
    if (!connected.ok) return text(connected.error);

    const { createStandaloneCoreAdapter } = await import('../script-adapter');
    const adapter = createStandaloneCoreAdapter(getExchangeManager(), sessionFor(account));
    try {
      return text(renderReadResult(method, await binding.invoke(adapter, args as Record<string, unknown>)));
    } catch (error) {
      return text(`${toolName} failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  });
}

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
