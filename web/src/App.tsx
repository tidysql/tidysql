import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import Editor from '@monaco-editor/react'
import { useTidysqlWorkspace } from './hooks/useTidysqlWorkspace'
import { extractDialect, updateDialectInConfig } from './utils/config'
import { cancelIdle, scheduleIdle } from './utils/idle'
import type { MonacoDiagnostic } from './workers/protocol'
import './App.css'

const initialSql = `SELECT id, name
FROM users
WHERE active = true;
`

const defaultConfigToml = `[core]
dialect = "ansi"
`

let monacoConfigured = false

const configureMonaco = (monaco: typeof import('monaco-editor')) => {
  if (monacoConfigured) {
    return
  }
  monacoConfigured = true

  if (!monaco.languages.getLanguages().some((language) => language.id === 'toml')) {
    monaco.languages.register({ id: 'toml' })
    monaco.languages.setMonarchTokensProvider('toml', {
      defaultToken: '',
      tokenPostfix: '.toml',
      escapes: /\\(u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8}|[btnfr"\\/])/,
      tokenizer: {
        root: [
          [/^\s*\[+.*\]+/, 'type.identifier'],
          [/(#.*$)/, 'comment'],
          [/([A-Za-z0-9_.-]+)(\s*)(=)/, ['identifier', '', 'delimiter']],
          { include: '@whitespace' },
          [/"([^"\\]|\\.)*$/, 'string.invalid'],
          [/"/, { token: 'string.quote', bracket: '@open', next: '@string' }],
          [/'[^']*'/, 'string'],
          [/\b(true|false)\b/, 'keyword'],
          [/\b\d+(\.\d+)?\b/, 'number'],
          [/[{}()[\]]/, '@brackets'],
          [/[,:]/, 'delimiter'],
        ],
        whitespace: [
          [/[ \t\r\n]+/, 'white'],
          [/(#.*$)/, 'comment'],
        ],
        string: [
          [/[^\\"]+/, 'string'],
          [/@escapes/, 'string.escape'],
          [/\\./, 'string.escape.invalid'],
          [/"/, { token: 'string.quote', bracket: '@close', next: '@pop' }],
        ],
      },
    })
  }

  monaco.editor.defineTheme('tidysql-light', {
    base: 'vs',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '7C3AED', fontStyle: 'bold' },
      { token: 'string', foreground: '047857' },
      { token: 'comment', foreground: '9CA3AF' },
      { token: 'number', foreground: '2563EB' },
      { token: 'type.identifier', foreground: '2563EB' },
      { token: 'identifier', foreground: '111111' },
    ],
    colors: {
      'editor.background': '#ffffff',
      'editor.foreground': '#111111',
      'editorLineNumber.foreground': '#a3a3a3',
      'editorCursor.foreground': '#111111',
      'editor.selectionBackground': '#dbeafe',
      'editor.inactiveSelectionBackground': '#eff6ff',
      'editor.lineHighlightBackground': '#ffffff',
      'editorIndentGuide.background': '#e5e7eb',
      'editorIndentGuide.activeBackground': '#d4d4d8',
      'editorWhitespace.foreground': '#e5e7eb',
    },
  })
}

const ensureMonacoOverlayRoot = () => {
  if (typeof document === 'undefined') {
    return null
  }

  let root = document.getElementById('monaco-overlays')

  if (!root) {
    root = document.createElement('div')
    root.id = 'monaco-overlays'
    root.className = 'monaco-editor'
    document.body.appendChild(root)
  }

  return root as HTMLDivElement
}

type MonacoEditor = import('monaco-editor').editor.IStandaloneCodeEditor
type MonacoModule = typeof import('monaco-editor')

function App() {
  const [sql, setSql] = useState(initialSql)
  const [configToml, setConfigToml] = useState(defaultConfigToml)
  const [dialect, setDialect] = useState(
    () => extractDialect(defaultConfigToml) ?? 'ansi'
  )
  const [activeTab, setActiveTab] = useState<'sql' | 'toml'>('sql')
  const [editorReady, setEditorReady] = useState(false)
  const [dialectChangePending, setDialectChangePending] = useState(false)
  const [diagnostics, setDiagnostics] = useState<MonacoDiagnostic[]>([])
  const configRef = useRef(configToml)
  const sqlRef = useRef(sql)
  const latestDiagnosticsTokenRef = useRef(0)
  const {
    status: workspaceStatus,
    error: workspaceError,
    dialectOptions,
    dialectsReady,
    checkWithConfig,
    formatWithConfig,
    fixWithConfig,
  } = useTidysqlWorkspace()
  const wasmReady = workspaceStatus === 'ready'
  const wasmReadyRef = useRef(wasmReady)
  const checkWithConfigRef = useRef(checkWithConfig)
  const formatWithConfigRef = useRef(formatWithConfig)
  const fixWithConfigRef = useRef(fixWithConfig)
  const overlayRoot = useMemo(() => ensureMonacoOverlayRoot(), [])
  const activeSource = activeTab === 'sql' ? sql : configToml
  const lineNumbers = useMemo(() => {
    const lineCount = Math.max(1, activeSource.split('\n').length)
    return Array.from({ length: lineCount }, (_, index) => index + 1)
  }, [activeSource])
  const hasDialectOption = useMemo(
    () => dialectOptions.some((option) => option.id === dialect),
    [dialectOptions, dialect]
  )
  const sqlDiagnosticsCount = useMemo(
    () =>
      diagnostics.filter((diagnostic) => (diagnostic.source ?? 'sql') === 'sql')
        .length,
    [diagnostics]
  )
  const loadingLabel = useMemo(() => {
    if (workspaceStatus === 'loading') {
      return 'Loading parser...'
    }
    if (workspaceStatus === 'error') {
      return 'Parser failed to load'
    }
    if (!dialectsReady) {
      return 'Loading dialects...'
    }
    if (dialectOptions.length === 0) {
      return 'No dialects available'
    }
    return ''
  }, [dialectOptions.length, dialectsReady, workspaceStatus])
  const activeLoadingLabel = useMemo(
    () => loadingLabel || (dialectChangePending ? 'Applying dialect...' : ''),
    [loadingLabel, dialectChangePending]
  )
  const showSpinner = useMemo(() => {
    if (!activeLoadingLabel) {
      return false
    }
    if (workspaceStatus === 'error') {
      return false
    }
    if (dialectChangePending) {
      return true
    }
    return workspaceStatus === 'loading' || !dialectsReady
  }, [activeLoadingLabel, workspaceStatus, dialectChangePending, dialectsReady])
  const gutterRef = useRef<HTMLDivElement | null>(null)
  const editorRef = useRef<import('monaco-editor').editor.IStandaloneCodeEditor | null>(
    null
  )
  const configEditorRef = useRef<
    import('monaco-editor').editor.IStandaloneCodeEditor | null
  >(null)
  const monacoRef = useRef<typeof import('monaco-editor') | null>(null)

  useEffect(() => {
    configRef.current = configToml
  }, [configToml])

  useEffect(() => {
    sqlRef.current = sql
  }, [sql])

  useEffect(() => {
    wasmReadyRef.current = wasmReady
  }, [wasmReady])

  useEffect(() => {
    checkWithConfigRef.current = checkWithConfig
  }, [checkWithConfig])

  useEffect(() => {
    formatWithConfigRef.current = formatWithConfig
  }, [formatWithConfig])

  useEffect(() => {
    fixWithConfigRef.current = fixWithConfig
  }, [fixWithConfig])

  const clearDiagnosticsMarkers = useCallback(() => {
    const monaco = monacoRef.current
    const sqlModel = editorRef.current?.getModel()
    const configModel = configEditorRef.current?.getModel()

    if (monaco && sqlModel) {
      monaco.editor.setModelMarkers(sqlModel, 'tidysql', [])
    }

    if (monaco && configModel) {
      monaco.editor.setModelMarkers(configModel, 'tidysql-config', [])
    }
  }, [])

  const applyDiagnosticsMarkers = useCallback((nextDiagnostics: MonacoDiagnostic[]) => {
    const monaco = monacoRef.current
    const sqlModel = editorRef.current?.getModel()
    const configModel = configEditorRef.current?.getModel()

    if (!monaco) {
      return
    }

    const severityMap: Record<MonacoDiagnostic['severity'], number> = {
      error: monaco.MarkerSeverity.Error,
      warning: monaco.MarkerSeverity.Warning,
      info: monaco.MarkerSeverity.Info,
      hint: monaco.MarkerSeverity.Hint,
    }

    const toMarker = (diagnostic: MonacoDiagnostic) => {
      const startLineNumber = diagnostic.start.line
      const startColumn = diagnostic.start.column
      const endLineNumber = diagnostic.end.line
      let endColumn = diagnostic.end.column

      if (startLineNumber === endLineNumber && startColumn === endColumn) {
        endColumn = startColumn + 1
      }

      return {
        startLineNumber,
        startColumn,
        endLineNumber,
        endColumn,
        message: diagnostic.message,
        severity: severityMap[diagnostic.severity] ?? monaco.MarkerSeverity.Info,
      }
    }

    const sqlMarkers = nextDiagnostics
      .filter((diagnostic) => (diagnostic.source ?? 'sql') === 'sql')
      .map(toMarker)
    const configMarkers = nextDiagnostics
      .filter((diagnostic) => diagnostic.source === 'config')
      .map(toMarker)

    if (sqlModel) {
      monaco.editor.setModelMarkers(sqlModel, 'tidysql', sqlMarkers)
    }

    if (configModel) {
      monaco.editor.setModelMarkers(configModel, 'tidysql-config', configMarkers)
    }
  }, [])

  const runDiagnostics = useCallback(async (source: string) => {
    const requestToken = latestDiagnosticsTokenRef.current + 1
    latestDiagnosticsTokenRef.current = requestToken

    try {
      const nextDiagnostics = await checkWithConfigRef.current(source, configRef.current)

      if (latestDiagnosticsTokenRef.current !== requestToken) {
        return
      }

      setDiagnostics(nextDiagnostics)
      applyDiagnosticsMarkers(nextDiagnostics)
    } catch {
      if (latestDiagnosticsTokenRef.current !== requestToken) {
        return
      }

      clearDiagnosticsMarkers()
      setDiagnostics([])
    }
  }, [applyDiagnosticsMarkers, clearDiagnosticsMarkers])

  const runSqlTransform = useCallback(
    async (operation: 'format' | 'fix', sourceOverride?: string, editorOverride?: MonacoEditor | null) => {
      if (!wasmReadyRef.current) {
        return
      }

      const activeEditor = editorOverride ?? editorRef.current
      const source = sourceOverride ?? activeEditor?.getValue() ?? sqlRef.current
      const expectedVersion = activeEditor?.getModel()?.getAlternativeVersionId() ?? null

      try {
        const transformed =
          operation === 'format'
            ? await formatWithConfigRef.current(source, configRef.current)
            : await fixWithConfigRef.current(source, configRef.current)

        const latestEditor = editorOverride ?? editorRef.current
        const latestModel = latestEditor?.getModel()

        // Drop stale async edits instead of overwriting newer typing.
        if (
          expectedVersion !== null &&
          latestModel &&
          latestModel.getAlternativeVersionId() !== expectedVersion
        ) {
          await runDiagnostics(latestEditor?.getValue() ?? sqlRef.current)
          return
        }

        if (latestEditor) {
          const latestSource = latestEditor.getValue()

          if (transformed !== latestSource) {
            latestEditor.setValue(transformed)
          }

          return
        }

        setSql(transformed)
      } catch {
        await runDiagnostics(source)
      }
    },
    [runDiagnostics]
  )

  const handleFormat = useCallback(() => {
    if (!wasmReady) {
      return
    }

    void runSqlTransform('format')
  }, [runSqlTransform, wasmReady])

  const handleFixAll = useCallback(() => {
    if (!wasmReady) {
      return
    }

    void runSqlTransform('fix')
  }, [runSqlTransform, wasmReady])

  const updateConfigToml = (nextConfig: string) => {
    setConfigToml(nextConfig)
    const nextDialect = extractDialect(nextConfig)

    if (!nextDialect) {
      return
    }

    setDialect((current) => (current === nextDialect ? current : nextDialect))
  }

  useEffect(() => {
    if (!wasmReady || !editorReady) {
      return
    }

    const handle = scheduleIdle(() => {
      void runDiagnostics(sql).finally(() => {
        setDialectChangePending(false)
      })
    })

    return () => {
      cancelIdle(handle)
    }
  }, [sql, wasmReady, configToml, editorReady, runDiagnostics])

  useEffect(() => {
    const activeEditor =
      activeTab === 'sql' ? editorRef.current : configEditorRef.current

    if (activeEditor) {
      activeEditor.layout()
      if (gutterRef.current) {
        gutterRef.current.scrollTop = activeEditor.getScrollTop()
      }
    }
  }, [activeTab])

  const handleBeforeMount = (monaco: MonacoModule) => {
    configureMonaco(monaco)
  }

  const handleEditorScroll = (event: { scrollTop: number }) => {
    if (gutterRef.current) {
      gutterRef.current.scrollTop = event.scrollTop
    }
  }

  const handleSqlMount = (
    editor: MonacoEditor,
    monaco: MonacoModule
  ) => {
    editorRef.current = editor
    monacoRef.current = monaco
    setEditorReady(true)

    editor.addAction({
      id: 'tidysql.format',
      label: 'Format Document',
      keybindings: [monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyF],
      contextMenuGroupId: '1_modification',
      contextMenuOrder: 1.5,
      run: async () => {
        await runSqlTransform('format', editor.getValue(), editor)
      },
    })

    editor.onDidScrollChange(handleEditorScroll)
  }

  const handleTomlMount = (editor: MonacoEditor) => {
    configEditorRef.current = editor
    editor.onDidScrollChange(handleEditorScroll)

    if (wasmReady) {
      scheduleIdle(() => {
        void runDiagnostics(sqlRef.current)
      })
    }
  }

  const editorOptions = useMemo(
    () => ({
      minimap: { enabled: false },
      lineNumbers: 'off' as const,
      lineDecorationsWidth: 0,
      lineNumbersMinChars: 0,
      glyphMargin: false,
      folding: false,
      fixedOverflowWidgets: true,
      ...(overlayRoot ? { overflowWidgetsDomNode: overlayRoot } : {}),
      hover: { enabled: true, above: false, sticky: true },
      renderLineHighlight: 'none' as const,
      scrollBeyondLastLine: false,
      wordWrap: 'off' as const,
      automaticLayout: true,
      fontFamily: '"JetBrains Mono", monospace',
      fontSize: 15,
      lineHeight: 32,
      padding: { top: 24, bottom: 24 },
    }),
    [overlayRoot]
  )

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <h1 className="brand-name">TidySQL</h1>
        </div>
        <div className="header-right">
          {activeLoadingLabel ? (
            <div
              className={`header-status${
                workspaceStatus === 'error' ? ' header-status-error' : ''
              }`}
              aria-live="polite"
            >
              {activeLoadingLabel}
            </div>
          ) : null}
          <div className="select-group">
            <select
              className="control-select"
              value={dialectOptions.length && hasDialectOption ? dialect : ''}
              disabled={!dialectOptions.length}
              onChange={(event) => {
                const nextDialect = event.target.value
                setDialectChangePending(true)
                setDialect(nextDialect)
                setConfigToml((current) => updateDialectInConfig(current, nextDialect))
              }}
            >
              {dialectOptions.length && !hasDialectOption ? (
                <option value="">Custom</option>
              ) : null}
              {dialectOptions.length ? (
                dialectOptions.map((option) => (
                  <option key={option.id} value={option.id}>
                    {option.label}
                  </option>
                ))
              ) : (
                <option value="">
                  {workspaceStatus === 'error'
                    ? 'Dialects unavailable'
                    : dialectsReady
                    ? 'No dialects available'
                    : 'Loading dialects...'}
                </option>
              )}
            </select>
            <svg
              aria-hidden="true"
              className="select-caret"
              viewBox="0 0 20 20"
              fill="none"
            >
              <path
                d="M5.5 7.5L10 12l4.5-4.5"
                stroke="currentColor"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="1.7"
              />
            </svg>
          </div>
          <button
            className="control-button"
            type="button"
            onClick={handleFormat}
            disabled={!wasmReady}
          >
            Format
          </button>
          <button
            className="control-button control-button-accent"
            type="button"
            onClick={handleFixAll}
            disabled={!wasmReady || sqlDiagnosticsCount === 0}
          >
            Fix All ({sqlDiagnosticsCount})
          </button>
        </div>
      </header>
      <main className="app-main">
        <section className="editor-panel">
          <div className="editor-tabs">
            <button
              className={`editor-tab${activeTab === 'sql' ? ' editor-tab-active' : ''}`}
              type="button"
              onClick={() => setActiveTab('sql')}
              aria-pressed={activeTab === 'sql'}
            >
              SQL
            </button>
            <button
              className={`editor-tab${
                activeTab === 'toml' ? ' editor-tab-active' : ''
              }`}
              type="button"
              onClick={() => setActiveTab('toml')}
              aria-pressed={activeTab === 'toml'}
            >
              Config
            </button>
          </div>
          <div className="editor-body">
            <div
              className="editor-gutter code-font"
              ref={gutterRef}
              onWheel={(event) => {
                event.preventDefault()
                const activeEditor =
                  activeTab === 'sql' ? editorRef.current : configEditorRef.current

                if (activeEditor) {
                  activeEditor.setScrollTop(
                    activeEditor.getScrollTop() + event.deltaY
                  )
                }
              }}
            >
              {lineNumbers.map((lineNumber) => (
                <div className="gutter-line" key={lineNumber}>
                  {lineNumber}
                </div>
              ))}
            </div>
            <div className="editor-code">
              <div
                className={`editor-surface${
                  activeTab === 'sql' ? '' : ' editor-surface-hidden'
                }`}
              >
                <Editor
                  height="100%"
                  width="100%"
                  language="sql"
                  value={sql}
                  theme="tidysql-light"
                  beforeMount={handleBeforeMount}
                  onMount={handleSqlMount}
                  onChange={(value) => setSql(value ?? '')}
                  options={editorOptions}
                />
              </div>
              <div
                className={`editor-surface${
                  activeTab === 'toml' ? '' : ' editor-surface-hidden'
                }`}
              >
                <Editor
                  height="100%"
                  width="100%"
                  language="toml"
                  value={configToml}
                  theme="tidysql-light"
                  beforeMount={handleBeforeMount}
                  onMount={handleTomlMount}
                  onChange={(value) => updateConfigToml(value ?? '')}
                  options={editorOptions}
                />
              </div>
            </div>
          </div>
        </section>
        <aside className="issues-panel">
          <div className="issues-header">
            <h2 className="issues-title">
              Issues <span className="issues-count">{diagnostics.length}</span>
            </h2>
          </div>
          <div className="issue-list custom-scrollbar">
            {workspaceError ? (
              <div className="issue-empty issue-error" role="alert">
                <div className="issue-error-title">Parser failed to load.</div>
                <p className="issue-error-message">{workspaceError}</p>
                <button
                  className="control-button"
                  type="button"
                  onClick={() => window.location.reload()}
                >
                  Reload
                </button>
              </div>
            ) : !wasmReady ? (
              <div className="issue-empty">
                {showSpinner ? 'Initializing parser...' : 'Loading workspace...'}
              </div>
            ) : diagnostics.length === 0 ? (
              <div className="issue-empty">No issues to display.</div>
            ) : (
              diagnostics.map((diagnostic, index) => {
                const severity =
                  diagnostic.severity === 'error'
                    ? 'error'
                    : diagnostic.severity === 'warning'
                    ? 'warning'
                    : 'style'

                return (
                  <div className={`issue-item issue-item-${severity}`} key={index}>
                    <div className="issue-meta">
                      <span
                        className={`issue-badge${
                          severity === 'error'
                            ? ' issue-badge-error'
                            : severity === 'warning'
                            ? ' issue-badge-warning'
                            : ''
                        }`}
                      >
                        {diagnostic.severity}
                      </span>
                      <span className="issue-line">
                        Line {diagnostic.start.line}:{diagnostic.start.column}
                      </span>
                    </div>
                    <p className="issue-title">{diagnostic.message}</p>
                  </div>
                )
              })
            )}
          </div>
        </aside>
      </main>
    </div>
  )
}

export default App
