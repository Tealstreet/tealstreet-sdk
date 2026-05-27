export const decompress = async (byteArray: ArrayBuffer, encoding = 'gzip' as CompressionFormat): Promise<string> => {
  const cs = new DecompressionStream(encoding);
  const writer = cs.writable.getWriter();
  writer.write(new Uint8Array(byteArray));
  writer.close();

  // Read decompressed chunks from the readable stream
  const reader = cs.readable.getReader();
  const chunks: Uint8Array[] = [];
  let totalLength = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    totalLength += value.length;
  }

  // Combine chunks into single ArrayBuffer
  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }

  return new TextDecoder().decode(result);
};
