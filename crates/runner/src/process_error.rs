//! Process evidence retains native errors in the process that observed them and exports chrono-error/v1.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, error::Error, fmt, io};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ErrorSchema {
    #[serde(rename = "chrono-error/v1")]
    V1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorRecord {
    pub schema: ErrorSchema,
    pub error: ErrorNode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorNode {
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: String,
    pub message: String,
    pub context: BTreeMap<String, Value>,
    pub source: Option<Box<ErrorNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceback: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<RelatedError>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelatedError {
    pub role: String,
    pub error: ErrorNode,
}
impl ErrorNode {
    fn new(kind: &str, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error_type: kind.into(),
            code: code.into(),
            message: message.into(),
            context: BTreeMap::new(),
            native_code: None,
            traceback: None,
            source: None,
            related: Vec::new(),
        }
    }
}

pub trait RecordedError: Error + Send + Sync + 'static {
    fn error_node(&self) -> ErrorNode;
}

#[derive(Debug)]
pub struct NativeIoError(io::Error);
impl NativeIoError {
    pub(crate) fn into_inner(self) -> io::Error {
        self.0
    }

    pub fn new(error: io::Error) -> Self {
        Self(error)
    }
}
impl fmt::Display for NativeIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Error for NativeIoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}
impl RecordedError for NativeIoError {
    fn error_node(&self) -> ErrorNode {
        let kind = format!("{:?}", self.0.kind());
        let mut io = ErrorNode::new("std::io::Error", format!("io::{kind}"), self.to_string());
        io.context.insert("kind".into(), json!(kind));
        io.native_code = self.0.raw_os_error().map(i64::from);
        if let Some(source) = self.0.get_ref() {
            if let Some(recorded) = source.downcast_ref::<Failure>() {
                io.source = Some(Box::new(recorded.error_node()));
            } else {
                io.context
                    .insert("custom_source".into(), json!("not_recorded"));
            }
        }
        let mut cause = ErrorNode::new(
            std::any::type_name::<Self>(),
            "E_NATIVE_IO",
            self.to_string(),
        );
        cause.context.insert("kind".into(), json!(kind));
        cause.native_code = io.native_code;
        cause.source = Some(Box::new(io));
        cause
    }
}

/// Record the same native error object used by Rust callers, together with ordered secondary failures.
#[derive(Debug)]
pub struct Failure {
    node: ErrorNode,
    source: Option<Box<dyn RecordedError>>,
    related: Vec<(String, Box<dyn RecordedError>)>,
}
impl Failure {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            node: ErrorNode::new(std::any::type_name::<Self>(), code, message),
            source: None,
            related: Vec::new(),
        }
    }
    pub fn at(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.node.context.insert(key.into(), value.into());
        self
    }
    pub fn wrap(
        code: impl Into<String>,
        message: impl Into<String>,
        source: impl RecordedError,
    ) -> Self {
        let mut error = Self::new(code, message);
        error.source = Some(Box::new(source));
        error
    }
    pub fn with_related(mut self, role: impl Into<String>, error: impl RecordedError) -> Self {
        self.related.push((role.into(), Box::new(error)));
        self
    }
    pub fn related(&self) -> impl Iterator<Item = (&str, &dyn RecordedError)> {
        self.related
            .iter()
            .map(|(name, error)| (name.as_str(), error.as_ref()))
    }
    pub fn record(&self) -> ErrorRecord {
        ErrorRecord {
            schema: ErrorSchema::V1,
            error: self.error_node(),
        }
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.node.message)
    }
}
impl Error for Failure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|e| e as &dyn Error)
    }
}
impl RecordedError for Failure {
    fn error_node(&self) -> ErrorNode {
        let mut node = self.node.clone();
        node.source = self.source.as_ref().map(|e| Box::new(e.error_node()));
        node.related = self
            .related()
            .map(|(role, error)| RelatedError {
                role: role.into(),
                error: error.error_node(),
            })
            .collect();
        node
    }
}

impl fmt::Display for ErrorNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for ErrorNode {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|node| node as &dyn Error)
    }
}
/// A received record retains its wire tree and the native writer error if the caller observed it.
#[derive(Debug)]
pub struct ReceivedError {
    node: ErrorNode,
    writer: Option<io::Error>,
}
impl ReceivedError {
    pub(crate) fn with_native(record: ErrorRecord, writer: io::Error) -> Self {
        Self {
            node: record.error,
            writer: Some(writer),
        }
    }
}
impl From<ErrorRecord> for ReceivedError {
    fn from(record: ErrorRecord) -> Self {
        Self::from(record.error)
    }
}
impl From<ErrorNode> for ReceivedError {
    fn from(node: ErrorNode) -> Self {
        Self { node, writer: None }
    }
}
impl fmt::Display for ReceivedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.node.fmt(f)
    }
}
impl Error for ReceivedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.writer {
            Some(error) => Some(error),
            None => self.node.source(),
        }
    }
}
impl RecordedError for ReceivedError {
    fn error_node(&self) -> ErrorNode {
        self.node.clone()
    }
}
