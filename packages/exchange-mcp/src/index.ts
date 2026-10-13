import './stdio';

import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';

import pkg from '../package.json';
import { readConfig } from './config';
import { createServer } from './server';

const { server, exchange } = await createServer(readConfig(process.env), pkg.version);
await server.connect(new StdioServerTransport());

const stop = () => {
  exchange.dispose();
  process.exit(0);
};
process.stdin.on('close', stop);
process.on('SIGTERM', stop);
