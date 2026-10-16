// Live captures label their actual host; NaN alone has a canonical payload.
process.stderr.write(JSON.stringify({captureArchitecture:process.arch, platform:process.platform, node:process.version, nanBits:'7ff8000000000000'})+'\n');
function bits(value) {
  if (Number.isNaN(value)) return '7ff8000000000000';
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(value);
  return buffer.toString('hex');
}
module.exports = {bits};
