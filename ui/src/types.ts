/** HTTP response from the Rust backend */
export interface IpcResponse {
  status: number;
  status_text: string;
  headers: Record<string, string>;
  body: string;
  elapsed_ms: number;
  size_bytes: number;
}

/** A single request entry in a collection */
export interface IpcRequestEntry {
  path: string;
  name: string;
  method: string;
}

/** Collection info returned after opening a .wire/ directory */
export interface IpcCollectionInfo {
  name: string;
  version: number;
  active_env: string | null;
  requests: IpcRequestEntry[];
  environments: string[];
}
