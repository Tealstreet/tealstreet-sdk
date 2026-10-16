// Capture architecture is recorded per row by each caller; only NaN is canonical.
function bits(number) {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, number);
  if (Number.isNaN(view.getFloat64(0))) return '7ff8000000000000';
  return view.getBigUint64(0).toString(16).padStart(16, '0');
}
module.exports = {bits};
if (require.main === module) {
  const view = new DataView(new ArrayBuffer(8));
  const patterns = ['7ff8000000000000', 'fff8000000000000', '7ff0000000000001', '8000000000000000', '0000000000000000', '3ff8000000000000', '7ff0000000000000', 'fff0000000000000'];
  console.log(JSON.stringify(patterns.map(input => {
    view.setBigUint64(0, BigInt('0x' + input));
    return {input, bits: bits(view.getFloat64(0)), captureArchitecture: process.arch, captureNodeMajor: Number(process.versions.node.split(".")[0])};
  })));
}
