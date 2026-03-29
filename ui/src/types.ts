/** HTTP response from the Rust backend */
export interface IpcResponse {
  status: number;
  status_text: string;
  headers: Record<string, string>;
  body: string;
  elapsed_ms: number;
  size_bytes: number;
}

/** Request body */
export interface WireBody {
  type: "json" | "text" | "formdata";
  content: unknown;
}

/** A test assertion defined in a .wire.yaml file */
export interface Assertion {
  field: string;
  equals?: unknown;
  not_equals?: unknown;
  contains?: string;
  starts_with?: string;
  ends_with?: string;
  less_than?: number;
  greater_than?: number;
  is_array?: boolean;
  is_object?: boolean;
  is_string?: boolean;
  is_number?: boolean;
  exists?: boolean;
  body_contains?: string;
  body_matches?: string;
}

/** Result of evaluating a test assertion */
export interface TestResult {
  field: string;
  operator: string;
  passed: boolean;
  expected: string;
  actual: string;
}

/** Full wire request (matches Rust WireRequest) */
export interface WireRequest {
  name: string;
  method: string;
  url: string;
  headers: Record<string, string>;
  params: Record<string, string>;
  body: WireBody | null;
  extends?: string;
  tests?: Assertion[];
  response_schema?: [string, string][];
}

/** A history entry from the Rust backend */
export interface HistoryEntry {
  timestamp: string;
  name: string;
  method: string;
  url: string;
  status: number;
  elapsed_ms: number;
}

/** A single request entry in a collection */
export interface IpcRequestEntry {
  path: string;
  name: string;
  method: string;
}

/** Result of scanning a codebase for HTTP endpoints */
export interface IpcScanResult {
  framework: string;
  endpoints_found: number;
  files_scanned: number;
  collection: IpcCollectionInfo | null;
  wire_dir: string | null;
}

/** Collection info returned after opening a .wire/ directory */
export interface IpcCollectionInfo {
  name: string;
  version: number;
  active_env: string | null;
  default_templates: string[];
  requests: IpcRequestEntry[];
  environments: string[];
  templates: string[];
}
