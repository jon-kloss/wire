import * as vscode from 'vscode';
import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, chmod, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { homedir, platform, arch } from 'node:os';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

/** Known binary locations to check, in priority order */
const SEARCH_PATHS = [
  // User's PATH (resolved via `which`)
  null,
  // Wire-specific install location
  () => join(homedir(), '.wire', 'bin', 'wire'),
  // Cargo install location
  () => join(homedir(), '.cargo', 'bin', 'wire'),
];

/** Map Node.js os values to GitHub release target triples */
function getReleaseTarget(): string | null {
  const p = platform();
  const a = arch();

  const targets: Record<string, Record<string, string>> = {
    darwin: {
      arm64: 'aarch64-apple-darwin',
      x64: 'x86_64-apple-darwin',
    },
    linux: {
      arm64: 'aarch64-unknown-linux-gnu',
      x64: 'x86_64-unknown-linux-gnu',
    },
    win32: {
      x64: 'x86_64-pc-windows-msvc',
    },
  };

  return targets[p]?.[a] ?? null;
}

export class WireBinary {
  private context: vscode.ExtensionContext;
  private resolvedPath: string | null = null;

  constructor(context: vscode.ExtensionContext) {
    this.context = context;
  }

  /** Find or download the wire binary. Returns the path or null on failure. */
  async ensureAvailable(): Promise<string | null> {
    // 1. Check cached path from last session
    const cached = this.context.globalState.get<string>('wire.binaryPath');
    if (cached && existsSync(cached)) {
      const valid = await this.validateBinary(cached);
      if (valid) {
        this.resolvedPath = cached;
        return cached;
      }
    }

    // 2. Search known locations
    const found = await this.searchForBinary();
    if (found) {
      this.resolvedPath = found;
      await this.context.globalState.update('wire.binaryPath', found);
      return found;
    }

    // 3. Offer to download
    const choice = await vscode.window.showInformationMessage(
      'Wire CLI not found. Download it automatically?',
      'Download',
      'Cancel'
    );

    if (choice === 'Download') {
      const downloaded = await this.downloadBinary();
      if (downloaded) {
        this.resolvedPath = downloaded;
        await this.context.globalState.update('wire.binaryPath', downloaded);
        return downloaded;
      }
    }

    return null;
  }

  /** Get the resolved binary path (after ensureAvailable) */
  getPath(): string | null {
    return this.resolvedPath;
  }

  /** Get the version of the resolved binary */
  async getVersion(): Promise<string | null> {
    if (!this.resolvedPath) return null;
    try {
      const { stdout } = await execFileAsync(this.resolvedPath, ['--version']);
      return stdout.trim();
    } catch {
      return null;
    }
  }

  /** Search known locations for the wire binary */
  private async searchForBinary(): Promise<string | null> {
    // Check PATH first via `which` (unix) or `where` (windows)
    try {
      const cmd = platform() === 'win32' ? 'where' : 'which';
      const { stdout } = await execFileAsync(cmd, ['wire']);
      const path = stdout.trim().split('\n')[0];
      if (path && await this.validateBinary(path)) {
        return path;
      }
    } catch {
      // Not in PATH
    }

    // Check known fixed locations
    for (const pathFn of SEARCH_PATHS) {
      if (!pathFn) continue;
      const path = pathFn();
      if (existsSync(path) && await this.validateBinary(path)) {
        return path;
      }
    }

    return null;
  }

  /** Validate that a binary at the given path is actually wire */
  private async validateBinary(path: string): Promise<boolean> {
    try {
      const { stdout } = await execFileAsync(path, ['--version'], {
        timeout: 5000,
      });
      return stdout.includes('wire');
    } catch {
      return false;
    }
  }

  /** Download the wire binary from GitHub releases */
  private async downloadBinary(): Promise<string | null> {
    const target = getReleaseTarget();
    if (!target) {
      vscode.window.showErrorMessage(
        `Unsupported platform: ${platform()} ${arch()}`
      );
      return null;
    }

    return await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: 'Downloading Wire CLI...',
        cancellable: true,
      },
      async (progress, token) => {
        try {
          // Determine install directory
          const installDir = join(homedir(), '.wire', 'bin');
          await mkdir(installDir, { recursive: true });

          const binaryName = platform() === 'win32' ? 'wire.exe' : 'wire';
          const installPath = join(installDir, binaryName);

          progress.report({ message: 'Fetching latest release info...' });

          // Fetch latest release from GitHub API
          const { request } = await import('undici');

          const releaseResponse = await request(
            'https://api.github.com/repos/wire-cli/wire/releases/latest',
            {
              headers: {
                'User-Agent': 'wire-vscode-extension',
                Accept: 'application/vnd.github.v3+json',
              },
            }
          );

          if (releaseResponse.statusCode !== 200) {
            throw new Error(`GitHub API returned ${releaseResponse.statusCode}`);
          }

          const release = await releaseResponse.body.json() as {
            assets: Array<{ name: string; browser_download_url: string }>;
          };

          // Find matching asset
          const asset = release.assets.find(
            (a: { name: string }) => a.name.includes(target)
          );

          if (!asset) {
            throw new Error(
              `No release asset found for ${target}. Available: ${release.assets.map((a: { name: string }) => a.name).join(', ')}`
            );
          }

          if (token.isCancellationRequested) return null;

          progress.report({ message: `Downloading ${asset.name}...` });

          // Download the binary
          const downloadResponse = await request(asset.browser_download_url, {
            headers: { 'User-Agent': 'wire-vscode-extension' },
            maxRedirections: 5,
          });

          if (downloadResponse.statusCode !== 200) {
            throw new Error(`Download failed with ${downloadResponse.statusCode}`);
          }

          const buffer = Buffer.from(await downloadResponse.body.arrayBuffer());

          if (token.isCancellationRequested) return null;

          progress.report({ message: 'Installing...' });

          // Write binary and make executable
          await writeFile(installPath, buffer);
          if (platform() !== 'win32') {
            await chmod(installPath, 0o755);
          }

          // Validate the downloaded binary
          if (await this.validateBinary(installPath)) {
            const version = await this.getVersionFromPath(installPath);
            vscode.window.showInformationMessage(
              `Wire CLI ${version ?? ''} installed to ${installPath}`
            );
            return installPath;
          }

          throw new Error('Downloaded binary failed validation');
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          vscode.window.showErrorMessage(`Failed to download Wire CLI: ${message}`);
          return null;
        }
      }
    );
  }

  private async getVersionFromPath(path: string): Promise<string | null> {
    try {
      const { stdout } = await execFileAsync(path, ['--version']);
      return stdout.trim();
    } catch {
      return null;
    }
  }
}
