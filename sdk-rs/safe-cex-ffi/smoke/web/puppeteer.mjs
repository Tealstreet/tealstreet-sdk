import { createRequire } from 'node:module';

// Resolve the browser driver from the workspace that declares the escape suite.
const require = createRequire(new URL('../../../../apps/module-frame/package.json', import.meta.url));
const { version } = require('puppeteer/package.json');
const [major, minor] = version.split('.').map(Number);
if (major !== 25 || minor < 12) {
  throw new Error(`FFI smoke requires module-frame's Puppeteer ^25.12.0; resolved ${version}. Run yarn install --frozen-lockfile.`);
}
export default require('puppeteer').default;
