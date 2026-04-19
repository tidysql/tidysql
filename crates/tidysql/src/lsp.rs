use std::collections::HashMap;
use std::ops::Range as StdRange;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentFormattingParams, InitializeParams, InitializeResult, InitializedParams, MessageType,
    NumberOrString, Position, Range as LspRange, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::ConfigArguments;

const SOURCE: &str = "tidysql";
const CONFIG_ERROR_CODE: &str = "config_error";
type DocumentState = (String, i32);

struct AnalysisTask {
    version: i32,
    handle: JoinHandle<()>,
}

pub fn run(config: ConfigArguments) -> std::result::Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .build()
        .map_err(|err| err.to_string())?;

    runtime.block_on(async move { run_async(config).await })
}

async fn run_async(config: ConfigArguments) -> std::result::Result<(), String> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client, config));
    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}

struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
    in_flight: Arc<RwLock<HashMap<Url, AnalysisTask>>>,
    config: Arc<ConfigArguments>,
}

impl Backend {
    fn new(client: Client, config: ConfigArguments) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
        }
    }

    fn config_error_diagnostic(message: String) -> LspDiagnostic {
        LspDiagnostic {
            range: LspRange { start: Position::new(0, 0), end: Position::new(0, 0) },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(CONFIG_ERROR_CODE.to_string())),
            source: Some(SOURCE.to_string()),
            message,
            ..Default::default()
        }
    }

    async fn load_text(&self, uri: &Url) -> Option<(String, i32)> {
        if let Some(entry) = self.documents.read().await.get(uri).cloned() {
            return Some(entry);
        }

        let path = uri.to_file_path().ok()?;
        let text = std::fs::read_to_string(path).ok()?;
        Some((text, 0))
    }

    async fn update_document(&self, uri: Url, text: String, version: i32) {
        self.documents.write().await.insert(uri.clone(), (text.clone(), version));
        self.schedule_diagnostics(uri, text, version).await;
    }

    async fn schedule_diagnostics(&self, uri: Url, text: String, version: i32) {
        let handle = self.spawn_diagnostics_task(uri.clone(), text, version);
        if let Some(previous) =
            self.in_flight.write().await.insert(uri, AnalysisTask { version, handle })
        {
            previous.handle.abort();
        }
    }

    fn spawn_diagnostics_task(&self, uri: Url, text: String, version: i32) -> JoinHandle<()> {
        let client = self.client.clone();
        let config = self.config.clone();
        let in_flight = self.in_flight.clone();

        tokio::spawn(async move {
            let diagnostics = match tokio::task::spawn_blocking({
                let uri = uri.clone();
                move || compute_lsp_diagnostics(config, &uri, &text)
            })
            .await
            {
                Ok(Ok(diagnostics)) => diagnostics,
                Ok(Err(message)) => vec![Backend::config_error_diagnostic(message)],
                Err(_) => return,
            };

            if !is_latest_version(&in_flight, &uri, version).await {
                return;
            }

            client.publish_diagnostics(uri.clone(), diagnostics, Some(version)).await;

            let mut tasks = in_flight.write().await;
            if tasks.get(&uri).map(|task| task.version) == Some(version) {
                tasks.remove(&uri);
            }
        })
    }

    async fn refresh_open_documents(&self) {
        let open_documents = self.documents.read().await.clone();
        for (uri, (text, version)) in open_documents {
            self.schedule_diagnostics(uri, text, version).await;
        }
    }

    async fn abort_in_flight(&self, uri: &Url) {
        if let Some(task) = self.in_flight.write().await.remove(uri) {
            task.handle.abort();
        }
    }

    async fn document_version(&self, uri: &Url) -> i32 {
        self.documents.read().await.get(uri).map(|(_, version)| *version).unwrap_or(0)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "tidysql".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_formatting_provider: None,
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "tidysql LSP ready").await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = params.text_document.text;
        self.update_document(uri, text, version).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let Some(text) = params.content_changes.into_iter().last().map(|change| change.text) else {
            return;
        };
        self.update_document(uri, text, version).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if is_config_uri(&uri) {
            self.config.invalidate_resolver();
            self.refresh_open_documents().await;
            return;
        }

        let version = self.document_version(&uri).await;
        let text = match params.text {
            Some(text) => Some((text, version)),
            None => self.load_text(&uri).await,
        };

        if let Some((text, version)) = text {
            self.update_document(uri, text, version).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        self.abort_in_flight(&uri).await;
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let _ = params;
        Ok(None)
    }
}

fn compute_lsp_diagnostics(
    config_arguments: Arc<ConfigArguments>,
    uri: &Url,
    text: &str,
) -> std::result::Result<Vec<LspDiagnostic>, String> {
    let source_path = uri.to_file_path().ok();
    let source_path = source_path.as_deref().unwrap_or_else(|| Path::new("."));
    let config = config_arguments.load_config(source_path)?;
    let diagnostics = tidysql::check_with_config(text, &config);
    Ok(diagnostics.iter().filter_map(|diagnostic| to_lsp_diagnostic(diagnostic, text)).collect())
}

async fn is_latest_version(
    in_flight: &Arc<RwLock<HashMap<Url, AnalysisTask>>>,
    uri: &Url,
    version: i32,
) -> bool {
    in_flight.read().await.get(uri).map(|task| task.version) == Some(version)
}

fn is_config_uri(uri: &Url) -> bool {
    uri.to_file_path()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy() == "tidysql.toml"))
        .unwrap_or(false)
}

fn to_lsp_diagnostic(diagnostic: &tidysql::Diagnostic, text: &str) -> Option<LspDiagnostic> {
    let severity = lsp_severity(diagnostic.severity)?;
    let range = lsp_range(diagnostic.range.clone(), text);
    Some(LspDiagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(diagnostic.code.to_string())),
        source: Some(SOURCE.to_string()),
        message: diagnostic.message.clone(),
        ..Default::default()
    })
}

fn lsp_severity(severity: tidysql::Severity) -> Option<DiagnosticSeverity> {
    match severity {
        tidysql::Severity::Error => Some(DiagnosticSeverity::ERROR),
        tidysql::Severity::Warn => Some(DiagnosticSeverity::WARNING),
        tidysql::Severity::Info => Some(DiagnosticSeverity::INFORMATION),
        tidysql::Severity::Hint => Some(DiagnosticSeverity::HINT),
        tidysql::Severity::Allow => None,
    }
}

type ByteRange = StdRange<usize>;

fn lsp_range(range: ByteRange, text: &str) -> LspRange {
    let range = clamp_range(range, text.len());
    LspRange {
        start: offset_to_position(text, range.start),
        end: offset_to_position(text, range.end),
    }
}

fn clamp_range(range: ByteRange, source_len: usize) -> ByteRange {
    let start = range.start.min(source_len);
    let end = range.end.min(source_len);

    if end < start { start..start } else { start..end }
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut column = 0u32;
    let mut index = 0usize;
    let limit = offset.min(text.len());

    for ch in text.chars() {
        let ch_len = ch.len_utf8();
        if index + ch_len > limit {
            break;
        }

        if ch == '\n' {
            line += 1;
            column = 0;
        } else if ch != '\r' {
            column += ch.len_utf16() as u32;
        }

        index += ch_len;
    }

    Position::new(line, column)
}
