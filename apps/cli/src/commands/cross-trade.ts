import { Command } from 'commander';
import chalk from 'chalk';

import type { CrossTradeExecution } from '@tealstreet/safe-cex/src/crossTrade/types';

import {
  buildSingleCopierConfig,
  getCrossTradeConfig,
  getCrossTradeFilePath,
  listCrossTradeConfigs,
  removeCrossTradeConfig,
  setCrossTradeConfigEnabled,
  upsertCrossTradeConfig,
  type StoredCrossTradeConfig,
} from '../cross-trade-store';
import { BRAND } from '../brand';

const SUPPORTED_EXECUTIONS: CrossTradeExecution[] = ['LimitSlippage', 'Market', 'Chase'];

function shortId(id: string): string {
  return id.slice(0, 8);
}

function summarizeRow(row: StoredCrossTradeConfig): string {
  const copierNames = row.copiers.filter((c) => c?.account).map((c) => c.account!);
  const copierExec = row.copiers[0]?.execution ?? 'LimitSlippage';
  const status = row.enabled ? chalk.green('enabled') : chalk.gray('disabled');
  const symbol = row.masterSymbol ? row.masterSymbol : chalk.gray('(any symbol)');
  return [
    `${chalk.bold(shortId(row.id))}  ${status}`,
    `  master:  ${row.masterAccount ?? chalk.red('(unset)')}  ${symbol}`,
    `  copiers: ${copierNames.length ? copierNames.join(', ') : chalk.red('(none)')}  mode: ${copierExec}`,
  ].join('\n');
}

export function registerCrossTradeCommand(program: Command): void {
  const crossTradeCmd = program
    .command('cross-trade')
    .alias('crosstrade')
    .description(`Manage and run the cross-trade engine (configs at ~/${BRAND.configDirName}/cross-trade.json)`);

  crossTradeCmd
    .command('list')
    .alias('ls')
    .description('List all cross-trade configs')
    .action(() => {
      const rows = listCrossTradeConfigs();
      if (rows.length === 0) {
        console.log(chalk.yellow('No cross-trade configs.'));
        console.log(chalk.gray(`Run: ${BRAND.binName} cross-trade add --master <account> --copier <account>`));
        return;
      }
      console.log(chalk.cyan('\nCross-trade configs:\n'));
      rows.forEach((row) => {
        console.log(summarizeRow(row));
        console.log('');
      });
      console.log(chalk.gray(`File: ${getCrossTradeFilePath()}`));
    });

  crossTradeCmd
    .command('add')
    .description('Add a single-master / single-copier config')
    .requiredOption('--master <account>', 'Master account name')
    .requiredOption('--copier <account>', 'Copier account name')
    .option('--symbol <symbol>', 'Master symbol (omit to copy all symbols)')
    .option('--mode <mode>', `Execution mode (${SUPPORTED_EXECUTIONS.join(' | ')})`, 'LimitSlippage')
    .option('--weight <number>', 'Copier weight', '1')
    .option('--enabled', 'Enable the config immediately (default off)')
    .action((options) => {
      const mode = options.mode as CrossTradeExecution;
      if (!SUPPORTED_EXECUTIONS.includes(mode)) {
        console.log(chalk.red(`Unsupported mode: ${options.mode}`));
        console.log(chalk.gray(`Supported: ${SUPPORTED_EXECUTIONS.join(', ')}`));
        process.exitCode = 1;
        return;
      }
      const weight = Number.parseFloat(options.weight);
      if (!Number.isFinite(weight)) {
        console.log(chalk.red(`Invalid --weight: ${options.weight}`));
        process.exitCode = 1;
        return;
      }
      const row = buildSingleCopierConfig({
        masterAccount: options.master,
        copierAccount: options.copier,
        masterSymbol: options.symbol,
        copierExecution: mode,
        copierWeight: weight,
        enabled: !!options.enabled,
      });
      upsertCrossTradeConfig(row);
      console.log(chalk.green(`Added cross-trade config ${shortId(row.id)} (${row.id})`));
      if (mode === 'Chase') {
        console.log(
          chalk.yellow(
            '\nNote: Chase execution mode is NOT supported by the CLI cross-trade engine in v1.\n' +
              `The config will be saved but skipped at \`${BRAND.binName} cross-trade run\`. Use Market\n` +
              'or LimitSlippage if you want to run this on the CLI.',
          ),
        );
      }
      if (!row.enabled) {
        console.log(chalk.gray(`Pass --enabled, or run: ${BRAND.binName} cross-trade enable ${shortId(row.id)}`));
      }
    });

  crossTradeCmd
    .command('enable <id>')
    .description('Enable a cross-trade config (matches by id prefix)')
    .action((id: string) => {
      const target = resolveByPrefix(id);
      if (!target) return;
      if (setCrossTradeConfigEnabled(target.id, true)) {
        console.log(chalk.green(`Enabled cross-trade config ${shortId(target.id)}`));
      }
    });

  crossTradeCmd
    .command('disable <id>')
    .description('Disable a cross-trade config (matches by id prefix)')
    .action((id: string) => {
      const target = resolveByPrefix(id);
      if (!target) return;
      if (setCrossTradeConfigEnabled(target.id, false)) {
        console.log(chalk.yellow(`Disabled cross-trade config ${shortId(target.id)}`));
      }
    });

  crossTradeCmd
    .command('remove <id>')
    .alias('rm')
    .description('Soft-delete a cross-trade config (tombstoned for sync)')
    .action((id: string) => {
      const target = resolveByPrefix(id);
      if (!target) return;
      if (removeCrossTradeConfig(target.id)) {
        console.log(chalk.green(`Removed cross-trade config ${shortId(target.id)}`));
      }
    });

  crossTradeCmd
    .command('status')
    .description('Show whether the cross-trade engine is running')
    .action(async () => {
      const { readFreshHeartbeat } = await import('../cross-trade-heartbeat');
      const hb = readFreshHeartbeat();
      const configs = listCrossTradeConfigs();
      const runnableCount = configs.filter(
        (c) => c.enabled && c.masterAccount && c.copiers.some((cp) => cp?.enabled && cp.account),
      ).length;

      if (!hb) {
        console.log(chalk.gray('Cross-trade engine: not running.'));
        console.log(chalk.gray(`Configs: ${configs.length} total, ${runnableCount} runnable.`));
        if (runnableCount > 0) {
          console.log(chalk.gray(`Start with: ${BRAND.binName} cross-trade run`));
        }
        return;
      }

      const uptimeSec = Math.floor((Date.now() - hb.startedAt) / 1000);
      const heartbeatAgeMs = Date.now() - hb.lastHeartbeatAt;
      console.log(chalk.green(`Cross-trade engine: running (pid ${hb.pid}).`));
      console.log(chalk.gray(`  Started: ${new Date(hb.startedAt).toISOString()} (${uptimeSec}s ago)`));
      console.log(chalk.gray(`  Active configs: ${hb.configCount}`));
      console.log(chalk.gray(`  Last heartbeat: ${heartbeatAgeMs}ms ago`));
      console.log(chalk.gray(`  Configs (total / runnable): ${configs.length} / ${runnableCount}`));
    });

  crossTradeCmd
    .command('run')
    .description('Run the cross-trade engine (foreground daemon)')
    .action(async () => {
      const { runCrossTradeDaemon } = await import('../cross-trade-daemon');
      try {
        await runCrossTradeDaemon();
      } catch (err) {
        console.log(chalk.red(`cross-trade engine error: ${(err as Error).message}`));
        process.exitCode = 1;
      }
    });
}

function resolveByPrefix(prefix: string): StoredCrossTradeConfig | undefined {
  const exact = getCrossTradeConfig(prefix);
  if (exact) return exact;
  const matches = listCrossTradeConfigs().filter((c) => c.id.startsWith(prefix));
  if (matches.length === 0) {
    console.log(chalk.red(`No cross-trade config matches "${prefix}"`));
    process.exitCode = 1;
    return undefined;
  }
  if (matches.length > 1) {
    console.log(chalk.red(`Ambiguous prefix "${prefix}" — matches:`));
    matches.forEach((m) => console.log(`  ${shortId(m.id)}  master=${m.masterAccount}`));
    process.exitCode = 1;
    return undefined;
  }
  return matches[0];
}
