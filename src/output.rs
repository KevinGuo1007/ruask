use std::io::{self, Write};

// Both protocols deliver text increments here; only complete lines are displayed.
pub struct Lines<W> {
    writer: W,
    pending: String,
    has_text: bool,
}

impl<W: Write> Lines<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            pending: String::new(),
            has_text: false,
        }
    }

    pub fn push(&mut self, text: &str) -> io::Result<()> {
        self.pending.push_str(text);
        self.has_text |= !text.trim().is_empty();
        if self.has_text {
            while let Some(end) = self.pending.find('\n') {
                self.writer.write_all(&self.pending.as_bytes()[..=end])?;
                self.writer.flush()?;
                self.pending.drain(..=end);
            }
        }
        Ok(())
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if !self.has_text {
            return Err(io::Error::other("模型响应中没有文本回答"));
        }
        if !self.pending.is_empty() {
            writeln!(self.writer, "{}", self.pending)?;
            self.writer.flush()?;
            self.pending.clear();
        }
        Ok(())
    }
}
