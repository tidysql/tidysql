/// <reference lib="webworker" />

import init, { Workspace } from 'tidysql-wasm'
import {
  serializeWorkerError,
  type DialectOption,
  type MonacoDiagnostic,
  type MonacoDiagnosticSeverity,
  type TidysqlWorkerRequest,
  type TidysqlWorkerResponse,
} from './protocol'

const workerScope = self as DedicatedWorkerGlobalScope

let workspace: Workspace | null = null
let workspaceInitPromise: Promise<Workspace> | null = null

const normalizeDialectOption = (value: unknown): DialectOption => {
  if (!value || typeof value !== 'object') {
    return { id: '', label: '' }
  }

  const candidate = value as Partial<DialectOption>
  const id = typeof candidate.id === 'string' ? candidate.id : ''
  const label = typeof candidate.label === 'string' ? candidate.label : id

  return { id, label }
}

const normalizeSeverity = (value: unknown): MonacoDiagnosticSeverity => {
  switch (value) {
    case 'error':
    case 'warning':
    case 'hint':
      return value
    case 'info':
    default:
      return 'info'
  }
}

const normalizeDiagnostic = (value: unknown): MonacoDiagnostic => {
  if (!value || typeof value !== 'object') {
    return {
      message: '',
      severity: 'info',
      start: { line: 1, column: 1 },
      end: { line: 1, column: 1 },
      source: 'sql',
    }
  }

  const candidate = value as Partial<MonacoDiagnostic>
  const start = candidate.start ?? { line: 1, column: 1 }
  const end = candidate.end ?? start

  return {
    message: typeof candidate.message === 'string' ? candidate.message : '',
    severity: normalizeSeverity(candidate.severity),
    start: {
      line: typeof start.line === 'number' ? start.line : 1,
      column: typeof start.column === 'number' ? start.column : 1,
    },
    end: {
      line: typeof end.line === 'number' ? end.line : 1,
      column: typeof end.column === 'number' ? end.column : 1,
    },
    source: candidate.source === 'config' ? 'config' : 'sql',
  }
}

const normalizeDialects = (value: unknown): DialectOption[] => {
  if (!Array.isArray(value)) {
    return []
  }

  return value.map(normalizeDialectOption)
}

const normalizeDiagnostics = (value: unknown): MonacoDiagnostic[] => {
  if (!Array.isArray(value)) {
    return []
  }

  return value.map(normalizeDiagnostic)
}

const getWorkspace = async () => {
  if (workspace) {
    return workspace
  }

  if (!workspaceInitPromise) {
    workspaceInitPromise = (async () => {
      await init()
      const nextWorkspace = new Workspace()
      workspace = nextWorkspace
      return nextWorkspace
    })().catch((error) => {
      workspaceInitPromise = null
      throw error
    })
  }

  return workspaceInitPromise
}

const dispatchRequest = async (
  request: TidysqlWorkerRequest
): Promise<TidysqlWorkerResponse> => {
  try {
    const activeWorkspace = await getWorkspace()

    switch (request.type) {
      case 'dialects':
        return {
          id: request.id,
          ok: true,
          type: 'dialects',
          dialects: normalizeDialects(activeWorkspace.dialects()),
        }
      case 'check':
        return {
          id: request.id,
          ok: true,
          type: 'check',
          diagnostics: normalizeDiagnostics(
            activeWorkspace.check_with_config(request.source, request.configToml)
          ),
        }
      case 'format':
        return {
          id: request.id,
          ok: true,
          type: 'format',
          sql: activeWorkspace.format_with_config(request.source, request.configToml),
        }
      case 'fix':
        return {
          id: request.id,
          ok: true,
          type: 'fix',
          sql: activeWorkspace.fix_with_config(request.source, request.configToml),
        }
    }
  } catch (error) {
    const serialized = serializeWorkerError(error)
    return {
      id: request.id,
      ok: false,
      type: request.type,
      error: serialized.error,
      ...(serialized.stack ? { stack: serialized.stack } : {}),
    }
  }
}

workerScope.onmessage = async (event: MessageEvent<TidysqlWorkerRequest>) => {
  const response = await dispatchRequest(event.data)
  workerScope.postMessage(response)
}

