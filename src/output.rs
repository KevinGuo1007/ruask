use std::io::{self, Write};

#[derive(Default)]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

impl Color {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => Err("--color 仅支持 auto、always 或 never"),
        }
    }

    pub fn enabled(&self, terminal: bool) -> bool {
        match self {
            Self::Auto => terminal && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

// Both protocols deliver text increments here; only complete lines are displayed.
pub struct Lines<W> {
    writer: W,
    pending: String,
    leading: String,
    has_text: bool,
    displayed: bool,
    color: bool,
    fence: Option<(char, usize)>,
    control: Control,
}

impl<W: Write> Lines<W> {
    pub fn new(writer: W, color: bool) -> Self {
        Self {
            writer,
            pending: String::new(),
            leading: String::new(),
            has_text: false,
            displayed: false,
            color,
            fence: None,
            control: Control::Text,
        }
    }

    pub fn push(&mut self, text: &str) -> io::Result<()> {
        self.collect_text(text);
        while let Some(end) = self.pending.find('\n') {
            let line = self.pending[..end].to_owned();
            self.pending.drain(..=end);
            self.write_line(&line)?;
        }
        Ok(())
    }

    fn collect_text(&mut self, text: &str) {
        for character in text.chars() {
            self.control = match self.control {
                Control::Text => match character {
                    '\x1b' => Control::Escape,
                    '\u{9b}' => Control::Csi,
                    '\u{9d}' => Control::Osc,
                    '\n' | '\t' => {
                        self.pending.push(character);
                        Control::Text
                    }
                    c if c.is_control() => Control::Text,
                    _ => {
                        self.pending.push(character);
                        Control::Text
                    }
                },
                Control::Escape => match character {
                    '[' => Control::Csi,
                    ']' => Control::Osc,
                    _ => Control::Text,
                },
                Control::Csi if ('@'..='~').contains(&character) => Control::Text,
                Control::Csi => Control::Csi,
                Control::Osc if character == '\x07' || character == '\u{9c}' => Control::Text,
                Control::Osc if character == '\x1b' => Control::OscEscape,
                Control::Osc => Control::Osc,
                Control::OscEscape if character == '\\' => Control::Text,
                Control::OscEscape => Control::Osc,
            };
        }
    }

    pub fn reset_before_retry(&mut self) -> bool {
        if self.displayed {
            return false;
        }
        self.pending.clear();
        self.leading.clear();
        self.has_text = false;
        self.fence = None;
        self.control = Control::Text;
        true
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        let fenced = self.fence.is_some();
        let trimmed = line.trim();
        if let Some(marker) = trimmed.chars().next().filter(|c| matches!(c, '`' | '~')) {
            let count = trimmed.chars().take_while(|c| *c == marker).count();
            if count >= 3 {
                if self.fence.is_none() {
                    self.fence = Some((marker, count));
                    return Ok(());
                }
                if self.fence.is_some_and(|(opening, length)| {
                    marker == opening && count >= length && trimmed[count..].trim().is_empty()
                }) {
                    self.fence = None;
                    return Ok(());
                }
            }
        }
        if !self.has_text && line.trim().is_empty() {
            self.leading.push_str(line);
            self.leading.push('\n');
            return Ok(());
        }
        self.has_text = true;
        self.writer.write_all(self.leading.as_bytes())?;
        self.leading.clear();
        if self.color && (fenced || line.starts_with("  ") || line.starts_with('\t')) {
            write!(self.writer, "\x1b[1;36m")?;
            let mut rest = line;
            while let Some(start) = rest.find("{{") {
                let Some(end) = rest[start + 2..].find("}}") else {
                    break;
                };
                let end = start + 2 + end + 2;
                write!(
                    self.writer,
                    "{}\x1b[1;33m{}\x1b[1;36m",
                    &rest[..start],
                    &rest[start..end]
                )?;
                rest = &rest[end..];
            }
            writeln!(self.writer, "{rest}\x1b[0m")?;
        } else {
            writeln!(self.writer, "{line}")?;
        }
        self.writer.flush()?;
        self.displayed = true;
        Ok(())
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let line = self.pending.clone();
            self.write_line(&line)?;
            self.writer.flush()?;
            self.pending.clear();
        }
        if !self.has_text {
            return Err(io::Error::other("模型响应中没有文本回答"));
        }
        Ok(())
    }
}

// Strip model-supplied terminal controls before line buffering, including split CSI/OSC sequences.
#[derive(Clone, Copy)]
enum Control {
    Text,
    Escape,
    Csi,
    Osc,
    OscEscape,
}
