use std::fmt;

use serde::{Deserialize, Serialize};

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: msg.into(),
            span: None,
            notes: vec![],
            help: None,
        }
    }

    pub fn warning(msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: msg.into(),
            span: None,
            notes: vec![],
            help: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    pub entries: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self { entries: vec![] }
    }

    pub fn push(&mut self, diag: Diagnostic) {
        self.entries.push(diag);
    }

    pub fn has_errors(&self) -> bool {
        self.entries
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn extend(&mut self, other: Self) {
        self.entries.extend(other.entries);
    }
}

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for diag in &self.entries {
            writeln!(f, "{:?}: {}", diag.severity, diag.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_diagnostic() {
        let d = Diagnostic::error("something broke");
        assert_eq!(d.severity, Severity::Error);
        assert!(d.span.is_none());
    }

    #[test]
    fn test_warning_diagnostic() {
        let d = Diagnostic::warning("be careful").with_note("here").with_help("do this");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.notes.len(), 1);
        assert!(d.help.is_some());
    }

    #[test]
    fn test_diagnostics_collection() {
        let mut diags = Diagnostics::new();
        diags.push(Diagnostic::error("err1"));
        diags.push(Diagnostic::warning("warn1"));
        assert!(diags.has_errors());
    }

    #[test]
    fn test_diagnostics_no_errors() {
        let mut diags = Diagnostics::new();
        diags.push(Diagnostic::warning("warn1"));
        assert!(!diags.has_errors());
    }

    #[test]
    fn test_diagnostics_extend() {
        let mut a = Diagnostics::new();
        a.push(Diagnostic::error("e1"));
        let mut b = Diagnostics::new();
        b.push(Diagnostic::error("e2"));
        a.extend(b);
        assert_eq!(a.entries.len(), 2);
    }

    #[test]
    fn test_diagnostics_error_impl() {
        let diags = Diagnostics::new();
        let _err: &dyn std::error::Error = &diags;
    }
}
