import { parentPort, workerData } from 'node:worker_threads'
import { performance } from 'node:perf_hooks'
import init, { Workspace } from '../src/tidysql-wasm/tidysql_wasm.js'

const serializeError = (error) => {
  if (error instanceof Error) {
    return {
      error: error.message || 'Unknown worker error.',
      ...(error.stack ? { stack: error.stack } : {}),
    }
  }

  if (typeof error === 'string') {
    return { error }
  }

  try {
    const serialized = JSON.stringify(error)
    return { error: serialized ?? String(error) }
  } catch {
    return { error: String(error) }
  }
}

await init({ module_or_path: workerData.wasmBytes })

const workspace = new Workspace()

parentPort.postMessage({ type: 'ready' })

parentPort.on('message', (message) => {
  if (message.type === 'dispose') {
    workspace.free()
    parentPort.close()
    return
  }

  if (message.type !== 'check') {
    parentPort.postMessage({
      id: message.id,
      ok: false,
      ...serializeError(`Unsupported worker message type: ${message.type}`),
    })
    return
  }

  const startedAt = performance.now()

  try {
    const diagnostics = workspace.check_with_config(message.source, message.configToml)
    parentPort.postMessage({
      id: message.id,
      ok: true,
      diagnostics,
      computeTimeMs: performance.now() - startedAt,
    })
  } catch (error) {
    parentPort.postMessage({
      id: message.id,
      ok: false,
      computeTimeMs: performance.now() - startedAt,
      ...serializeError(error),
    })
  }
})
