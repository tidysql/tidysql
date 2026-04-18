export type DialectOption = {
  id: string
  label: string
}

export type MonacoPosition = {
  line: number
  column: number
}

export type MonacoDiagnosticSeverity = 'error' | 'warning' | 'info' | 'hint'

export type MonacoDiagnosticSource = 'sql' | 'config'

export type MonacoDiagnostic = {
  message: string
  severity: MonacoDiagnosticSeverity
  start: MonacoPosition
  end: MonacoPosition
  source?: MonacoDiagnosticSource
}

export type TidysqlWorkerRequest =
  | { id: number; type: 'dialects' }
  | { id: number; type: 'check'; source: string; configToml: string }
  | { id: number; type: 'format'; source: string; configToml: string }
  | { id: number; type: 'fix'; source: string; configToml: string }

export type TidysqlWorkerErrorResponse = {
  id: number
  ok: false
  type: TidysqlWorkerRequest['type']
  error: string
  stack?: string
}

export type TidysqlWorkerSuccessResponse =
  | { id: number; ok: true; type: 'dialects'; dialects: DialectOption[] }
  | { id: number; ok: true; type: 'check'; diagnostics: MonacoDiagnostic[] }
  | { id: number; ok: true; type: 'format'; sql: string }
  | { id: number; ok: true; type: 'fix'; sql: string }

export type TidysqlWorkerResponse =
  | TidysqlWorkerSuccessResponse
  | TidysqlWorkerErrorResponse

type SerializableError = {
  error: string
  stack?: string
}

const stringifyFallback = (value: unknown) => {
  try {
    const serialized = JSON.stringify(value)
    return serialized ?? String(value)
  } catch {
    return String(value)
  }
}

export const serializeWorkerError = (error: unknown): SerializableError => {
  if (error instanceof Error) {
    return {
      error: error.message || 'Unknown worker error.',
      ...(error.stack ? { stack: error.stack } : {}),
    }
  }

  if (typeof error === 'string') {
    return { error }
  }

  if (error && typeof error === 'object' && 'message' in error) {
    const message = error.message
    if (typeof message === 'string' && message) {
      const stack = 'stack' in error && typeof error.stack === 'string' ? error.stack : undefined
      return { error: message, ...(stack ? { stack } : {}) }
    }
  }

  return {
    error: stringifyFallback(error) || 'Unknown worker error.',
  }
}

export const isTidysqlWorkerResponse = (
  value: unknown
): value is TidysqlWorkerResponse => {
  if (!value || typeof value !== 'object') {
    return false
  }

  const candidate = value as Partial<TidysqlWorkerResponse>

  return (
    typeof candidate.id === 'number' &&
    typeof candidate.type === 'string' &&
    typeof candidate.ok === 'boolean'
  )
}
