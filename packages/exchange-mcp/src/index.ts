import './stdio';

import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';

import pkg from '../package.json';
import { readConfig } from './config';
import { createServer } from './server';
import { registerBundledRustEngine } from './rust-engine';

registerBundledRustEngine();
const { server, exchange, ready } = await createServer(readConfig(process.env), pkg.version);
await server.connect(new StdioServerTransport());
// Warm up so the first tool call does not wait for markets; a failure here is retried on that call.
ready().catch(() => undefined);

const stop = () => {
  exchange.dispose();
  process.exit(0);
};
process.stdin.on('close', stop);
process.on('SIGTERM', stop);
