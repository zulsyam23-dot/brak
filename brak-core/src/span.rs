use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SourceLoc {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl SourceLoc {
    pub const fn new(line: usize, column: usize, offset: usize) -> Self {
        Self { line, column, offset }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: SourceLoc,
    pub end: SourceLoc,
}

impl Span {
    pub const fn new(start: SourceLoc, end: SourceLoc) -> Self {
        Self { start, end }
    }
}

pub const DUMMY_SPAN: Span = Span::new(
    SourceLoc::new(0, 0, 0),
    SourceLoc::new(0, 0, 0),
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLine {
    pub number: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMap {
    pub filename: String,
    pub source: String,
    pub lines: Vec<SourceLine>,
}

impl SourceMap {
    pub fn new(filename: impl Into<String>, source: impl Into<String>) -> Self {
        let source: String = source.into();
        let lines: Vec<SourceLine> = source
            .split('\n')
            .enumerate()
            .map(|(i, text)| SourceLine {
                number: i + 1,
                text: text.to_string(),
            })
            .collect();
        Self {
            filename: filename.into(),
            source,
            lines,
        }
    }

    pub fn loc_at(&self, offset: usize) -> Option<SourceLoc> {
        let mut cum = 0usize;
        for (i, line) in self.lines.iter().enumerate() {
            let line_len = line.text.len() + 1;
            if offset < cum + line_len {
                return Some(SourceLoc::new(i + 1, offset - cum + 1, offset));
            }
            cum += line_len;
        }
        None
    }

    pub fn span_at(&self, start: usize, end: usize) -> Option<Span> {
        Some(Span::new(self.loc_at(start)?, self.loc_at(end)?))
    }

    #[cfg(test)]
    pub fn test_source(s: &str) -> Self {
        Self::new("test.brk", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_loc_default() {
        let loc = SourceLoc::default();
        assert_eq!(loc.line, 0);
        assert_eq!(loc.column, 0);
        assert_eq!(loc.offset, 0);
    }

    #[test]
    fn test_source_map_lines() {
        let sm = SourceMap::test_source("hello\nworld\nfoo");
        assert_eq!(sm.lines.len(), 3);
        assert_eq!(sm.lines[0].text, "hello");
        assert_eq!(sm.lines[1].text, "world");
        assert_eq!(sm.lines[2].text, "foo");
    }

    #[test]
    fn test_loc_at() {
        let sm = SourceMap::test_source("abc\ndef");
        let loc = sm.loc_at(0).unwrap();
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 1);

        let loc = sm.loc_at(4).unwrap();
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 1);
    }

    #[test]
    fn test_span_at() {
        let sm = SourceMap::test_source("hello world");
        let span = sm.span_at(0, 5).unwrap();
        assert_eq!(span.start.line, 1);
        assert_eq!(span.start.column, 1);
        assert_eq!(span.end.column, 6);
    }
}
