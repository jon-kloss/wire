import * as vscode from 'vscode';
import { readdir, stat } from 'node:fs/promises';
import { join, basename } from 'node:path';
import { parseRequestFile, parseCollectionFile, ParseError } from '../core/yaml.js';

/** HTTP method → icon color mapping using Codicons */
const METHOD_ICONS: Record<string, vscode.ThemeIcon> = {
  GET: new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor('charts.green')),
  POST: new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor('charts.yellow')),
  PUT: new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor('charts.blue')),
  PATCH: new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor('charts.purple')),
  DELETE: new vscode.ThemeIcon('circle-filled', new vscode.ThemeColor('charts.red')),
};

export type TreeItemType = 'collection' | 'folder' | 'request' | 'template' | 'chain' | 'env';

export class CollectionTreeItem extends vscode.TreeItem {
  constructor(
    public readonly label: string,
    public readonly itemType: TreeItemType,
    public readonly collapsibleState: vscode.TreeItemCollapsibleState,
    public readonly filePath?: string,
    public readonly method?: string,
  ) {
    super(label, collapsibleState);
    this.contextValue = itemType;

    if (filePath) {
      this.resourceUri = vscode.Uri.file(filePath);
    }

    // Set icon based on type
    if (itemType === 'request' && method) {
      this.iconPath = METHOD_ICONS[method.toUpperCase()] ?? new vscode.ThemeIcon('circle-outline');
      this.description = method.toUpperCase();
    } else if (itemType === 'folder') {
      this.iconPath = new vscode.ThemeIcon('folder');
    } else if (itemType === 'collection') {
      this.iconPath = new vscode.ThemeIcon('database');
    } else if (itemType === 'template') {
      this.iconPath = new vscode.ThemeIcon('symbol-snippet');
      this.description = 'template';
    } else if (itemType === 'chain') {
      this.iconPath = new vscode.ThemeIcon('link');
      this.description = 'chain';
    } else if (itemType === 'env') {
      this.iconPath = new vscode.ThemeIcon('server-environment');
    }

    // Click to open request files
    if (itemType === 'request' || itemType === 'template' || itemType === 'chain') {
      this.command = {
        command: 'wire.openRequest',
        title: 'Open Request',
        arguments: [this],
      };
    }
  }
}

export class CollectionTreeProvider implements vscode.TreeDataProvider<CollectionTreeItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<CollectionTreeItem | undefined>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private wireDirectories: string[] = [];

  constructor() {
    this.discoverCollections();
  }

  refresh(): void {
    this.discoverCollections();
    this._onDidChangeTreeData.fire(undefined);
  }

  getTreeItem(element: CollectionTreeItem): vscode.TreeItem {
    return element;
  }

  async getChildren(element?: CollectionTreeItem): Promise<CollectionTreeItem[]> {
    if (!element) {
      // Root level: show discovered collections
      return this.getRootItems();
    }

    if (element.itemType === 'collection' && element.filePath) {
      return this.getCollectionChildren(element.filePath);
    }

    if (element.itemType === 'folder' && element.filePath) {
      return this.getFolderChildren(element.filePath);
    }

    return [];
  }

  /** Discover .wire/ directories in the workspace */
  private async discoverCollections() {
    this.wireDirectories = [];

    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (!workspaceFolders) return;

    for (const folder of workspaceFolders) {
      const wireYamlPattern = new vscode.RelativePattern(folder, '**/.wire/wire.yaml');
      const files = await vscode.workspace.findFiles(wireYamlPattern, '**/node_modules/**', 10);

      for (const file of files) {
        const wireDir = join(file.fsPath, '..');
        this.wireDirectories.push(wireDir);
      }
    }
  }

  private async getRootItems(): Promise<CollectionTreeItem[]> {
    if (this.wireDirectories.length === 0) {
      await this.discoverCollections();
    }

    const items: CollectionTreeItem[] = [];

    for (const wireDir of this.wireDirectories) {
      // Read wire.yaml for collection name
      let name = basename(join(wireDir, '..'));
      try {
        const config = await parseCollectionFile(join(wireDir, 'wire.yaml'));
        name = config.name;
      } catch {
        // Use directory name as fallback
      }

      items.push(
        new CollectionTreeItem(
          name,
          'collection',
          vscode.TreeItemCollapsibleState.Expanded,
          wireDir,
        )
      );
    }

    if (items.length === 0) {
      return [
        new CollectionTreeItem(
          'No .wire/ collections found',
          'folder',
          vscode.TreeItemCollapsibleState.None,
        ),
      ];
    }

    return items;
  }

  private async getCollectionChildren(wireDir: string): Promise<CollectionTreeItem[]> {
    const items: CollectionTreeItem[] = [];

    // Show: requests/, templates/, envs/ as top-level folders
    const requestsDir = join(wireDir, 'requests');
    const templatesDir = join(wireDir, 'templates');
    const envsDir = join(wireDir, 'envs');

    if (await dirExists(requestsDir)) {
      items.push(
        new CollectionTreeItem(
          'Requests',
          'folder',
          vscode.TreeItemCollapsibleState.Expanded,
          requestsDir,
        )
      );
    }

    if (await dirExists(templatesDir)) {
      items.push(
        new CollectionTreeItem(
          'Templates',
          'folder',
          vscode.TreeItemCollapsibleState.Collapsed,
          templatesDir,
        )
      );
    }

    if (await dirExists(envsDir)) {
      items.push(
        new CollectionTreeItem(
          'Environments',
          'folder',
          vscode.TreeItemCollapsibleState.Collapsed,
          envsDir,
        )
      );
    }

    return items;
  }

  private async getFolderChildren(dirPath: string): Promise<CollectionTreeItem[]> {
    const items: CollectionTreeItem[] = [];

    try {
      const entries = await readdir(dirPath, { withFileTypes: true });

      // Sort: directories first, then files
      const dirs = entries.filter((e) => e.isDirectory()).sort((a, b) => a.name.localeCompare(b.name));
      const files = entries.filter((e) => e.isFile()).sort((a, b) => a.name.localeCompare(b.name));

      for (const dir of dirs) {
        items.push(
          new CollectionTreeItem(
            dir.name,
            'folder',
            vscode.TreeItemCollapsibleState.Collapsed,
            join(dirPath, dir.name),
          )
        );
      }

      for (const file of files) {
        if (!file.name.endsWith('.wire.yaml') && !file.name.endsWith('.yaml')) continue;

        const filePath = join(dirPath, file.name);
        const item = await this.createFileItem(filePath);
        if (item) items.push(item);
      }
    } catch {
      // Directory not readable
    }

    return items;
  }

  private async createFileItem(filePath: string): Promise<CollectionTreeItem | null> {
    try {
      const request = await parseRequestFile(filePath);

      // Determine type: chain, template (in templates/), or request
      const isChain = request.chain.length > 0;
      const isTemplate = filePath.includes('/templates/');

      if (isChain) {
        return new CollectionTreeItem(
          request.name,
          'chain',
          vscode.TreeItemCollapsibleState.None,
          filePath,
        );
      }

      if (isTemplate) {
        return new CollectionTreeItem(
          request.name,
          'template',
          vscode.TreeItemCollapsibleState.None,
          filePath,
        );
      }

      // Regular request
      return new CollectionTreeItem(
        request.name,
        'request',
        vscode.TreeItemCollapsibleState.None,
        filePath,
        request.method,
      );
    } catch {
      return null;
    }
  }
}

async function dirExists(path: string): Promise<boolean> {
  try {
    const s = await stat(path);
    return s.isDirectory();
  } catch {
    return false;
  }
}
