import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { Worker } from 'node:worker_threads'
import { performance } from 'node:perf_hooks'
import { fileURLToPath } from 'node:url'
import init, { Workspace } from '../src/tidysql-wasm/tidysql_wasm.js'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

const defaultConfigToml = `[core]
dialect = "ansi"
`

const buildLargeSql = (statementCount = 4000) =>
  Array.from(
    { length: statementCount },
    (_, index) =>
      `SELECT user_id AS id_${index}, created_at FROM events_${index} WHERE status = NULL OR score != 1 ORDER BY created_at;\n`
  ).join('')

const delay = (ms) => new Promise((resolve) => {
  setTimeout(resolve, ms)
})

const startEventLoopLagMonitor = (sampleMs = 10) => {
  let maxLagMs = 0
  let expected = performance.now() + sampleMs

  const handle = setInterval(() => {
    const now = performance.now()
    const lag = Math.max(0, now - expected)
    maxLagMs = Math.max(maxLagMs, lag)
    expected = now + sampleMs
  }, sampleMs)

  return {
    stop() {
      clearInterval(handle)
      return maxLagMs
    },
  }
}

const checksumDiagnostics = (diagnostics) => {
  const payload = JSON.stringify(diagnostics)
  let hash = 0

  for (let index = 0; index < payload.length; index += 1) {
    hash = (hash * 31 + payload.charCodeAt(index)) >>> 0
  }

  return hash.toString(16).padStart(8, '0')
}

const measureWithMainThreadLag = async (task) => {
  const lagMonitor = startEventLoopLagMonitor()
  await delay(20)

  const startedAt = performance.now()
  const result = await task()
  const wallTimeMs = performance.now() - startedAt

  await delay(20)

  return {
    result,
    wallTimeMs,
    mainThreadLagMs: lagMonitor.stop(),
  }
}

const createWorkerClient = async (wasmBytes) => {
  const worker = new Worker(new URL('./wasm-check-worker.mjs', import.meta.url), {
    type: 'module',
    workerData: { wasmBytes },
  })
  const pending = new Map()
  let nextRequestId = 1

  const awaitReady = new Promise((resolve, reject) => {
    const handleMessage = (message) => {
      if (message?.type === 'ready') {
        worker.off('message', handleMessage)
        resolve()
      }
    }

    worker.on('message', handleMessage)
    worker.once('error', reject)
  })

  worker.on('message', (message) => {
    if (message?.type === 'ready') {
      return
    }

    const pendingRequest = pending.get(message.id)

    if (!pendingRequest) {
      return
    }

    pending.delete(message.id)

    if (message.ok) {
      pendingRequest.resolve(message)
      return
    }

    pendingRequest.reject(new Error(message.error))
  })

  await awaitReady

  return {
    async check(source, configToml) {
      const id = nextRequestId
      nextRequestId += 1

      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject })
        worker.postMessage({ id, type: 'check', source, configToml })
      })
    },
    async dispose() {
      pending.forEach(({ reject }) => reject(new Error('Measurement worker disposed.')))
      pending.clear()
      worker.postMessage({ type: 'dispose' })
      await worker.terminate()
    },
  }
}

const wasmPath = path.resolve(__dirname, '../src/tidysql-wasm/tidysql_wasm_bg.wasm')
const wasmBytes = await readFile(wasmPath)
const sql = buildLargeSql()

await init({ module_or_path: wasmBytes })

const workspace = new Workspace()

const directMeasurement = await measureWithMainThreadLag(async () => {
  const diagnostics = workspace.check_with_config(sql, defaultConfigToml)
  return {
    diagnostics,
  }
})

const workerClient = await createWorkerClient(wasmBytes)
const workerMeasurement = await measureWithMainThreadLag(async () => {
  const result = await workerClient.check(sql, defaultConfigToml)
  return result
})

await workerClient.dispose()
workspace.free()

const directDiagnostics = directMeasurement.result.diagnostics
const workerDiagnostics = workerMeasurement.result.diagnostics
const parity = {
  directCount: directDiagnostics.length,
  workerCount: workerDiagnostics.length,
  directChecksum: checksumDiagnostics(directDiagnostics),
  workerChecksum: checksumDiagnostics(workerDiagnostics),
}

const parityMatches =
  parity.directCount === parity.workerCount &&
  parity.directChecksum === parity.workerChecksum

console.log(`Input statements: ${sql.trim().split('\n').length}`)
console.log('')
console.log('Direct main-thread check')
console.log(`  wall time: ${directMeasurement.wallTimeMs.toFixed(2)} ms`)
console.log(`  event-loop stall: ${directMeasurement.mainThreadLagMs.toFixed(2)} ms`)
console.log(`  diagnostics: ${parity.directCount}`)
console.log(`  checksum: ${parity.directChecksum}`)
console.log('')
console.log('Worker-thread check')
console.log(`  wall time: ${workerMeasurement.wallTimeMs.toFixed(2)} ms`)
console.log(`  worker compute: ${workerMeasurement.result.computeTimeMs.toFixed(2)} ms`)
console.log(`  main-thread stall: ${workerMeasurement.mainThreadLagMs.toFixed(2)} ms`)
console.log(`  diagnostics: ${parity.workerCount}`)
console.log(`  checksum: ${parity.workerChecksum}`)
console.log('')
console.log(`Parity: ${parityMatches ? 'match' : 'mismatch'}`)

if (!parityMatches) {
  process.exitCode = 1
}
