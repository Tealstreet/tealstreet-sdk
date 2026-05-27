import { inflate } from 'pako';

export const readBlob = async (blob: Blob): Promise<string> => {
  const buffer = await blob.arrayBuffer();

  // Turn number array into byte-array
  const binData = new Uint8Array(buffer);

  // Pako
  const inflatedData = inflate(binData);

  const uint16Array = new Uint16Array(inflatedData) as any as number[];

  // Convert gunzipped byteArray back to ascii string:
  return uint16Array.reduce((acc, i) => (acc += String.fromCharCode.apply(null, [i])), '');
};
