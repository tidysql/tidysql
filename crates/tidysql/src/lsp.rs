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
use crate::paths::normalize_path;

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
        if should_invalidate_resolver_for_uri(self.config.as_ref(), &uri) {
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
    if is_config_document_uri(config_arguments.as_ref(), uri) {
        let path = uri.to_file_path().ok();
        tidysql_config::parse_config(text, path).map_err(|err| err.to_string())?;
        return Ok(Vec::new());
    }

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

fn should_invalidate_resolver_for_uri(config_arguments: &ConfigArguments, uri: &Url) -> bool {
    is_config_document_uri(config_arguments, uri)
}

fn is_config_document_uri(config_arguments: &ConfigArguments, uri: &Url) -> bool {
    let Ok(path) = uri.to_file_path() else {
        return false;
    };

    is_default_config_path(&path)
        || is_explicit_config_path(config_arguments, &path)
        || config_arguments.resolver.has_loaded_config_path(&path)
}

fn is_default_config_path(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy() == tidysql_config::DEFAULT_CONFIG_FILE)
        .unwrap_or(false)
}

fn is_explicit_config_path(config_arguments: &ConfigArguments, path: &Path) -> bool {
    config_arguments
        .config_path
        .as_deref()
        .map(|config_path| normalize_path(config_path) == normalize_path(path))
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use futures::StreamExt;
    use serde_json::json;
    use tempfile::tempdir;
    use tower::{Service, ServiceExt};

    use super::*;
    use crate::{ConfigOverrideArgs, GlobalConfigArgs};

    fn test_config_arguments(explicit: Option<PathBuf>) -> Arc<ConfigArguments> {
        Arc::new(ConfigArguments::from_cli_arguments(
            GlobalConfigArgs { config: explicit, isolated: false },
            ConfigOverrideArgs {
                dialect: None,
                allow: Vec::new(),
                warn: Vec::new(),
                deny: Vec::new(),
            }
            .into(),
        ))
    }

    async fn initialize_service(service: &mut LspService<Backend>) {
        let response = service
            .ready()
            .await
            .unwrap()
            .call(
                tower_lsp::jsonrpc::Request::build("initialize")
                    .params(json!({ "capabilities": {} }))
                    .id(1)
                    .finish(),
            )
            .await
            .unwrap();

        assert!(response.is_some(), "initialize should return a response");
    }

    async fn next_publish_diagnostics(socket: &mut tower_lsp::ClientSocket) -> serde_json::Value {
        loop {
            let request = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .expect("timed out waiting for server notification")
                .expect("server notification stream ended");
            if request.method() == "textDocument/publishDiagnostics" {
                return request.params().cloned().expect("diagnostics request should have params");
            }
        }
    }

    #[test]
    fn config_documents_are_validated_as_config_not_sql() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(tidysql_config::DEFAULT_CONFIG_FILE);
        let uri = Url::from_file_path(&path).unwrap();
        let config = test_config_arguments(None);

        let diagnostics = compute_lsp_diagnostics(
            config,
            &uri,
            r#"
[core]
dialect = "ansi"
"#,
        )
        .unwrap();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn invalid_config_documents_return_config_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(tidysql_config::DEFAULT_CONFIG_FILE);
        let uri = Url::from_file_path(&path).unwrap();
        let config = test_config_arguments(None);

        let error = compute_lsp_diagnostics(
            config,
            &uri,
            r#"
[files]
respect_gitigore = true
"#,
        )
        .unwrap_err();

        assert!(error.contains("unknown field `respect_gitigore`"));
    }

    #[test]
    fn explicit_custom_config_paths_are_treated_as_config_documents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("custom-config.toml");
        let uri = Url::from_file_path(&path).unwrap();
        let config = test_config_arguments(Some(path.clone()));

        assert!(is_config_document_uri(config.as_ref(), &uri));
    }

    #[test]
    fn loaded_extended_config_paths_are_treated_as_config_documents() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("base.toml");
        let child = dir.path().join(tidysql_config::DEFAULT_CONFIG_FILE);
        let sql = dir.path().join("query.sql");

        std::fs::write(
            &parent,
            r#"
[core]
dialect = "postgres"
"#,
        )
        .unwrap();
        std::fs::write(
            &child,
            r#"
extend = "base.toml"
"#,
        )
        .unwrap();
        std::fs::write(&sql, "select 1\n").unwrap();

        let config = test_config_arguments(None);
        config.load_config(&sql).unwrap();

        let parent_uri = Url::from_file_path(&parent).unwrap();
        assert!(is_config_document_uri(config.as_ref(), &parent_uri));
        assert!(should_invalidate_resolver_for_uri(config.as_ref(), &parent_uri));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saving_loaded_parent_config_refreshes_open_sql_diagnostics() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("base.toml");
        let child = dir.path().join(tidysql_config::DEFAULT_CONFIG_FILE);
        let sql = dir.path().join("query.sql");

        std::fs::write(
            &parent,
            r#"
[lints]
require_order_by = { level = "allow" }
"#,
        )
        .unwrap();
        std::fs::write(
            &child,
            r#"
extend = "base.toml"
"#,
        )
        .unwrap();
        std::fs::write(&sql, "SELECT * FROM foo LIMIT 10\n").unwrap();

        let config = ConfigArguments::from_cli_arguments(
            GlobalConfigArgs { config: None, isolated: false },
            ConfigOverrideArgs {
                dialect: None,
                allow: Vec::new(),
                warn: Vec::new(),
                deny: Vec::new(),
            }
            .into(),
        );
        let (mut service, mut socket) = LspService::new(|client| Backend::new(client, config));
        initialize_service(&mut service).await;

        let sql_uri = Url::from_file_path(&sql).unwrap();
        service
            .inner()
            .did_open(DidOpenTextDocumentParams {
                text_document: tower_lsp::lsp_types::TextDocumentItem {
                    uri: sql_uri.clone(),
                    language_id: "sql".to_string(),
                    version: 1,
                    text: "SELECT * FROM foo LIMIT 10\n".to_string(),
                },
            })
            .await;

        let initial = next_publish_diagnostics(&mut socket).await;
        assert_eq!(
            initial["uri"].as_str(),
            Some(sql_uri.as_str()),
            "initial diagnostics should target the open SQL document"
        );
        assert_eq!(
            initial["diagnostics"].as_array().map(Vec::len),
            Some(0),
            "initial config disables require_order_by"
        );

        std::fs::write(
            &parent,
            r#"
[lints]
require_order_by = { level = "warn" }
"#,
        )
        .unwrap();

        let parent_uri = Url::from_file_path(&parent).unwrap();
        service
            .inner()
            .did_save(DidSaveTextDocumentParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri: parent_uri },
                text: None,
            })
            .await;

        let refreshed = next_publish_diagnostics(&mut socket).await;
        assert_eq!(
            refreshed["uri"].as_str(),
            Some(sql_uri.as_str()),
            "saving the loaded parent config should refresh open SQL diagnostics"
        );
        assert_eq!(
            refreshed["diagnostics"].as_array().map(Vec::len),
            Some(1),
            "refreshed diagnostics should reflect the updated parent config"
        );
    }
}
