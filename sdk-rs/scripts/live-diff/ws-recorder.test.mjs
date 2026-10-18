import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import WebSocket, { WebSocketServer } from 'ws';
import { makeCA } from './recorder.mjs';
import { buildConnectHook, connectHookEnvironment, websocketRecorder } from './ws-recorder.mjs';

for (const port of [443, 8443, 9443])
  test(`child-only socket hook records TLS on ${port} and replays exact text/binary`, { timeout: 20000 }, async (t) => {
    const directory = mkdtempSync(`${tmpdir()}/live-ws-test-`);
    const ca = makeCA(directory);
    const hook = buildConnectHook(directory, fileURLToPath(new URL('./ws-connect.c', import.meta.url)));
    const upstream = new WebSocketServer({ host: '127.0.0.1', port: 0 });
    await new Promise((resolve) => upstream.once('listening', resolve));
    let realSubscriptions = 0;
    upstream.on('connection', (socket) =>
      socket.once('message', () => {
        realSubscriptions++;
        socket.send('{"last":42}');
        socket.send(Buffer.from([0, 255, 128, 1]), { binary: true });
      }),
    );
    const streams = new Map(),
      records = [],
      relays = [];
    t.after(async () => {
      for (const relay of relays) await relay.close();
      await new Promise((resolve) => upstream.close(resolve));
      rmSync(directory, { recursive: true, force: true });
    });
    async function run(implementation) {
      const relay = await websocketRecorder({
        directory,
        ca,
        hosts: ['public.exchange.test'],
        implementation,
        streams,
        records,
        openUpstream: () => new WebSocket(`ws://127.0.0.1:${upstream.address().port}`),
      });
      relays.push(relay);
      const script = `
      const WebSocket = require('ws');
      const frames = [];
      const socket = new WebSocket('wss://public.exchange.test:${port}/stream?requestId=${implementation}', {
        lookup: (_name, options, callback) => options.all
          ? callback(null, [{address:'${implementation === 'rust' ? '::2' : '127.0.0.2'}',family:${implementation === 'rust' ? 6 : 4}}])
          : callback(null,'${implementation === 'rust' ? '::2' : '127.0.0.2'}',${implementation === 'rust' ? 6 : 4}),
      });
      socket.on('open', () => socket.send('subscribe'));
      socket.on('message', (bytes, binary) => {
        frames.push({base64:bytes.toString('base64'),binary});
        if(frames.length===2){ console.log(JSON.stringify(frames)); socket.close(); }
      });
      socket.on('error', error => {console.error(error.message);process.exit(1)});
    `;
      const result = await new Promise((resolve, reject) => {
        const child = spawn(process.execPath, ['-e', script], {
          env: { ...process.env, ...connectHookEnvironment(hook, relay.ports), NODE_EXTRA_CA_CERTS: ca.cert },
          stdio: ['ignore', 'pipe', 'pipe'],
        });
        let stdout = '',
          stderr = '';
        child.stdout.on('data', (bytes) => (stdout += bytes));
        child.stderr.on('data', (bytes) => (stderr += bytes));
        child.on('exit', (code) => (code === 0 ? resolve(JSON.parse(stdout)) : reject(new Error(stderr))));
      });
      return { result, relay };
    }
    const ts = await run('ts');
    ts.relay.freeze();
    const rust = await run('rust');
    assert.deepEqual(rust.result, ts.result, 'the parser-facing bytes and binary flags are identical');
    assert.equal(realSubscriptions, 1, 'replay never opens a second upstream data stream');
    assert.equal(records[0].inboundMessages, 2);
    assert.equal(records[1].frameHash, records[0].frameHash);
  });
