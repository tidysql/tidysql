import { useEffect, useRef, useState } from 'react'
import init, { Workspace } from 'tidysql-wasm'
import { cancelIdle, scheduleIdle, type IdleCallbackHandle } from '../utils/idle'

export type DialectOption = {
  id: string
  label: string
}

type WorkspaceStatus = 'loading' | 'ready' | 'error'

type WorkspaceError = string | null

export const useTidysqlWorkspace = () => {
  const workspaceRef = useRef<Workspace | null>(null)
  const [status, setStatus] = useState<WorkspaceStatus>('loading')
  const [error, setError] = useState<WorkspaceError>(null)
  const [dialectOptions, setDialectOptions] = useState<DialectOption[]>([])
  const [dialectsReady, setDialectsReady] = useState(false)

  useEffect(() => {
    let active = true
    let dialectHandle: IdleCallbackHandle | null = null

    const start = async () => {
      setStatus('loading')
      setError(null)
      setDialectsReady(false)

      try {
        await init()
        if (!active) {
          return
        }

        const workspace = new Workspace()
        workspaceRef.current = workspace
        setStatus('ready')

        dialectHandle = scheduleIdle(() => {
          if (!active) {
            return
          }

          let dialects: DialectOption[] = []

          try {
            const result = workspace.dialects() as DialectOption[]
            if (Array.isArray(result)) {
              dialects = result
            }
          } catch {
            dialects = []
          }

          if (active) {
            setDialectOptions(dialects)
            setDialectsReady(true)
          }
        })
      } catch (err) {
        if (!active) {
          return
        }

        const message =
          err instanceof Error && err.message ? err.message : 'Failed to load parser.'
        setError(message)
        setStatus('error')
      }
    }

    start()

    return () => {
      active = false
      if (dialectHandle !== null) {
        cancelIdle(dialectHandle)
      }

      if (workspaceRef.current) {
        workspaceRef.current.free()
        workspaceRef.current = null
      }
    }
  }, [])

  return {
    workspaceRef,
    status,
    error,
    dialectOptions,
    dialectsReady,
  }
}
