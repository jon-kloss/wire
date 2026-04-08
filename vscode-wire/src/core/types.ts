/**
 * Wire core types — TypeScript equivalents of the Rust structs in wire-core.
 * These are used by both the embedded sender and CLI JSON output parsing.
 */

// ─── Request Types ───────────────────────────────────────────────────────────

export interface WireRequest {
  name: string;
  method: string;
  url: string;
  headers: Record<string, string>;
  params: Record<string, string>;
  body?: Body;
  extends?: string;
  tests: Assertion[];
  response_schema: Array<[string, string]>;
  chain: ChainStep[];
  snapshot?: SnapshotConfig;
}

export interface Body {
  type: BodyType;
  content: unknown;
}

export type BodyType = 'json' | 'text' | 'formdata';

// ─── Collection Types ────────────────────────────────────────────────────────

export interface WireCollection {
  name: string;
  version: number;
  active_env?: string;
  default_template?: string;
  default_templates: string[];
  source_dir?: string;
}

export interface Environment {
  name: string;
  variables: Record<string, string>;
}

// ─── Test Assertion Types ────────────────────────────────────────────────────

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

export interface TestResult {
  field: string;
  operator: string;
  passed: boolean;
  expected: string;
  actual: string;
}

// ─── Chain Types ─────────────────────────────────────────────────────────────

export interface ChainStep {
  run: string;
  extract: Record<string, string>;
  persist: boolean;
}

export interface ChainStepResult {
  step_index: number;
  request_name: string;
  request_path: string;
  status: number;
  status_text: string;
  elapsed_ms: number;
  extracted: Record<string, string>;
  passed: boolean;
  error?: string;
  request_method: string;
  request_url: string;
  request_headers: Record<string, string>;
  response_headers: Record<string, string>;
  response_body: string;
}

export interface ChainResult {
  steps: ChainStepResult[];
  success: boolean;
  total_elapsed_ms: number;
  error?: string;
}

// ─── HTTP Response Types ─────────────────────────────────────────────────────

export interface WireResponse {
  status: number;
  status_text: string;
  headers: Record<string, string>;
  body: string;
  elapsed_ms: number;
  size_bytes: number;
}

// ─── Snapshot Types ──────────────────────────────────────────────────────────

export interface SnapshotConfig {
  ignore: string[];
}

export interface Snapshot {
  status: number;
  headers: Record<string, string>;
  body: unknown;
}

// ─── Diff Types ──────────────────────────────────────────────────────────────

export type DiffKind = 'Added' | 'Removed' | 'Changed';

export interface DiffEntry {
  path: string;
  kind: DiffKind;
  old?: unknown;
  new?: unknown;
}

// ─── Test Runner Output Types ────────────────────────────────────────────────

export interface TestRunSummary {
  results: RequestTestResult[];
  total_assertions: number;
  passed: number;
  failed: number;
  errors: number;
}

export interface RequestTestResult {
  file: string;
  name: string;
  method: string;
  url: string;
  status?: number;
  assertions: TestResult[];
  error?: string;
  response_body?: string;
  headers?: Record<string, string>;
}

// ─── Drift Detection Types ───────────────────────────────────────────────────

export type DriftCategory = 'new' | 'stale' | 'changed';

export interface DriftItem {
  category: DriftCategory;
  method: string;
  route: string;
  name: string;
  changes: string[];
  request_path?: string;
}

export interface DriftReport {
  items: DriftItem[];
  new_count: number;
  stale_count: number;
  changed_count: number;
}

// ─── Breaking Change Types ───────────────────────────────────────────────────

export type Severity = 'breaking' | 'warning' | 'info';

export interface ContractChange {
  severity: Severity;
  method: string;
  route: string;
  description: string;
}

export interface BreakingReport {
  changes: ContractChange[];
  breaking_count: number;
  warning_count: number;
  info_count: number;
}

// ─── History Types ───────────────────────────────────────────────────────────

export interface HistoryEntry {
  timestamp: string;
  name: string;
  method: string;
  url: string;
  status: number;
  elapsed_ms: number;
}

// ─── Collection Info (from IPC / wire list) ──────────────────────────────────

export interface CollectionInfo {
  name: string;
  version: number;
  active_env?: string;
  default_templates: string[];
  requests: RequestEntry[];
  environments: string[];
  templates: string[];
  source_dir?: string;
}

export interface RequestEntry {
  path: string;
  name: string;
  method: string;
}

// ─── Scan Result (from wire generate) ────────────────────────────────────────

export interface ScanResult {
  framework: string;
  endpoints_found: number;
  files_scanned: number;
  collection?: CollectionInfo;
  wire_dir?: string;
}
