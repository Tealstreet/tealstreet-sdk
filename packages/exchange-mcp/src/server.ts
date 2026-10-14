import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import type { ServerConfig } from './config';

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { createExchange } from '@tealstreet/safe-cex/src/index';

import { supportedTools } from './tools';

/** Starts the exchange once, and again on the next call if starting failed. */
const startOnce = (exchange: Exchange): (() => Promise<Exchange>) => {
  let started: Promise<Exchange> | undefined;
  return () =>
    (started ??= exchange.start().then(
      () => exchange,
      (err: unknown) => {
        started = undefined;
        throw err;
      },
    ));
};

export const createServer = async ({ exchange: name, options }: ServerConfig, version: string) => {
  const exchange: Exchange = await createExchange(name, options);
  exchange.disablePublicWebsockets();
  const ready = startOnce(exchange);

  const server = new McpServer({ name: `exchange-mcp-${name}`, version });
  for (const tool of supportedTools(exchange, { options: Boolean(options.enableOptionMarkets) })) {
    server.registerTool(tool.name, { description: tool.description, inputSchema: tool.input }, async (args) => {
      const result = await tool.run(await ready(), args as never);
      return { content: [{ type: 'text', text: JSON.stringify(result) }] };
    });
  }
  return { server, exchange, ready };
};
