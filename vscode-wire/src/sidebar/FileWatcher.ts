import * as vscode from 'vscode';
import type { CollectionTreeProvider } from './CollectionTree.js';

/**
 * Watches for .wire.yaml file changes and refreshes the collection tree.
 * Handles changes from CLI, AI agents, and manual edits.
 */
export class FileWatcher implements vscode.Disposable {
  private watchers: vscode.FileSystemWatcher[] = [];
  private debounceTimer: NodeJS.Timeout | undefined;

  constructor(private treeProvider: CollectionTreeProvider) {
    // Watch for .wire.yaml file changes
    const wireYamlWatcher = vscode.workspace.createFileSystemWatcher('**/*.wire.yaml');
    wireYamlWatcher.onDidCreate(() => this.onFileChanged());
    wireYamlWatcher.onDidChange(() => this.onFileChanged());
    wireYamlWatcher.onDidDelete(() => this.onFileChanged());
    this.watchers.push(wireYamlWatcher);

    // Watch for env file changes
    const envWatcher = vscode.workspace.createFileSystemWatcher('**/.wire/envs/*.yaml');
    envWatcher.onDidCreate(() => this.onFileChanged());
    envWatcher.onDidChange(() => this.onFileChanged());
    envWatcher.onDidDelete(() => this.onFileChanged());
    this.watchers.push(envWatcher);

    // Watch for wire.yaml (collection config) changes
    const configWatcher = vscode.workspace.createFileSystemWatcher('**/.wire/wire.yaml');
    configWatcher.onDidChange(() => this.onFileChanged());
    this.watchers.push(configWatcher);
  }

  private onFileChanged() {
    // Debounce: batch rapid changes (e.g., from wire generate)
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
    }
    this.debounceTimer = setTimeout(() => {
      this.treeProvider.refresh();
    }, 300);
  }

  dispose() {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
    }
    for (const watcher of this.watchers) {
      watcher.dispose();
    }
  }
}
