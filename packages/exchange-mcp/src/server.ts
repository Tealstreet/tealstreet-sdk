import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import type { ServerConfig } from './config';

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { createExchange } from '@tealstreet/safe-cex/src/index';
import { RustEngineExchange } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

import { supportedTools } from './tools';

export const createServer = async ({ exchange: name, options }: ServerConfig, version: string) => {
  const instantiate = async (): Promise<Exchange> => {
    const next = await createExchange(name, options);
    if (next instanceof RustEngineExchange && options.payloadSigner) {
      next.dispose();
      throw new Error('Rust MCP accounts require local credentials; EXCHANGE_SIGNER_HEADER is unsupported');
    }
    // Native reads return current engine state; its public feed keeps quotes
    // fresh. TS adapters instead fetch on demand without their public sockets.
    if (!(next instanceof RustEngineExchange)) {
      next.options = { ...next.options, extra: { tickInterval: Number.POSITIVE_INFINITY, ...next.options.extra } };
      next.disablePublicWebsockets();
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
  for (const tool of supportedTools(exchange, { options: Boolean(options.enableOptionMarkets) })) {
    server.registerTool(tool.name, { description: tool.description, inputSchema: tool.input }, async (args) => {
      const result = await tool.run(await ready(), args as never);
      return { content: [{ type: 'text', text: JSON.stringify(result) }] };
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
