export const decompress = async (byteArray: ArrayBuffer, encoding = 'gzip' as CompressionFormat): Promise<string> => {
  const cs = new DecompressionStream(encoding);
  const writer = cs.writable.getWriter();

  // Kick off write+close but DON'T await before draining the readable: awaiting
  // close() first would deadlock on backpressure for large payloads (the
  // transform stalls once the readable buffer fills). We still await these
  // promises (via the Promise.all below) so a write/close error surfaces as a
  // rejected promise instead of an unhandled rejection.
  const writeDone = (async () => {
    await writer.write(new Uint8Array(byteArray));
    await writer.close();
  })();

  // Read decompressed chunks from the readable stream
  const reader = cs.readable.getReader();
  const chunks: Uint8Array[] = [];
  let totalLength = 0;

  const readDone = (async () => {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      totalLength += value.length;
    }
  })();

  // Observe both sides so neither can produce an unhandled rejection.
  await Promise.all([writeDone, readDone]);

  // Combine chunks into single ArrayBuffer
  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }

  return new TextDecoder().decode(result);
};
