//! Emelino's LSP transport. Analysis remains in the shared public frontend API.
mod queries;
mod text;
mod workspace;

use crate::project;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types as lsp;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use workspace::CheckedProject;

pub const HELP: &str = "Usage: emelino lsp\n\nRuns the language server over stdio for Emelino.toml projects.\nProvides diagnostics, completion, hover and go to definition without an SDK.\nIn the Emela VS Code extension, set emela.serverPath to this executable.";

pub fn run_stdio() -> Result<(), String> {
    let (connection, threads) = Connection::stdio();
    let result = serve(&connection);
    drop(connection);
    if result.is_ok() {
        threads.join().map_err(|e| e.to_string())?;
    }
    result
}

struct Document {
    text: String,
    version: i32,
}

struct Server {
    roots: Vec<PathBuf>,
    documents: BTreeMap<PathBuf, Document>,
    published: BTreeSet<PathBuf>,
}

fn serve(connection: &Connection) -> Result<(), String> {
    let (id, params) = connection.initialize_start().map_err(|e| e.to_string())?;
    let params: lsp::InitializeParams =
        serde_json::from_value(params).map_err(|e| e.to_string())?;
    let capabilities = json!({
        "capabilities": {
            "positionEncoding":"utf-16",
            "textDocumentSync": {"openClose":true,"change":1,"save":{"includeText":true}},
            "completionProvider":{"triggerCharacters":["."]},
            "hoverProvider":true,"definitionProvider":true
        },
        "serverInfo":{"name":"emelino","version":env!("CARGO_PKG_VERSION")}
    });
    connection
        .initialize_finish(id, capabilities)
        .map_err(|e| e.to_string())?;
    #[allow(deprecated)]
    let roots: Vec<_> = params
        .workspace_folders
        .into_iter()
        .flatten()
        .filter_map(|f| text::path(&f.uri))
        .chain(params.root_uri.as_ref().and_then(text::path))
        .collect();
    if params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|w| w.did_change_watched_files.as_ref())
        .and_then(|w| w.dynamic_registration)
        == Some(true)
    {
        send(connection, Request::new(RequestId::from("emelino-watch-files".to_owned()), "client/registerCapability".into(),
            json!({"registrations":[{"id":"emelino-project-files","method":"workspace/didChangeWatchedFiles","registerOptions":{"watchers":[{"globPattern":"**/Emelino.toml"},{"globPattern":"**/*.emel"}]}}]})).into())?;
    }
    let mut server = Server {
        roots,
        documents: BTreeMap::new(),
        published: BTreeSet::new(),
    };
    server.publish(connection)?;
    let mut shutdown = false;
    for message in &connection.receiver {
        match message {
            Message::Request(request) if request.method == "shutdown" => {
                shutdown = true;
                send(connection, Response::new_ok(request.id, Value::Null).into())?;
            }
            Message::Request(request) => {
                let response = if shutdown {
                    Response::new_err(
                        request.id,
                        ErrorCode::InvalidRequest as i32,
                        "server has shut down".into(),
                    )
                } else {
                    server.request(request)
                };
                send(connection, response.into())?;
            }
            Message::Notification(notification) if notification.method == "exit" => {
                return if shutdown {
                    Ok(())
                } else {
                    Err("exit received before shutdown".into())
                };
            }
            Message::Notification(notification) if !shutdown => {
                match server.notification(notification) {
                    Ok(true) => server.publish(connection)?,
                    Ok(false) => (),
                    Err(error) => eprintln!("emelino lsp: {error}"),
                }
            }
            _ => (),
        }
    }
    Ok(())
}

fn send(connection: &Connection, message: Message) -> Result<(), String> {
    connection.sender.send(message).map_err(|e| e.to_string())
}

impl Server {
    fn overlay(&self) -> BTreeMap<PathBuf, String> {
        self.documents
            .iter()
            .map(|(path, doc)| (path.clone(), doc.text.clone()))
            .collect()
    }

    fn notification(&mut self, notification: Notification) -> Result<bool, String> {
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let params: lsp::DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params).map_err(|e| e.to_string())?;
                if let Some(path) = text::path(&params.text_document.uri) {
                    self.documents.insert(
                        path,
                        Document {
                            text: params.text_document.text,
                            version: params.text_document.version,
                        },
                    );
                    return Ok(true);
                }
            }
            "textDocument/didChange" => {
                let params: lsp::DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params).map_err(|e| e.to_string())?;
                if let Some(path) = text::path(&params.text_document.uri)
                    && let Some(doc) = self.documents.get_mut(&path)
                {
                    if params.text_document.version <= doc.version {
                        return Ok(false);
                    }
                    let mut value = doc.text.clone();
                    for change in params.content_changes {
                        if let Some(range) = change.range {
                            let start =
                                text::offset(&value, range.start).ok_or("invalid change start")?;
                            let end =
                                text::offset(&value, range.end).ok_or("invalid change end")?;
                            if start > end {
                                return Err("invalid change range".into());
                            }
                            value.replace_range(usize::from(start)..usize::from(end), &change.text);
                        } else {
                            value = change.text;
                        }
                    }
                    doc.text = value;
                    doc.version = params.text_document.version;
                    return Ok(true);
                }
            }
            "textDocument/didSave" => {
                let params: lsp::DidSaveTextDocumentParams =
                    serde_json::from_value(notification.params).map_err(|e| e.to_string())?;
                if let Some(path) = text::path(&params.text_document.uri)
                    && let Some(doc) = self.documents.get_mut(&path)
                    && let Some(text) = params.text
                {
                    doc.text = text;
                }
                return Ok(true);
            }
            "textDocument/didClose" => {
                let params: lsp::DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params).map_err(|e| e.to_string())?;
                if let Some(path) = text::path(&params.text_document.uri) {
                    self.documents.remove(&path);
                    return Ok(true);
                }
            }
            "workspace/didChangeWatchedFiles" => return Ok(true),
            "workspace/didChangeWorkspaceFolders" => {
                let params: lsp::DidChangeWorkspaceFoldersParams =
                    serde_json::from_value(notification.params).map_err(|e| e.to_string())?;
                for removed in params.event.removed {
                    if let Some(path) = text::path(&removed.uri) {
                        self.roots.retain(|p| p != &path);
                    }
                }
                self.roots.extend(
                    params
                        .event
                        .added
                        .into_iter()
                        .filter_map(|f| text::path(&f.uri)),
                );
                return Ok(true);
            }
            _ => (),
        }
        Ok(false)
    }

    fn request(&self, request: Request) -> Response {
        if !matches!(
            request.method.as_str(),
            "textDocument/hover" | "textDocument/definition" | "textDocument/completion"
        ) {
            return Response::new_err(
                request.id,
                ErrorCode::MethodNotFound as i32,
                format!("unsupported method: {}", request.method),
            );
        }
        let params: lsp::TextDocumentPositionParams = match serde_json::from_value(request.params) {
            Ok(params) => params,
            Err(error) => {
                return Response::new_err(
                    request.id,
                    ErrorCode::InvalidParams as i32,
                    error.to_string(),
                );
            }
        };
        let result = self.query(&request.method, &params).unwrap_or(Value::Null);
        Response::new_ok(request.id, result)
    }

    fn query(&self, method: &str, params: &lsp::TextDocumentPositionParams) -> Option<Value> {
        let path = text::path(&params.text_document.uri)?;
        let overlay = self.overlay();
        let project = workspace::discover(&path, &overlay).ok()?;
        let checked = CheckedProject::check(project, &overlay).ok()?;
        let module = checked.module_at(&path)?;
        let source = checked.analysis.texts.get(module)?;
        let offset = text::offset(source, params.position)?;
        match method {
            "textDocument/hover" => {
                let (range, value) = queries::hover(&checked.analysis, module, offset)?;
                serde_json::to_value(lsp::Hover {
                    contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                        kind: lsp::MarkupKind::Markdown,
                        value,
                    }),
                    range: Some(text::range(source, range)),
                })
                .ok()
            }
            "textDocument/definition" => {
                let span = queries::definition(&checked.analysis, module, offset)?;
                let uri = text::uri(&checked.module_path(span.module)?).ok()?;
                let source = checked.analysis.texts.get(span.module)?;
                serde_json::to_value(lsp::Location::new(uri, text::range(source, span.range))).ok()
            }
            "textDocument/completion" => {
                let mut items = queries::complete(&checked.analysis, module, offset);
                let before = source.get(..usize::from(offset))?;
                let start = before
                    .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_')
                    .len();
                let range =
                    text::range(source, rowan::TextRange::new((start as u32).into(), offset));
                for item in &mut items {
                    item.text_edit = Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range,
                        new_text: item.label.clone(),
                    }));
                }
                serde_json::to_value(lsp::CompletionList {
                    is_incomplete: false,
                    items,
                })
                .ok()
            }
            _ => None,
        }
    }

    fn publish(&mut self, connection: &Connection) -> Result<(), String> {
        let overlay = self.overlay();
        let mut projects = BTreeMap::new();
        let mut diagnostics: BTreeMap<PathBuf, Vec<lsp::Diagnostic>> = BTreeMap::new();
        for path in self.roots.iter().chain(self.documents.keys()) {
            match workspace::discover(path, &overlay) {
                Ok(project) => {
                    projects.entry(project.root.clone()).or_insert(project);
                }
                Err(error) if self.documents.contains_key(path) => {
                    let anchor = workspace::anchor(path, &overlay);
                    let result = diagnostics.entry(anchor).or_default();
                    if !result.iter().any(|d| d.message == error) {
                        result.push(simple_diagnostic(error));
                    }
                }
                Err(_) => (),
            }
        }
        for (_, project) in projects {
            let manifest = project.root.join(project::MANIFEST);
            diagnostics.entry(manifest.clone()).or_default();
            match CheckedProject::check(project, &overlay) {
                Ok(checked) => {
                    for (module, _) in checked.analysis.texts.iter() {
                        if let Some(path) = checked.module_path(module) {
                            diagnostics.entry(path).or_default();
                        }
                    }
                    for diagnostic in &checked.analysis.diagnostics {
                        let module = checked
                            .analysis
                            .modules
                            .iter()
                            .find_map(|(m, data)| (data.file == diagnostic.file).then_some(m));
                        let path =
                            module
                                .and_then(|m| checked.module_path(m))
                                .unwrap_or_else(|| {
                                    checked
                                        .project
                                        .root
                                        .join(&checked.project.manifest.project.entry)
                                });
                        let range = module
                            .and_then(|m| checked.analysis.texts.get(m))
                            .map(|source| text::range(source, diagnostic.range.unwrap_or_default()))
                            .unwrap_or_default();
                        diagnostics.entry(path).or_default().push(lsp::Diagnostic {
                            range,
                            severity: Some(if diagnostic.error {
                                lsp::DiagnosticSeverity::ERROR
                            } else {
                                lsp::DiagnosticSeverity::WARNING
                            }),
                            code: diagnostic.code.clone().map(lsp::NumberOrString::String),
                            source: Some("emelino".into()),
                            message: diagnostic.message.clone(),
                            ..lsp::Diagnostic::default()
                        });
                    }
                }
                Err(error) => diagnostics
                    .entry(manifest)
                    .or_default()
                    .push(simple_diagnostic(error)),
            }
        }
        let current: BTreeSet<_> = diagnostics.keys().cloned().collect();
        for old in self.published.difference(&current) {
            self.publish_one(connection, old, Vec::new())?;
        }
        for (path, items) in diagnostics {
            self.publish_one(connection, &path, items)?;
        }
        self.published = current;
        Ok(())
    }

    fn publish_one(
        &self,
        connection: &Connection,
        path: &Path,
        diagnostics: Vec<lsp::Diagnostic>,
    ) -> Result<(), String> {
        let params = lsp::PublishDiagnosticsParams {
            uri: text::uri(path)?,
            diagnostics,
            version: self.documents.get(path).map(|d| d.version),
        };
        send(
            connection,
            Notification::new("textDocument/publishDiagnostics".into(), params).into(),
        )
    }
}

fn simple_diagnostic(message: String) -> lsp::Diagnostic {
    lsp::Diagnostic {
        severity: Some(lsp::DiagnosticSeverity::ERROR),
        source: Some("emelino".into()),
        message,
        ..lsp::Diagnostic::default()
    }
}
