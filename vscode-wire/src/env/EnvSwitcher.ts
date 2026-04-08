import * as vscode from 'vscode';
import { readdir, writeFile } from 'node:fs/promises';
import { join, basename } from 'node:path';
import {
  parseCollectionFile,
  parseEnvironmentFile,
  serializeCollection,
} from '../core/yaml.js';

/**
 * Status bar item for quick environment switching.
 * Shows the active environment and allows switching via quick pick.
 */
export class EnvSwitcher implements vscode.Disposable {
  private statusBarItem: vscode.StatusBarItem;
  private activeEnv: string = 'dev';
  private activeWireDir: string | null = null;

  constructor() {
    this.statusBarItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Right,
      100
    );
    this.statusBarItem.command = 'wire.switchEnvironment';
    this.statusBarItem.tooltip = 'Wire: Switch Environment';
    this.updateStatusBar();
    this.statusBarItem.show();

    // Load initial state from first discovered collection
    this.loadActiveEnv();
  }

  async showQuickPick() {
    const envs = await this.getAvailableEnvironments();
    if (envs.length === 0) {
      vscode.window.showInformationMessage('No Wire environments found.');
      return;
    }

    const items: vscode.QuickPickItem[] = envs.map((env) => ({
      label: env.name,
      description: env.file,
      picked: env.name === this.activeEnv,
    }));

    const selected = await vscode.window.showQuickPick(items, {
      placeHolder: 'Select Wire environment',
    });

    if (selected) {
      await this.setActiveEnv(selected.label);
    }
  }

  private async loadActiveEnv() {
    const wireDir = await this.findFirstWireDir();
    if (!wireDir) return;

    this.activeWireDir = wireDir;

    try {
      const config = await parseCollectionFile(join(wireDir, 'wire.yaml'));
      if (config.active_env) {
        this.activeEnv = config.active_env;
        this.updateStatusBar();
      }
    } catch {
      // Use default
    }
  }

  private async setActiveEnv(envName: string) {
    this.activeEnv = envName;
    this.updateStatusBar();

    // Update wire.yaml
    if (this.activeWireDir) {
      try {
        const configPath = join(this.activeWireDir, 'wire.yaml');
        const config = await parseCollectionFile(configPath);
        config.active_env = envName;
        await writeFile(configPath, serializeCollection(config));
      } catch {
        vscode.window.showErrorMessage(`Failed to update active environment to '${envName}'`);
      }
    }
  }

  private updateStatusBar() {
    this.statusBarItem.text = `$(server-environment) ${this.activeEnv}`;
  }

  private async getAvailableEnvironments(): Promise<Array<{ name: string; file: string }>> {
    const wireDir = this.activeWireDir ?? await this.findFirstWireDir();
    if (!wireDir) return [];

    const envsDir = join(wireDir, 'envs');
    try {
      const files = await readdir(envsDir);
      const envs: Array<{ name: string; file: string }> = [];

      for (const file of files) {
        if (!file.endsWith('.yaml')) continue;
        try {
          const env = await parseEnvironmentFile(join(envsDir, file));
          envs.push({ name: env.name, file });
        } catch {
          envs.push({ name: basename(file, '.yaml'), file });
        }
      }

      return envs;
    } catch {
      return [];
    }
  }

  private async findFirstWireDir(): Promise<string | null> {
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (!workspaceFolders) return null;

    for (const folder of workspaceFolders) {
      const pattern = new vscode.RelativePattern(folder, '**/.wire/wire.yaml');
      const files = await vscode.workspace.findFiles(pattern, '**/node_modules/**', 1);
      if (files.length > 0) {
        return join(files[0].fsPath, '..');
      }
    }

    return null;
  }

  dispose() {
    this.statusBarItem.dispose();
  }
}
