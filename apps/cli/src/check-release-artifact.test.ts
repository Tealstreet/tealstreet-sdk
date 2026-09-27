import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

const SCANNER = join(__dirname, '..', 'scripts', 'check-release-artifact.cjs');
const dir = mkdtempSync(join(tmpdir(), 'artifact-scan-'));

/** Write a fake binary: NUL-delimited so the scanner's string extractor sees one chunk. */
const artifact = (name: string, ...chunks: string[]): string => {
  const file = join(dir, name);
  writeFileSync(file, Buffer.concat([Buffer.from([0]), Buffer.from(chunks.join('\0')), Buffer.from([0])]));
  return file;
};

const scan = (file: string): { code: number; out: string } => {
  try {
    execFileSync(process.execPath, [SCANNER, file], { encoding: 'utf8', stdio: 'pipe' });
    return { code: 0, out: '' };
  } catch (error) {
    const e = error as { status?: number; stderr?: string };
    return { code: e.status ?? -1, out: e.stderr ?? '' };
  }
};

/**
 * This scanner is the only thing standing between a leaked credential and a
 * published binary, and it had no test. It also has one hard constraint that
 * is easy to regress: it reads WHOLE minified chunks, hundreds of KB each, so
 * both its false-positive rate and its reporting have to survive that scale.
 */
describe('check-release-artifact', () => {
  it('passes a clean artifact', () => {
    expect(scan(artifact('clean.bin', 'hello world, nothing to see here')).code).toBe(0);
  });

  it('still catches real credentials', () => {
    expect(scan(artifact('gh.bin', 'ghp_abcdefghijklmnopqrstuvwxyz0123456789')).out).toContain('GitHub token');
    expect(scan(artifact('npm.bin', 'npm_abcdefghijklmnopqrstuvwxyz0123456789')).out).toContain('npm token');
    expect(scan(artifact('pw.bin', 'DATABASE_PASSWORD="hunter2Correct-HorseBattery_Staple99"')).out).toContain(
      'secret assignment',
    );
    expect(scan(artifact('src.bin', 'at /Users/someone/apps/cli/src/index.ts:12')).out).toContain('CLI source path');
  });

  /**
   * OpenTelemetry (bundled via @sentry/bun) ships hundreds of constants shaped
   * `AWS_SECRETSMANAGER_SECRET_ARN = "aws.secretsmanager.secret.arn"`. Whether
   * they tripped the check depended on how the compiled binary's byte layout
   * split the printable-string chunks — so this fired on a local build while
   * CI, on the same lockfile, went green. A nondeterministic release blocker.
   */
  it('ignores OpenTelemetry semantic-convention constants', () => {
    const otel = [
      'NE$.AWS_SECRETSMANAGER_SECRET_ARN="aws.secretsmanager.secret.arn"',
      'NE$.GCP_CLIENT_SECRET="gcp.client.secret"',
      'NE$.DB_PASSWORD_FIELD="db.password.field"',
    ].join(',');
    expect(scan(artifact('otel.bin', otel)).code).toBe(0);
  });

  it('does not let a benign match hide a real one in the same chunk', () => {
    const chunk =
      'NE$.AWS_SECRETSMANAGER_SECRET_ARN="aws.secretsmanager.secret.arn",' +
      'DATABASE_PASSWORD="hunter2Correct-HorseBattery_Staple99"';
    const { code, out } = scan(artifact('mixed.bin', chunk));
    expect(code).toBe(1);
    expect(out).toContain('secret assignment');
  });

  it('reports the match, not the whole chunk', () => {
    // A 200KB chunk carrying one token must not be reported as "(200000 chars)".
    const chunk = `${'x'.repeat(200_000)} ghp_abcdefghijklmnopqrstuvwxyz0123456789`;
    const { out } = scan(artifact('big.bin', chunk));
    expect(out).toContain('(40 chars)');
    expect(out).not.toContain('200');
  });
});
