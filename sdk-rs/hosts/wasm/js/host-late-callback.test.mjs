import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

for (const event of ['open', 'message', 'binary', 'close', 'unsupported binary']) {
  for (const retire of [false, true]) {
    test(`${event}: socket observer ${retire ? 'retirement blocks later engine reads' : 'preserves healthy native delivery'}`, () => {
      const calls = [];
      const reads = [];
      let fenced = false;
      let armed = false;
      let callbacks;
      let host;
      const method = (name) => () => { calls.push(name); return ''; };
      const engine = new Proxy({
        wsOpen: method('open'),
        wsMessage: method('message'),
        wsClose: method('close'),
        ...(event === 'unsupported binary' ? {} : { wsBinary: method('binary') }),
      }, {
        get(target, property, receiver) {
          if (fenced) reads.push(String(property));
          return Reflect.get(target, property, receiver);
        },
      });
      host = new EngineHost(engine, {
        now: () => 42,
        connect(_url, value) {
          callbacks = value;
          return { close() { callbacks.onClose(1000); } };
        },
      }, {
        onSocketChange() {
          if (!armed || !retire) return;
          host.failEngine(new WebAssembly.RuntimeError('observer retired engine'));
          fenced = true;
        },
      });
      host.execute({ op: 'wsConnect', socket: 7, url: 'wss://observer.invalid', channel: 'private' });
      if (event !== 'open') callbacks.onOpen();
      if (event === 'unsupported binary') callbacks.onMessage('initialize healthy stream');
      calls.length = 0;
      armed = true;
      if (event === 'open') callbacks.onOpen();
      else if (event === 'message') callbacks.onMessage('native text');
      else if (event.includes('binary')) callbacks.onBinary(new Uint8Array([1]));
      else callbacks.onClose(1006);

      assert.equal(host.disposed, retire);
      assert.equal(host.dead, retire);
      assert.deepEqual(calls, retire ? [] : [event === 'unsupported binary' ? 'close' : event]);
      assert.deepEqual(reads, [], 'observer-fenced continuation must check disposal before engine property lookup');
      host.dispose();
    });
  }
}
