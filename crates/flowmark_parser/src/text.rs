//! Byte offset ↔ line conversion.

use crate::model::Span;

pub struct LineIndex {
    starts: Vec<u32>,
    len: u32,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i as u32 + 1);
            }
        }
        LineIndex {
            starts,
            len: text.len() as u32,
        }
    }

    /// 1-based line of a byte offset.
    pub fn line(&self, offset: u32) -> u32 {
        match self.starts.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(i) => i as u32,
        }
    }

    pub fn line_count(&self) -> u32 {
        // A trailing newline does not start a new line.
        if self.starts.len() > 1 && *self.starts.last().unwrap() == self.len {
            self.starts.len() as u32 - 1
        } else {
            self.starts.len() as u32
        }
    }

    pub fn line_start(&self, line: u32) -> u32 {
        self.starts[(line - 1) as usize]
    }

    /// The text of a line, without its line break.
    pub fn line_text<'a>(&self, text: &'a str, line: u32) -> &'a str {
        let start = self.line_start(line) as usize;
        let end = self
            .starts
            .get(line as usize)
            .map_or(text.len(), |&s| s as usize);
        text[start..end].trim_end_matches(['\n', '\r'])
    }

    pub fn span(&self, start: u32, end: u32) -> Span {
        // `end` is exclusive; a span ending right after a line break ends on the line before.
        let end_line = if end > start {
            self.line(end - 1)
        } else {
            self.line(end)
        };
        Span {
            start,
            end,
            start_line: self.line(start),
            end_line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        let text = "a\nbc\n\nd";
        let idx = LineIndex::new(text);
        assert_eq!(idx.line(0), 1);
        assert_eq!(idx.line(2), 2);
        assert_eq!(idx.line(5), 3);
        assert_eq!(idx.line(6), 4);
        assert_eq!(idx.line_count(), 4);
        assert_eq!(idx.line_text(text, 2), "bc");
        assert_eq!(idx.line_text(text, 3), "");
        assert_eq!(LineIndex::new("a\n").line_count(), 1);
    }
}
