import { useCallback, useEffect, useRef, useState } from 'react'
import {
  type DialectOption,
  type MonacoDiagnostic,
  isTidysqlWorkerResponse,
  type TidysqlWorkerRequest,
  type TidysqlWorkerSuccessResponse,
} from '../workers/protocol'

type WorkspaceStatus = 'loading' | 'ready' | 'error'

type WorkspaceError = string | null

type PendingRequest = {
  type: TidysqlWorkerRequest['type']
  resolve: (response: TidysqlWorkerSuccessResponse) => void
  reject: (error: Error) => void
}

type TidysqlWorkerRequestPayload =
  | { type: 'dialects' }
  | { type: 'check'; source: string; configToml: string }
  | { type: 'format'; source: string; configToml: string }
  | { type: 'fix'; source: string; configToml: string }

const disposedError = () => new Error('Workspace worker was disposed.')

const toErrorMessage = (error: unknown) =>
  error instanceof Error && error.message ? error.message : 'Failed to load parser.'

export const useTidysqlWorkspace = () => {
  const workerRef = useRef<Worker | null>(null)
  const pendingRequestsRef = useRef<Map<number, PendingRequest>>(new Map())
  const nextRequestIdRef = useRef(1)
  const disposedRef = useRef(false)
  const [status, setStatus] = useState<WorkspaceStatus>('loading')
  const [error, setError] = useState<WorkspaceError>(null)
  const [dialectOptions, setDialectOptions] = useState<DialectOption[]>([])
  const [dialectsReady, setDialectsReady] = useState(false)

  const rejectPendingRequests = useCallback((failure: Error) => {
    pendingRequestsRef.current.forEach(({ reject }) => reject(failure))
    pendingRequestsRef.current.clear()
  }, [])

  const destroyWorker = useCallback(() => {
    if (workerRef.current) {
      workerRef.current.terminate()
      workerRef.current = null
    }
  }, [])

  const handleFatalWorkerError = useCallback(
    (message: string) => {
      rejectPendingRequests(new Error(message))
      destroyWorker()

      if (disposedRef.current) {
        return
      }

      setStatus('error')
      setError(message)
      setDialectsReady(false)
      setDialectOptions([])
    },
    [destroyWorker, rejectPendingRequests]
  )

  const handleWorkerMessage = useCallback(
    (event: MessageEvent) => {
      const response = event.data

      if (!isTidysqlWorkerResponse(response)) {
        handleFatalWorkerError('Workspace worker returned an invalid response.')
        return
      }

      const pendingRequest = pendingRequestsRef.current.get(response.id)

      if (!pendingRequest) {
        return
      }

      pendingRequestsRef.current.delete(response.id)

      if (pendingRequest.type !== response.type) {
        pendingRequest.reject(
          new Error(`Workspace worker responded with the wrong type for ${pendingRequest.type}.`)
        )
        return
      }

      if (!response.ok) {
        pendingRequest.reject(new Error(response.error))
        return
      }

      pendingRequest.resolve(response)
    },
    [handleFatalWorkerError]
  )

  const ensureWorker = useCallback(() => {
    if (disposedRef.current) {
      throw disposedError()
    }

    if (workerRef.current) {
      return workerRef.current
    }

    const worker = new Worker(new URL('../workers/tidysqlWorker.ts', import.meta.url), {
      type: 'module',
    })

    worker.onmessage = (event) => {
      handleWorkerMessage(event)
    }
    worker.onerror = (event) => {
      handleFatalWorkerError(event.message || 'Workspace worker failed.')
    }
    worker.onmessageerror = () => {
      handleFatalWorkerError('Workspace worker received an unreadable message.')
    }

    workerRef.current = worker
    return worker
  }, [handleFatalWorkerError, handleWorkerMessage])

  const sendRequest = useCallback(
    (request: TidysqlWorkerRequestPayload): Promise<TidysqlWorkerSuccessResponse> => {
      const worker = ensureWorker()
      const id = nextRequestIdRef.current
      nextRequestIdRef.current += 1

      return new Promise((resolve, reject) => {
        pendingRequestsRef.current.set(id, {
          type: request.type,
          resolve: resolve as PendingRequest['resolve'],
          reject,
        })

        try {
          worker.postMessage({ ...request, id })
        } catch (postMessageError) {
          pendingRequestsRef.current.delete(id)
          reject(
            postMessageError instanceof Error
              ? postMessageError
              : new Error('Failed to post a message to the workspace worker.')
          )
        }
      })
    },
    [ensureWorker]
  )

  const dialects = useCallback(async () => {
    const response = await sendRequest({ type: 'dialects' })

    if (response.type !== 'dialects') {
      throw new Error('Workspace worker returned an unexpected dialects response.')
    }

    return response.dialects
  }, [sendRequest])

  const checkWithConfig = useCallback(
    async (source: string, configToml: string): Promise<MonacoDiagnostic[]> => {
      const response = await sendRequest({ type: 'check', source, configToml })

      if (response.type !== 'check') {
        throw new Error('Workspace worker returned an unexpected diagnostics response.')
      }

      return response.diagnostics
    },
    [sendRequest]
  )

  const formatWithConfig = useCallback(
    async (source: string, configToml: string) => {
      const response = await sendRequest({ type: 'format', source, configToml })

      if (response.type !== 'format') {
        throw new Error('Workspace worker returned an unexpected format response.')
      }

      return response.sql
    },
    [sendRequest]
  )

  const fixWithConfig = useCallback(
    async (source: string, configToml: string) => {
      const response = await sendRequest({ type: 'fix', source, configToml })

      if (response.type !== 'fix') {
        throw new Error('Workspace worker returned an unexpected fix response.')
      }

      return response.sql
    },
    [sendRequest]
  )

  useEffect(() => {
    disposedRef.current = false
    let active = true

    const start = async () => {
      try {
        const nextDialectOptions = await dialects()
        if (!active || disposedRef.current) {
          return
        }

        setDialectOptions(nextDialectOptions)
        setDialectsReady(true)
        setStatus('ready')
      } catch (loadError) {
        if (!active || disposedRef.current) {
          return
        }

        setStatus('error')
        setError(toErrorMessage(loadError))
        setDialectsReady(false)
        setDialectOptions([])
      }
    }

    void start()

    return () => {
      active = false
      disposedRef.current = true
      rejectPendingRequests(disposedError())
      destroyWorker()
    }
  }, [destroyWorker, dialects, rejectPendingRequests])

  return {
    status,
    error,
    dialectOptions,
    dialectsReady,
    dialects,
    checkWithConfig,
    formatWithConfig,
    fixWithConfig,
  }
}
