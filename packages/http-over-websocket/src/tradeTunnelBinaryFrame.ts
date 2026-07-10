import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';

const BINARY_RESPONSE_MAGIC = new Uint8Array([0x54, 0x53, 0x42, 0x31]); // TSB1
const BINARY_RESPONSE_PREFIX_BYTES = 8;
const MAX_BINARY_HEADER_BYTES = 1024 * 1024;

async function toArrayBuffer(data: unknown): Promise<ArrayBuffer> {
  if (data instanceof ArrayBuffer) {
    return data;
  }

  if (typeof Blob !== 'undefined' && data instanceof Blob) {
    return data.arrayBuffer();
  }

  if (ArrayBuffer.isView(data)) {
    const view = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
    return view.slice().buffer;
  }

  throw new TypeError('Unsupported TradeTunnel binary message type');
}

export async function decodeTradeTunnelMessage(data: unknown): Promise<any> {
  if (typeof data === 'string') {
    return jsonParse(data);
  }

  const bytes = new Uint8Array(await toArrayBuffer(data));
  if (bytes.length < BINARY_RESPONSE_PREFIX_BYTES) {
    throw new Error('TradeTunnel binary frame is shorter than its prefix');
  }

  for (let index = 0; index < BINARY_RESPONSE_MAGIC.length; index += 1) {
    if (bytes[index] !== BINARY_RESPONSE_MAGIC[index]) {
      throw new Error('TradeTunnel binary frame has an invalid magic value');
    }
  }

  const headerLength = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(4, false);
  if (headerLength === 0 || headerLength > MAX_BINARY_HEADER_BYTES) {
    throw new Error('TradeTunnel binary frame has an invalid header length');
  }

  const payloadOffset = BINARY_RESPONSE_PREFIX_BYTES + headerLength;
  if (payloadOffset >= bytes.length) {
    throw new Error('TradeTunnel binary frame is missing its payload');
  }

  const headerText = new TextDecoder('utf-8', { fatal: true }).decode(
    bytes.subarray(BINARY_RESPONSE_PREFIX_BYTES, payloadOffset),
  );
  const message = jsonParse(headerText);
  if (
    message?.type !== 'rpc.http' ||
    typeof message?.nonce !== 'string' ||
    !message?.data ||
    message.data.data_encoding !== 'gzip'
  ) {
    throw new Error('TradeTunnel binary frame has an invalid response header');
  }

  message.data.data_binary = bytes.slice(payloadOffset).buffer;
  return message;
}
