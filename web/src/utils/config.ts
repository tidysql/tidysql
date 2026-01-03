const isCoreHeader = (line: string) => /^\[\s*core\s*\]$/.test(line.trim())

const parseDialectValue = (line: string) => {
  const trimmed = line.trim()
  if (!trimmed || trimmed.startsWith('#')) {
    return null
  }

  const match = trimmed.match(/^dialect\s*=\s*(.+)$/)
  if (!match) {
    return null
  }

  const rawValue = match[1].split('#')[0].trim()
  if (rawValue.startsWith('"')) {
    const endIndex = rawValue.indexOf('"', 1)
    if (endIndex > 0) {
      return rawValue.slice(1, endIndex)
    }
  }

  if (rawValue.startsWith("'")) {
    const endIndex = rawValue.indexOf("'", 1)
    if (endIndex > 0) {
      return rawValue.slice(1, endIndex)
    }
  }

  const bareMatch = rawValue.match(/^([A-Za-z0-9_-]+)/)
  return bareMatch ? bareMatch[1] : null
}

export const extractDialect = (config: string) => {
  const lines = config.split('\n')
  let inCore = false

  for (const line of lines) {
    const trimmed = line.trim()

    if (isCoreHeader(trimmed)) {
      inCore = true
      continue
    }

    if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
      inCore = false
      continue
    }

    if (!inCore) {
      continue
    }

    const dialect = parseDialectValue(line)
    if (dialect) {
      return dialect
    }
  }

  return null
}

export const updateDialectInConfig = (config: string, dialect: string) => {
  const lines = config.split('\n')
  let inCore = false
  let coreFound = false
  let dialectUpdated = false

  for (let index = 0; index < lines.length; index += 1) {
    const trimmed = lines[index].trim()

    if (isCoreHeader(trimmed)) {
      inCore = true
      coreFound = true
      continue
    }

    if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
      if (inCore && !dialectUpdated) {
        lines.splice(index, 0, `dialect = "${dialect}"`)
        dialectUpdated = true
        break
      }
      inCore = false
      continue
    }

    if (inCore && /^dialect\s*=/.test(trimmed)) {
      lines[index] = `dialect = "${dialect}"`
      dialectUpdated = true
      break
    }
  }

  if (coreFound && !dialectUpdated) {
    lines.push(`dialect = "${dialect}"`)
    dialectUpdated = true
  }

  if (!coreFound) {
    if (lines.length === 1 && lines[0].trim() === '') {
      lines.length = 0
    } else if (lines.length > 0 && lines[lines.length - 1].trim() !== '') {
      lines.push('')
    }

    lines.push('[core]')
    lines.push(`dialect = "${dialect}"`)
  }

  return lines.join('\n')
}
