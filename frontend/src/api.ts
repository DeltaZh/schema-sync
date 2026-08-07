/** 与后端 /api 契约对齐的前端客户端 */

export type RiskLevel = 'safe' | 'caution' | 'dangerous'

export type DiffKind =
  | 'create_table'
  | 'add_column'
  | 'modify_column'
  | 'drop_column'
  | 'add_index'
  | 'drop_index'
  | 'modify_index'

export interface Instance {
  id: string
  host: string
  port: number
  user: string
  /** GET 永不返回明文；有密码时为 ******** */
  password?: string
  has_password?: boolean
  enabled: boolean
  remark: string
}

export interface InstanceWrite {
  id: string
  host: string
  port: number
  user: string
  /** 创建时传明文；编辑时留空/省略表示不改 */
  password?: string | null
  enabled: boolean
  remark: string
}

export interface TableGroup {
  id: string
  database_pattern: string
  tables: string[]
  instance_ids: string[]
}

export interface DiffItem {
  id: string
  kind: DiffKind
  risk: RiskLevel
  instance_id: string
  database: string
  table: string
  title: string
  sql: string
  selected_default: boolean
}

export interface ScanError {
  instance_id: string
  database: string | null
  message: string
}

export interface ScanResult {
  items: DiffItem[]
  errors: ScanError[]
}

export interface ExecResult {
  diff_id: string
  ok: boolean
  error: string | null
}

export interface HistoryRecord {
  id: string
  ts: string
  group_id: string
  template_instance_id: string
  template_database: string
  stop_on_error: boolean
  results: ExecResult[]
  item_snapshots: DiffItem[]
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
    ...init,
  })
  if (!res.ok) {
    let detail = res.statusText
    try {
      const body = (await res.json()) as { detail?: unknown }
      if (typeof body.detail === 'string') {
        detail = body.detail
      } else if (body.detail != null) {
        detail = JSON.stringify(body.detail)
      }
    } catch {
      /* 非 JSON */
    }
    throw new Error(detail || `HTTP ${res.status}`)
  }
  if (res.status === 204) {
    return undefined as T
  }
  return (await res.json()) as T
}

export const api = {
  listInstances: () => request<Instance[]>('/api/instances'),

  createInstance: (body: InstanceWrite) =>
    request<Instance>('/api/instances', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  updateInstance: (id: string, body: InstanceWrite) =>
    request<Instance>(`/api/instances/${encodeURIComponent(id)}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),

  deleteInstance: (id: string) =>
    request<{ ok: boolean }>(`/api/instances/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    }),

  pingInstance: (id: string) =>
    request<{ ok: boolean }>(`/api/instances/${encodeURIComponent(id)}/ping`, {
      method: 'POST',
    }),

  listTableGroups: () => request<TableGroup[]>('/api/table-groups'),

  saveTableGroups: (groups: TableGroup[]) =>
    request<TableGroup[]>('/api/table-groups', {
      method: 'PUT',
      body: JSON.stringify(groups),
    }),

  listMatchedDatabases: (groupId: string, instanceId: string) =>
    request<string[]>(
      `/api/table-groups/${encodeURIComponent(groupId)}/databases?instance_id=${encodeURIComponent(instanceId)}`,
    ),

  scan: (body: {
    group_id: string
    template_instance_id: string
    template_database: string
  }) =>
    request<ScanResult>('/api/sync/scan', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  execute: (body: {
    items: DiffItem[]
    item_ids?: string[]
    stop_on_error: boolean
    group_id?: string
    template_instance_id?: string
    template_database?: string
  }) =>
    request<ExecResult[]>('/api/sync/execute', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  listHistory: (limit = 50) =>
    request<HistoryRecord[]>(`/api/history?limit=${limit}`),
}
