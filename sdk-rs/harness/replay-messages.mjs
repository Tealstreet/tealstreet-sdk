import { socketMessages } from '../hosts/wasm/js/host.mjs';

// Retain the production decoder and synchronous text dispatch, but give the
// replay fixture a completion receipt for every input, including queued text.
export function replaySocketMessages(onMessage) {
  const queued = [];
  let failed = false;
  let failure;
  const fail = error => {
    failed = true;
    failure = error;
    for (const receipt of queued.splice(0)) receipt.reject(error);
  };
  const receive = socketMessages(value => {
    if (failed) return;
    const receipt = queued.shift();
    try {
      const result = onMessage(value);
      Promise.resolve(result).then(receipt.resolve, error => {
        receipt.reject(error);
        fail(error);
      });
      return result;
    } catch (error) {
      receipt.reject(error);
      fail(error);
      throw error;
    }
  }, fail);
  return data => new Promise((resolve, reject) => {
    if (failed) { reject(failure); return; }
    queued.push({ resolve, reject });
    try { receive(data); } catch (error) { fail(error); }
  });
}
