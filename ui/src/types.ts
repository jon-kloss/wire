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
  chain?: ChainStepDef[];
}

/** A chain step definition from a .wire.yaml file */
export interface ChainStepDef {
  run: string;
  extract?: Record<string, string>;
  persist?: boolean;
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

/** Drift detection result */
export interface DriftItem {
  category: "new" | "stale" | "changed";
  method: string;
  route: string;
  name: string;
  changes: string[];
  request_path: string | null;
}

export interface DriftReport {
  items: DriftItem[];
  new_count: number;
  stale_count: number;
  changed_count: number;
}

/** Result of a single chain step execution */
export interface ChainStepResult {
  step_index: number;
  request_name: string;
  request_path: string;
  status: number;
  elapsed_ms: number;
  extracted: Record<string, string>;
  passed: boolean;
  error: string | null;
}

/** Result of executing an entire chain */
export interface ChainResult {
  steps: ChainStepResult[];
  success: boolean;
  total_elapsed_ms: number;
  error: string | null;
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
  source_dir: string | null;
}
