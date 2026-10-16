// Live capture provenance is recorded beside probe values; validated Node 26.7.0 on darwin/arm64.
export const captureNodeMajor = Number(process.versions.node.split('.')[0]);
export const captureArchitecture = `${process.platform}/${process.arch}`;
export function captureBits(value) {
  if (Number.isNaN(value)) return '7ff8000000000000';
  const bytes = new DataView(new ArrayBuffer(8));
  bytes.setFloat64(0, value);
  return bytes.getBigUint64(0).toString(16).padStart(16, '0');
}
