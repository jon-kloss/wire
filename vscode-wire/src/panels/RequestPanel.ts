import * as vscode from 'vscode';
import { readFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { WireCliRunner } from '../cli/runner.js';
import { parseRequestFile } from '../core/yaml.js';
import type { WireRequest, WireResponse, TestResult } from '../core/types.js';

/**
 * Webview panel for the request builder + response viewer.
 * Opens when a user clicks a request in the collection tree or runs Wire: Send Request.
 */
export class RequestPanel {
  public static readonly viewType = 'wire.requestPanel';
  private static panels = new Map<string, RequestPanel>();

  private panel: vscode.WebviewPanel;
  private disposables: vscode.Disposable[] = [];

  private constructor(
    panel: vscode.WebviewPanel,
    private extensionUri: vscode.Uri,
    private cliRunner: WireCliRunner,
    private filePath: string,
  ) {
    this.panel = panel;
    this.panel.onDidDispose(() => this.dispose(), null, this.disposables);
    this.panel.webview.onDidReceiveMessage(
      (msg) => this.handleMessage(msg),
      null,
      this.disposables,
    );
    this.panel.webview.html = this.getHtml();
    this.loadRequest();
  }

  /** Open or focus a request panel for the given file. */
  static open(
    extensionUri: vscode.Uri,
    cliRunner: WireCliRunner,
    filePath: string,
  ) {
    const existing = RequestPanel.panels.get(filePath);
    if (existing) {
      existing.panel.reveal();
      return existing;
    }

    const panel = vscode.window.createWebviewPanel(
      RequestPanel.viewType,
      'Wire Request',
      vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'dist')],
      },
    );

    const instance = new RequestPanel(panel, extensionUri, cliRunner, filePath);
    RequestPanel.panels.set(filePath, instance);
    return instance;
  }

  /** Load the request file and send its data to the webview. */
  private async loadRequest() {
    try {
      const request = await parseRequestFile(this.filePath);
      this.panel.title = `${request.method} ${request.name}`;
      this.panel.webview.postMessage({
        type: 'loadRequest',
        request,
        filePath: this.filePath,
      });
    } catch (err) {
      this.panel.webview.postMessage({
        type: 'error',
        message: `Failed to load request: ${err}`,
      });
    }
  }

  /** Handle messages from the webview. */
  private async handleMessage(msg: { type: string; [key: string]: unknown }) {
    switch (msg.type) {
      case 'send': {
        await this.sendRequest();
        break;
      }
      case 'test': {
        await this.runTests();
        break;
      }
      case 'saveRequest': {
        await this.saveRequest(msg.request as WireRequest);
        break;
      }
    }
  }

  /** Execute the request via wire CLI and send the response to the webview. */
  private async sendRequest() {
    this.panel.webview.postMessage({ type: 'sending' });

    try {
      const wireDir = this.findWireDir();
      const response = await this.cliRunner.send(this.filePath, wireDir);
      this.panel.webview.postMessage({ type: 'response', response });
    } catch (err) {
      this.panel.webview.postMessage({
        type: 'error',
        message: `Request failed: ${err}`,
      });
    }
  }

  /** Run tests via wire CLI and send results to the webview. */
  private async runTests() {
    this.panel.webview.postMessage({ type: 'testing' });

    try {
      const wireDir = this.findWireDir();
      const results = await this.cliRunner.test(this.filePath, wireDir);
      this.panel.webview.postMessage({ type: 'testResults', results });
    } catch (err) {
      this.panel.webview.postMessage({
        type: 'error',
        message: `Tests failed: ${err}`,
      });
    }
  }

  /** Save modified request back to the .wire.yaml file. */
  private async saveRequest(request: WireRequest) {
    try {
      const { serializeRequest } = await import('../core/yaml.js');
      const { writeFile } = await import('node:fs/promises');
      await writeFile(this.filePath, serializeRequest(request));
      vscode.window.showInformationMessage(`Saved: ${request.name}`);
    } catch (err) {
      vscode.window.showErrorMessage(`Failed to save: ${err}`);
    }
  }

  /** Find the .wire/ directory for this request file. */
  private findWireDir(): string | undefined {
    let dir = dirname(this.filePath);
    for (let i = 0; i < 5; i++) {
      if (dir.endsWith('.wire') || dir.endsWith('.wire/')) {
        return dir;
      }
      const parent = dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
    return undefined;
  }

  /** Generate the webview HTML with the bundled React app. */
  private getHtml(): string {
    const webview = this.panel.webview;
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.extensionUri, 'dist', 'webview.js')
    );
    const nonce = getNonce();

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-${nonce}'; style-src 'unsafe-inline' ${webview.cspSource};">
  <title>Wire Request</title>
  <style>
    :root {
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      background-color: var(--vscode-editor-background);
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { padding: 0; }
  </style>
</head>
<body>
  <div id="root"></div>
  <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
  }

  private dispose() {
    RequestPanel.panels.delete(this.filePath);
    this.panel.dispose();
    for (const d of this.disposables) d.dispose();
  }
}

function getNonce(): string {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  let result = '';
  for (let i = 0; i < 32; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}
