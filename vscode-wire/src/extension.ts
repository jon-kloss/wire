import * as vscode from 'vscode';
import { WireBinary } from './cli/binary.js';
import { WireCliRunner } from './cli/runner.js';
import { CollectionTreeProvider, CollectionTreeItem } from './sidebar/CollectionTree.js';
import { FileWatcher } from './sidebar/FileWatcher.js';
import { EnvSwitcher } from './env/EnvSwitcher.js';
import { RequestPanel } from './panels/RequestPanel.js';
import { serializeRequest, serializeEnvironment } from './core/yaml.js';
import { writeFile, mkdir, rm, rename } from 'node:fs/promises';
import { join, dirname, basename, extname } from 'node:path';

let cliRunner: WireCliRunner;

export async function activate(context: vscode.ExtensionContext) {
  // 1. Ensure wire CLI binary is available
  const wireBinary = new WireBinary(context);
  const binaryPath = await wireBinary.ensureAvailable();

  if (!binaryPath) {
    vscode.window.showErrorMessage(
      'Wire CLI not found. Install it with `cargo install --path crates/wire-cli` or allow the extension to download it.'
    );
    return;
  }

  // 2. Initialize CLI runner
  cliRunner = new WireCliRunner(binaryPath);

  // 3. Set up collection tree sidebar
  const collectionTree = new CollectionTreeProvider();
  const treeView = vscode.window.createTreeView('wire.collections', {
    treeDataProvider: collectionTree,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  // 4. Set up file watcher
  const fileWatcher = new FileWatcher(collectionTree);
  context.subscriptions.push(fileWatcher);

  // 5. Set up environment switcher
  const envSwitcher = new EnvSwitcher();
  context.subscriptions.push(envSwitcher);

  // 6. Register commands
  context.subscriptions.push(
    // ─── Request execution ────────────────────────────────────────────
    vscode.commands.registerCommand('wire.sendRequest', (item?: CollectionTreeItem) => {
      const filePath = item?.filePath ?? getActiveWireFile();
      if (filePath) {
        const panel = RequestPanel.open(context.extensionUri, cliRunner, filePath);
      } else {
        vscode.window.showWarningMessage('No .wire.yaml file selected');
      }
    }),

    vscode.commands.registerCommand('wire.runTests', (item?: CollectionTreeItem) => {
      const filePath = item?.filePath ?? getActiveWireFile();
      if (filePath) {
        const panel = RequestPanel.open(context.extensionUri, cliRunner, filePath);
        // Panel will receive test command after loading
      }
    }),

    vscode.commands.registerCommand('wire.runChain', (item?: CollectionTreeItem) => {
      const filePath = item?.filePath;
      if (filePath) {
        vscode.window.showInformationMessage('Wire: Run Chain — coming soon');
      }
    }),

    // ─── Open request in panel ────────────────────────────────────────
    vscode.commands.registerCommand('wire.openRequest', (item: CollectionTreeItem) => {
      if (item?.filePath) {
        RequestPanel.open(context.extensionUri, cliRunner, item.filePath);
      }
    }),

    // ─── Environment ──────────────────────────────────────────────────
    vscode.commands.registerCommand('wire.switchEnvironment', () => {
      envSwitcher.showQuickPick();
    }),

    // ─── Collection tree CRUD ─────────────────────────────────────────
    vscode.commands.registerCommand('wire.newRequest', async (item?: CollectionTreeItem) => {
      const targetDir = item?.filePath ?? await findRequestsDir();
      if (!targetDir) {
        vscode.window.showWarningMessage('No .wire/ collection found');
        return;
      }

      const name = await vscode.window.showInputBox({
        prompt: 'Request name',
        placeHolder: 'e.g. get-users',
      });
      if (!name) return;

      const method = await vscode.window.showQuickPick(
        ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'],
        { placeHolder: 'HTTP method' },
      );
      if (!method) return;

      const url = await vscode.window.showInputBox({
        prompt: 'URL',
        placeHolder: '{{base_url}}/api/endpoint',
        value: '{{base_url}}/',
      });
      if (!url) return;

      const fileName = `${name}.wire.yaml`;
      const filePath = join(targetDir, fileName);

      const request = {
        name,
        method,
        url,
        headers: {},
        params: {},
        tests: [],
        response_schema: [],
        chain: [],
      };

      await writeFile(filePath, serializeRequest(request));
      collectionTree.refresh();
      RequestPanel.open(context.extensionUri, cliRunner, filePath);
    }),

    vscode.commands.registerCommand('wire.newFolder', async (item?: CollectionTreeItem) => {
      const parentDir = item?.filePath ?? await findRequestsDir();
      if (!parentDir) {
        vscode.window.showWarningMessage('No .wire/ collection found');
        return;
      }

      const name = await vscode.window.showInputBox({
        prompt: 'Folder name',
        placeHolder: 'e.g. users',
      });
      if (!name) return;

      await mkdir(join(parentDir, name), { recursive: true });
      collectionTree.refresh();
    }),

    vscode.commands.registerCommand('wire.deleteItem', async (item?: CollectionTreeItem) => {
      if (!item?.filePath) return;

      const label = typeof item.label === 'string' ? item.label : 'this item';
      const confirm = await vscode.window.showWarningMessage(
        `Delete "${label}"?`,
        { modal: true },
        'Delete',
      );
      if (confirm !== 'Delete') return;

      try {
        await rm(item.filePath, { recursive: true });
        collectionTree.refresh();
      } catch (err) {
        vscode.window.showErrorMessage(`Failed to delete: ${err}`);
      }
    }),

    vscode.commands.registerCommand('wire.renameItem', async (item?: CollectionTreeItem) => {
      if (!item?.filePath) return;

      const currentName = basename(item.filePath, '.wire.yaml');
      const newName = await vscode.window.showInputBox({
        prompt: 'New name',
        value: currentName,
      });
      if (!newName || newName === currentName) return;

      const dir = dirname(item.filePath);
      const ext = item.filePath.endsWith('.wire.yaml') ? '.wire.yaml' : extname(item.filePath);
      const newPath = join(dir, `${newName}${ext}`);

      try {
        await rename(item.filePath, newPath);
        collectionTree.refresh();
      } catch (err) {
        vscode.window.showErrorMessage(`Failed to rename: ${err}`);
      }
    }),

    vscode.commands.registerCommand('wire.refreshCollections', () => {
      collectionTree.refresh();
    }),

    // ─── Stub commands (panels coming later) ──────────────────────────
    vscode.commands.registerCommand('wire.scanDrift', () => {
      vscode.window.showInformationMessage('Wire: Scan Drift — coming soon');
    }),
    vscode.commands.registerCommand('wire.checkBreaking', () => {
      vscode.window.showInformationMessage('Wire: Check Breaking — coming soon');
    }),
    vscode.commands.registerCommand('wire.generateCollection', () => {
      vscode.window.showInformationMessage('Wire: Generate Collection — coming soon');
    }),
    vscode.commands.registerCommand('wire.saveSnapshot', () => {
      vscode.window.showInformationMessage('Wire: Save Snapshot — coming soon');
    }),
    vscode.commands.registerCommand('wire.compareSnapshot', () => {
      vscode.window.showInformationMessage('Wire: Compare Snapshot — coming soon');
    }),
    vscode.commands.registerCommand('wire.showHistory', () => {
      vscode.window.showInformationMessage('Wire: Show History — coming soon');
    }),
    vscode.commands.registerCommand('wire.validateSecrets', () => {
      vscode.window.showInformationMessage('Wire: Validate Secrets — coming soon');
    }),
  );

  vscode.window.showInformationMessage(`Wire extension activated (CLI: ${binaryPath})`);
}

export function deactivate() {
  // Cleanup handled by disposables
}

/** Get the active editor's file if it's a .wire.yaml file. */
function getActiveWireFile(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  if (editor?.document.fileName.endsWith('.wire.yaml')) {
    return editor.document.fileName;
  }
  return undefined;
}

/** Find the first .wire/requests/ directory in the workspace. */
async function findRequestsDir(): Promise<string | null> {
  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (!workspaceFolders) return null;

  for (const folder of workspaceFolders) {
    const pattern = new vscode.RelativePattern(folder, '**/.wire/wire.yaml');
    const files = await vscode.workspace.findFiles(pattern, '**/node_modules/**', 1);
    if (files.length > 0) {
      const wireDir = dirname(files[0].fsPath);
      return join(wireDir, 'requests');
    }
  }
  return null;
}
