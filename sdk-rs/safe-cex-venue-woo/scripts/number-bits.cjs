// NaN payload/sign is not a source assertion; every other Number keeps its bits.
module.exports = function numberBits(value) {
  if (Number.isNaN(value)) return '7ff8000000000000';
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
