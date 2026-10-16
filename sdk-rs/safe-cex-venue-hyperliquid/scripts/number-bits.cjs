// Capture policy: NaN has no language-observable sign or payload; every other bit remains exact.
module.exports = value => {
 if (Number.isNaN(value)) return '7ff8000000000000';
 const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');
};
