const path = require('node:path');
// The environment points at the compiler package directory, not its resolved
// realpath: symlinks must not move the runtime dependency boundary elsewhere.
module.exports = name => require(process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '..', name)
  : name);
