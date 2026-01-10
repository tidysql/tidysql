use std::collections::HashMap;
use std::ops::Range as StdRange;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentFormattingParams, InitializeParams, InitializeResult, InitializedParams, MessageType,
    NumberOrString, OneOf, Position, Range as LspRange, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::ConfigArguments;

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
    documents: RwLock<HashMap<Url, String>>,
    config: Arc<ConfigArguments>,
}

impl Backend {
    fn new(client: Client, config: ConfigArguments) -> Self {
        Self { client, documents: RwLock::new(HashMap::new()), config: Arc::new(config) }
    }

    async fn publish_diagnostics(&self, uri: Url, text: &str) {
        let config = self.load_config(&uri).await;
        let diagnostics = tidysql::check_with_config(text, &config);
        let lsp_diagnostics = diagnostics
            .iter()
            .filter_map(|diagnostic| to_lsp_diagnostic(diagnostic, text))
            .collect();
        self.client.publish_diagnostics(uri, lsp_diagnostics, None).await;
    }

    async fn load_config(&self, uri: &Url) -> tidysql_config::Config {
        let source_path = uri.to_file_path().ok();
        let source_path = source_path.as_deref().unwrap_or_else(|| Path::new("."));
        match self.config.load_config(source_path) {
            Ok(config) => config,
            Err(message) => {
                self.client.log_message(MessageType::ERROR, message).await;
                tidysql_config::Config::default()
            }
        }
    }

    async fn load_text(&self, uri: &Url) -> Option<String> {
        if let Some(text) = self.documents.read().await.get(uri).cloned() {
            return Some(text);
        }

        let path = uri.to_file_path().ok()?;
        std::fs::read_to_string(path).ok()
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
                document_formatting_provider: Some(OneOf::Left(true)),
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
        let text = params.text_document.text;
        self.documents.write().await.insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = match params.content_changes.into_iter().last() {
            Some(change) => change.text,
            None => return,
        };
        self.documents.write().await.insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = match params.text {
            Some(text) => Some(text),
            None => self.load_text(&uri).await,
        };

        if let Some(text) = text {
            self.documents.write().await.insert(uri.clone(), text.clone());
            self.publish_diagnostics(uri, &text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let text = match self.load_text(&uri).await {
            Some(text) => text,
            None => return Ok(None),
        };

        let config = self.load_config(&uri).await;
        let formatted = tidysql::format_with_config(&text, &config);
        let range = full_document_range(&text);
        Ok(Some(vec![TextEdit { range, new_text: formatted }]))
    }
}

fn to_lsp_diagnostic(diagnostic: &tidysql::Diagnostic, text: &str) -> Option<LspDiagnostic> {
    let severity = lsp_severity(diagnostic.severity)?;
    let range = lsp_range(diagnostic.range.clone(), text);
    Some(LspDiagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(diagnostic.code.to_string())),
        source: Some("tidysql".to_string()),
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

fn full_document_range(text: &str) -> LspRange {
    LspRange { start: Position::new(0, 0), end: offset_to_position(text, text.len()) }
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
