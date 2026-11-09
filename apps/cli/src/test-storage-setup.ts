// No CLI test may accidentally load or modify the developer's CLI profile.
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll } from 'vitest';
const root = mkdtempSync(join(tmpdir(), 'cli-test-profile-'));
process.env.TEALSTREET_CONFIG_DIR = join(root, 'tealstreet');
process.env.TEALSTREET_CONFIG_FILE = join(root, 'tealstreet/config.json');
process.env.DISRUPT_CONFIG_DIR = join(root, 'disrupt');
process.env.DISRUPT_CONFIG_FILE = join(root, 'disrupt/config.json');
afterAll(() => rmSync(root, { recursive: true, force: true }));
