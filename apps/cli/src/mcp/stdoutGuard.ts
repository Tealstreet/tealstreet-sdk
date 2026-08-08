import { Writable } from 'node:stream';

/**
 * Keep stdout pristine for the MCP stdio transport.
 *
 * stdio MCP frames JSON-RPC over stdout. ANY other byte corrupts the protocol,
 * and the failure mode is miserable: the client reports a parse error with no
 * indication of which stray write caused it, often minutes into a session.
 *
 * Patching `console.log` alone is NOT sufficient, which is the whole reason
 * this file is shaped the way it is. Every script tool calls `loadScripts()`,
 * which dynamic-imports every script already on disk — files written before
 * this server existed, which `checkTopLevelEffects` never saw. Such a script
 * can reach stdout through `console.table`, `console.dir`, `console.group`, or
 * `process.stdout.write` directly.
 *
 * So the guard replaces `process.stdout.write` itself — the one chokepoint all
 * of those funnel through — and hands the captured original to the transport.
 * Everything else in the process can write freely; it just lands on stderr.
 *
 * `console.warn` / `console.error` are left alone: they already go to stderr,
 * which MCP clients treat as diagnostics, and silencing them would discard the
 * server's only debugging signal.
 */

type StdoutWrite = typeof process.stdout.write;

let installed: { protocolStdout: Writable } | null = null;

/**
 * Redirect everything that would reach stdout to stderr, and return a stream
 * that still writes to the REAL stdout. Pass that stream to the transport so
 * JSON-RPC — and only JSON-RPC — reaches the client.
 */
export const installStdoutGuard = (): { protocolStdout: Writable } => {
  if (installed) return installed;

  const originalWrite = process.stdout.write.bind(process.stdout) as StdoutWrite;

  // The transport's private channel to the real stdout.
  const protocolStdout = new Writable({
    write(chunk, encoding, callback) {
      try {
        originalWrite(chunk as string | Uint8Array, encoding as BufferEncoding, callback);
      } catch (error) {
        callback(error as Error);
      }
    },
  });

  // Everything else in the process now writes to stderr instead.
  const redirect = ((chunk: unknown, encoding?: unknown, callback?: unknown) => {
    const cb = typeof encoding === 'function' ? encoding : callback;
    try {
      process.stderr.write(chunk as string | Uint8Array);
    } catch {
      // stderr failing is not worth crashing the server over.
    }
    if (typeof cb === 'function') (cb as () => void)();
    return true;
  }) as StdoutWrite;

  process.stdout.write = redirect;

  installed = { protocolStdout };
  return installed;
};

/** The transport stream, once the guard is installed. */
export const getProtocolStdout = (): Writable | null => installed?.protocolStdout ?? null;

/**
 * Whether this process was invoked as the MCP server.
 *
 * Checks argv[2] exactly rather than `includes('mcp')`: several commands take
 * `-a, --account <name>`, so a user with an account literally named `mcp`
 * would otherwise have `tealstreet buy BTC 1 -a mcp` silently redirect all its
 * stdout to stderr — breaking `>` redirection and `| jq` for a normal command.
 */
export const isMcpInvocation = (argv: readonly string[] = process.argv): boolean => argv[2] === 'mcp';
