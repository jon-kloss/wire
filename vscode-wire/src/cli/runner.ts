import { spawn } from 'node:child_process';
import type {
  WireResponse,
  TestRunSummary,
  ChainResult,
  DriftReport,
  BreakingReport,
  HistoryEntry,
  ScanResult,
} from '../core/types.js';

export interface CliResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

/**
 * Typed wrapper around the wire CLI binary.
 * Each method maps to a specific CLI command and returns typed JSON output.
 */
export class WireCliRunner {
  constructor(private binaryPath: string) {}

  // ─── Typed command methods ───────────────────────────────────────────

  /** Execute `wire send <file> -o json` and return the response */
  async send(filePath: string, wireDir?: string, env?: string): Promise<WireResponse> {
    const args = ['send', filePath];
    if (wireDir) args.push('-d', wireDir);
    if (env) args.push('-e', env);
    return this.runJson<WireResponse>(args);
  }

  /** Execute `wire test <path> -o json` and return test results */
  async test(path: string, wireDir?: string, env?: string, snapshot?: boolean): Promise<TestRunSummary> {
    const args = ['test', path];
    if (wireDir) args.push('-d', wireDir);
    if (env) args.push('-e', env);
    if (snapshot) args.push('--snapshot');
    return this.runJson<TestRunSummary>(args);
  }

  /** Execute `wire chain run <file> -o json` and return chain results */
  async chainRun(filePath: string, wireDir?: string, env?: string): Promise<ChainResult> {
    const args = ['chain', 'run', filePath];
    if (wireDir) args.push('-d', wireDir);
    if (env) args.push('-e', env);
    return this.runJson<ChainResult>(args);
  }

  /** Execute `wire drift <projectDir> -o json` and return drift report */
  async drift(projectDir: string, wireDir?: string): Promise<DriftReport> {
    const args = ['drift', projectDir];
    if (wireDir) args.push(wireDir);
    return this.runJson<DriftReport>(args);
  }

  /** Execute `wire drift <projectDir> --fix` to auto-fix drift */
  async driftFix(projectDir: string, wireDir?: string): Promise<CliResult> {
    const args = ['drift', projectDir];
    if (wireDir) args.push(wireDir);
    args.push('--fix');
    return this.run(args);
  }

  /** Execute `wire breaking -o json` to check for breaking changes */
  async breaking(wireDir?: string): Promise<BreakingReport> {
    const args = ['breaking'];
    if (wireDir) args.push('-d', wireDir);
    return this.runJson<BreakingReport>(args);
  }

  /** Execute `wire breaking --save` to save a baseline */
  async breakingSave(wireDir?: string): Promise<CliResult> {
    const args = ['breaking', '--save'];
    if (wireDir) args.push('-d', wireDir);
    return this.run(args);
  }

  /** Execute `wire generate <projectDir>` to scan and generate collection */
  async generate(projectDir: string): Promise<ScanResult> {
    return this.runJson<ScanResult>(['generate', projectDir]);
  }

  /** Execute `wire history -o json` to get request history */
  async history(wireDir?: string): Promise<HistoryEntry[]> {
    const args = ['history'];
    if (wireDir) args.push('-d', wireDir);
    return this.runJson<HistoryEntry[]>(args);
  }

  /** Execute `wire history clear` to clear history */
  async historyClear(wireDir?: string): Promise<CliResult> {
    const args = ['history', 'clear'];
    if (wireDir) args.push('-d', wireDir);
    return this.run(args);
  }

  /** Execute `wire env check -d <wireDir>` to validate secret references */
  async envCheck(wireDir: string): Promise<CliResult> {
    return this.run(['env', 'check', '-d', wireDir]);
  }

  /** Execute `wire send <file> --snapshot` to save a snapshot */
  async sendSnapshot(filePath: string, wireDir?: string, env?: string): Promise<WireResponse> {
    const args = ['send', filePath, '--snapshot'];
    if (wireDir) args.push('-d', wireDir);
    if (env) args.push('-e', env);
    return this.runJson<WireResponse>(args);
  }

  /** Execute `wire snapshot update <file>` to overwrite golden file */
  async snapshotUpdate(filePath: string, wireDir?: string): Promise<CliResult> {
    const args = ['snapshot', 'update', filePath];
    if (wireDir) args.push('-d', wireDir);
    return this.run(args);
  }

  /** Get the wire CLI version */
  async version(): Promise<string> {
    const result = await this.run(['--version']);
    return result.stdout.trim();
  }

  // ─── Generic execution ───────────────────────────────────────────────

  /** Run a wire command and return parsed JSON output */
  async runJson<T>(args: string[], cwd?: string): Promise<T> {
    const result = await this.run([...args, '-o', 'json'], cwd);
    if (result.exitCode !== 0) {
      throw new WireCliError(args, result);
    }
    try {
      return JSON.parse(result.stdout) as T;
    } catch {
      throw new WireCliError(
        args,
        { ...result, stderr: `Failed to parse JSON output: ${result.stdout.slice(0, 200)}` }
      );
    }
  }

  /** Run a wire command and return raw result */
  async run(args: string[], cwd?: string): Promise<CliResult> {
    return new Promise((resolve, reject) => {
      const child = spawn(this.binaryPath, args, {
        cwd,
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: 60_000,
      });

      const stdout: Buffer[] = [];
      const stderr: Buffer[] = [];

      child.stdout.on('data', (chunk: Buffer) => stdout.push(chunk));
      child.stderr.on('data', (chunk: Buffer) => stderr.push(chunk));

      child.on('error', (err) => reject(err));
      child.on('close', (code) => {
        resolve({
          stdout: Buffer.concat(stdout).toString('utf-8'),
          stderr: Buffer.concat(stderr).toString('utf-8'),
          exitCode: code ?? 1,
        });
      });
    });
  }

  /** Update the binary path (e.g., after auto-update) */
  setBinaryPath(path: string) {
    this.binaryPath = path;
  }
}

export class WireCliError extends Error {
  constructor(
    public readonly args: string[],
    public readonly result: CliResult
  ) {
    super(
      `wire ${args.join(' ')} exited with code ${result.exitCode}: ${result.stderr}`
    );
    this.name = 'WireCliError';
  }
}
