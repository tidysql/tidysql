import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import Editor from '@monaco-editor/react'
import { useTidysqlWorkspace } from './hooks/useTidysqlWorkspace'
import { extractDialect, updateDialectInConfig } from './utils/config'
import { cancelIdle, scheduleIdle } from './utils/idle'
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
      { token: 'keyword', foreground: 'A626A4', fontStyle: 'bold' },
      { token: 'string', foreground: '50A14F' },
      { token: 'comment', foreground: '94A3B8' },
      { token: 'number', foreground: '4078F2' },
      { token: 'type.identifier', foreground: '4078F2' },
    ],
    colors: {
      'editor.background': '#ffffff',
      'editor.foreground': '#0d121b',
      'editorLineNumber.foreground': '#9aa3b2',
      'editorCursor.foreground': '#135bec',
      'editor.selectionBackground': '#135bec26',
      'editor.inactiveSelectionBackground': '#135bec1a',
      'editor.lineHighlightBackground': '#f5f7fb',
      'editorIndentGuide.background': '#e7ebf3',
      'editorIndentGuide.activeBackground': '#d2d8e4',
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

type MonacoDiagnostic = {
  message: string
  severity: 'error' | 'warning' | 'info' | 'hint'
  start: { line: number; column: number }
  end: { line: number; column: number }
  source?: 'sql' | 'config'
}

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
  const {
    workspaceRef,
    status: workspaceStatus,
    error: workspaceError,
    dialectOptions,
    dialectsReady,
  } = useTidysqlWorkspace()
  const wasmReady = workspaceStatus === 'ready'
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

  const runDiagnostics = useCallback((source: string) => {
    const workspace = workspaceRef.current

    if (!workspace) {
      return
    }

    let diagnostics: MonacoDiagnostic[] = []
    const configSnapshot = configRef.current

    try {
      diagnostics = workspace.check_with_config(
        source,
        configSnapshot
      ) as MonacoDiagnostic[]
    } catch {
      const monaco = monacoRef.current
      const sqlModel = editorRef.current?.getModel()
      const configModel = configEditorRef.current?.getModel()

      if (monaco && sqlModel) {
        monaco.editor.setModelMarkers(sqlModel, 'tidysql', [])
      }

      if (monaco && configModel) {
        monaco.editor.setModelMarkers(configModel, 'tidysql-config', [])
      }

      setDiagnostics([])
      return
    }

    setDiagnostics(diagnostics)

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

    const sqlMarkers = diagnostics
      .filter((diagnostic) => (diagnostic.source ?? 'sql') === 'sql')
      .map(toMarker)
    const configMarkers = diagnostics
      .filter((diagnostic) => diagnostic.source === 'config')
      .map(toMarker)

    if (sqlModel) {
      monaco.editor.setModelMarkers(sqlModel, 'tidysql', sqlMarkers)
    }

    if (configModel) {
      monaco.editor.setModelMarkers(configModel, 'tidysql-config', configMarkers)
    }
  }, [workspaceRef])

  const handleFormat = () => {
    if (!workspaceRef.current) {
      return
    }

    const source = editorRef.current?.getValue() ?? sql
    let formatted = source
    try {
      formatted = workspaceRef.current.format_with_config(
        source,
        configRef.current
      ) as string
    } catch {
      runDiagnostics(source)
      return
    }

    if (editorRef.current && formatted !== source) {
      editorRef.current.setValue(formatted)
      return
    }

    setSql(formatted)
  }

  const handleFixAll = () => {
    handleFormat()
  }

  const formatSource = (source: string) => {
    if (!workspaceRef.current) {
      return source
    }

    try {
      return workspaceRef.current.format_with_config(
        source,
        configRef.current
      ) as string
    } catch {
      runDiagnostics(source)
      return source
    }
  }

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
      runDiagnostics(sql)
      setDialectChangePending(false)
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

  const handleBeforeMount = (monaco: typeof import('monaco-editor')) => {
    configureMonaco(monaco)
  }

  const handleEditorScroll = (event: { scrollTop: number }) => {
    if (gutterRef.current) {
      gutterRef.current.scrollTop = event.scrollTop
    }
  }

  const handleSqlMount = (
    editor: import('monaco-editor').editor.IStandaloneCodeEditor,
    monaco: typeof import('monaco-editor')
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
      run: () => {
        const source = editor.getValue()
        const formatted = formatSource(source)
        if (formatted !== source) {
          editor.setValue(formatted)
        }
      },
    })

    editor.onDidScrollChange(handleEditorScroll)
  }

  const handleTomlMount = (
    editor: import('monaco-editor').editor.IStandaloneCodeEditor
  ) => {
    configEditorRef.current = editor
    editor.onDidScrollChange(handleEditorScroll)

    if (wasmReady) {
      scheduleIdle(() => {
        runDiagnostics(sql)
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
      wordWrap: 'on' as const,
      automaticLayout: true,
      fontFamily: '"JetBrains Mono", monospace',
      fontSize: 13,
      lineHeight: 24,
    }),
    [overlayRoot]
  )

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-left">
          <div className="brand">
            <div className="brand-icon">
              <span className="material-symbols-outlined" aria-hidden="true">
                database
              </span>
            </div>
            <h1 className="brand-name">TidySQL</h1>
          </div>
          <div className="header-divider" />
          <div className="select-group">
            <span className="material-symbols-outlined select-icon" aria-hidden="true">
              tune
            </span>
            <select
              className="select-control"
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
            <span className="material-symbols-outlined select-caret" aria-hidden="true">
              expand_more
            </span>
          </div>
          {activeLoadingLabel ? (
            <div
              className={`loading-pill${
                workspaceStatus === 'error' ? ' loading-pill-error' : ''
              }`}
              aria-live="polite"
            >
              {showSpinner ? (
                <span className="loading-spinner" aria-hidden="true" />
              ) : null}
              <span>{activeLoadingLabel}</span>
            </div>
          ) : null}
        </div>
        <div className="header-right">
          <div className="header-run">
            <button
              className="btn-ghost"
              type="button"
              onClick={handleFormat}
              disabled={!wasmReady}
            >
              Format
            </button>
            <button
              className="btn-primary"
              type="button"
              onClick={handleFixAll}
              disabled={!wasmReady || diagnostics.length === 0}
            >
              <span className="material-symbols-outlined" aria-hidden="true">
                auto_fix
              </span>
              Fix All ({diagnostics.length})
            </button>
          </div>
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
          <div className="issues-body">
            <div className="issue-list custom-scrollbar">
              {workspaceError ? (
                <div className="issue-empty issue-error" role="alert">
                  <div className="issue-error-title">Parser failed to load.</div>
                  <p className="issue-error-message">{workspaceError}</p>
                  <button
                    className="btn-ghost"
                    type="button"
                    onClick={() => window.location.reload()}
                  >
                    Reload
                  </button>
                </div>
              ) : !wasmReady ? (
                <div className="issue-empty">Initializing parser...</div>
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
                  const iconName =
                    severity === 'error'
                      ? 'error'
                      : severity === 'warning'
                      ? 'warning'
                      : 'info'

                  return (
                    <div className={`issue-item issue-item-${severity}`} key={index}>
                      <div className="issue-item-top">
                        <div className="issue-meta">
                          <span
                            className={`material-symbols-outlined issue-icon issue-icon-${severity}`}
                            aria-hidden="true"
                          >
                            {iconName}
                          </span>
                          <span
                            className={`issue-code${
                              severity === 'error'
                                ? ' issue-code-error'
                                : severity === 'warning'
                                ? ' issue-code-warning'
                                : ''
                            }`}
                          >
                            PARSE
                          </span>
                          <span className="issue-line">
                            Line {diagnostic.start.line}:{diagnostic.start.column}
                          </span>
                        </div>
                      </div>
                      <p className="issue-title">{diagnostic.message}</p>
                    </div>
                  )
                })
              )}
            </div>
          </div>
        </aside>
      </main>
    </div>
  )
}

export default App
