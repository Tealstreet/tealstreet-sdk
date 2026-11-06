import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import type { ServerConfig } from './config';

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { createExchange } from '@tealstreet/safe-cex/src/index';
import { historyCompletenessOf } from '@tealstreet/safe-cex/src/historyCompleteness';
import { RustEngineExchange } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { deriveRemoteSigningPolicy } from '@tealstreet/safe-cex/src/exchanges/rust/construction';

import { supportedTools } from './tools';

export const createServer = async ({ exchange: name, options }: ServerConfig, version: string) => {
  const instantiate = async (): Promise<Exchange> => {
    const next = await createExchange(name, options);
    const delegated = deriveRemoteSigningPolicy(name);
    if (next instanceof RustEngineExchange && options.payloadSigner && !(delegated?.enabled && delegated.payloadSupported)) {
      next.dispose();
      throw new Error('Rust MCP accounts require local credentials; EXCHANGE_SIGNER_HEADER is unsupported');
    }
    return next;
  };
  let exchange = await instantiate();
  let started: Promise<Exchange> | undefined;
  let replace = false;
  const ready = (): Promise<Exchange> =>
    (started ??= Promise.resolve()
      .then(async () => {
        // Native startup owns one engine lifetime. Retrying it requires a fresh
        // account; registered tool callbacks always await the current instance.
        if (replace) {
          exchange.dispose();
          exchange = await instantiate();
          replace = false;
        }
        try {
          await exchange.start();
          return exchange;
        } catch (error) {
          replace = exchange instanceof RustEngineExchange;
          throw error;
        }
      })
      .catch((error: unknown) => {
        started = undefined;
        throw error;
      }));

  const server = new McpServer({ name: `exchange-mcp-${name}`, version });
  // A Rust record needs its engine. A failed load surfaces on the first tool
  // call, which retries with a fresh account; tools then use the base answers.
  await exchange.loadCapabilities().catch(() => undefined);
  for (const tool of supportedTools(exchange, { options: Boolean(options.enableOptionMarkets) })) {
    server.registerTool(tool.name, { description: tool.description, inputSchema: tool.input }, async (args, extra) => {
      const result = await tool.run(await ready(), args as never);
      const completeness = historyCompletenessOf(result);
      const serialized = completeness ? { requestId: extra.requestId, rows: result, completeness } : result;
      return { content: [{ type: 'text', text: JSON.stringify(serialized) }] };
    });
  }
  return {
    server,
    get exchange() {
      return exchange;
    },
    ready,
  };
};
