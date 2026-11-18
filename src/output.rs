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

// Both protocols deliver text increments here. Explanation and purpose lines are shown as they
// arrive (see `stream_pending`); commands, titles and fenced text wait for the complete line.
pub struct Lines<'a, W> {
    trace: &'a crate::trace::Trace,
    writer: W,
    pending: String,
    leading: String,
    has_text: bool,
    displayed: bool,
    color: bool,
    fence: Option<(char, usize)>,
    control: Control,
    structured: bool,
    previous: Option<Role>,
    blank_lines: String,
    purpose_pending: bool,
    format_issue: bool,
    warning: &'static str,
    colon_description: bool,
    mnemonics: bool,
    waiting: Option<crate::waiting::Waiting>,
    /// 终端宽度（列）；0 表示不折行。只有 stdout 是终端时由调用方设置。
    width: usize,
    /// 终端中给示例用途编号（为“选中即用”做准备）。
    numbered: bool,
    examples: usize,
    /// 正在逐字显示的说明行或用途行。
    open: Option<Open>,
}

/// 已经开始显示、尚未收到换行的一行。
struct Open {
    role: Role,
    style: &'static str,
    /// 已输出的正文字节数（不含行首标记）。
    emitted: usize,
    column: usize,
    continuation: usize,
    /// 折行时暂存的词间空格。
    space: bool,
}

impl<'a, W: Write> Lines<'a, W> {
    pub fn new(writer: W, color: bool, trace: &'a crate::trace::Trace) -> Self {
        Self {
            trace,
            writer,
            pending: String::new(),
            leading: String::new(),
            has_text: false,
            displayed: false,
            color,
            fence: None,
            control: Control::Text,
            structured: false,
            previous: None,
            blank_lines: String::new(),
            purpose_pending: false,
            format_issue: false,
            warning: "⚠ Destructive: double-check before running",
            colon_description: false,
            mnemonics: false,
            waiting: None,
            width: 0,
            numbered: false,
            examples: 0,
            open: None,
        }
    }

    /// stdout 是终端时的显示选项：按宽度软折行说明与用途（命令不折），并给示例编号。
    pub fn terminal(mut self, width: usize) -> Self {
        self.width = if width >= 30 { width } else { 0 };
        self.numbered = true;
        self
    }

    /// 渲染 tldr 英文页的助记符：`[c]reate` 显示为 `create`（终端中 c 带下划线）。
    pub fn mnemonics(mut self) -> Self {
        self.mnemonics = true;
        self
    }

    /// 等待模型时在 stderr 显示的提示；第一次写出内容（或出错释放）时清除。
    pub fn waiting(mut self, waiting: Option<crate::waiting::Waiting>) -> Self {
        self.waiting = waiting;
        self
    }

    /// 在向 stderr 写提醒之前调用，避免与等待提示混在同一行。
    pub fn stop_waiting(&mut self) {
        self.waiting.take();
    }

    /// 本地警告（危险命令标记）使用的语言。
    pub fn language(mut self, language: crate::query::Language) -> Self {
        self.warning = match language {
            crate::query::Language::Zh => "⚠ 不可逆操作，执行前请确认",
            crate::query::Language::En => "⚠ Destructive: double-check before running",
        };
        self
    }

    /// 回答结束后追加一行来源说明（终端中为暗色）。
    pub fn footer(&mut self, text: &str) -> io::Result<()> {
        self.stop_waiting();
        if self.color {
            writeln!(self.writer, "\n  \x1b[2m{text}\x1b[0m")?;
        } else {
            writeln!(self.writer, "\n  {text}")?;
        }
        self.writer.flush()
    }

    pub fn push(&mut self, text: &str) -> io::Result<()> {
        if !text.trim().is_empty() {
            self.trace.once("first_answer_text");
        }
        self.collect_text(text);
        while let Some(end) = self.pending.find('\n') {
            let line = self.pending[..end].to_owned();
            self.pending.drain(..=end);
            self.write_line(&line)?;
        }
        self.stream_pending()
    }

    /// 说明行（`> `）和用途行（`- `，后面不是反引号）不必等整行：攒够 8 个字符（足以判断“注意”行）
    /// 后开始显示，未配对的反引号和行尾不完整的单词暂缓，换行到达时补齐。命令、标题、围栏和
    /// 本地页面仍整行显示。最终字节与整行显示完全一致。
    fn stream_pending(&mut self) -> io::Result<()> {
        if self.open.is_none() {
            if self.fence.is_some() || self.mnemonics {
                return Ok(());
            }
            let trimmed = self.pending.trim_start();
            let role = if trimmed.starts_with("> ") {
                Role::Description
            } else if trimmed.starts_with("- ") && !trimmed[2..].trim_start().starts_with('`') {
                Role::Purpose
            } else {
                return Ok(());
            };
            let content = trimmed[2..].trim_start().to_owned();
            if content.chars().count() < 8 {
                return Ok(());
            }
            if !self.prepare(role, &content)? {
                return Ok(());
            }
            self.begin(role, &content)?;
        }
        let content = {
            let trimmed = self.pending.trim_start();
            trimmed[2..].trim_start().to_owned()
        };
        let Some(open) = &self.open else {
            return Ok(());
        };
        let rest = &content[open.emitted..];
        let safe = safe_prefix(rest, self.width > 0);
        if safe > 0 {
            let segment = rest[..safe].to_owned();
            self.emit(&segment)?;
            if let Some(open) = &mut self.open {
                open.emitted += safe;
            }
            self.writer.flush()?;
        }
        Ok(())
    }

    /// 写出一行之前的共同步骤：分组空行、格式检查、清除等待提示；返回 false 表示整行跳过。
    fn prepare(&mut self, role: Role, line: &str) -> io::Result<bool> {
        let fenced = self.fence.is_some();
        if !fenced && role != Role::Plain {
            self.structured = true;
            self.leading.clear();
        }
        if line.trim().is_empty() && (role != Role::Plain || !self.has_text) {
            return Ok(false);
        }
        match role {
            Role::Purpose => {
                self.format_issue |= self.purpose_pending;
                self.purpose_pending = true;
            }
            Role::Command => self.purpose_pending = false,
            _ => {}
        }
        self.stop_waiting();
        if role == Role::Plain && !self.blank_lines.is_empty() {
            self.writer.write_all(self.blank_lines.as_bytes())?;
        } else if self.structured
            && !(role == Role::Command && self.colon_description)
            && self
                .previous
                .is_some_and(|previous| role.separates_from(previous))
        {
            writeln!(self.writer)?;
        }
        self.blank_lines.clear();
        self.has_text = true;
        // 第一次向 stdout 写出回答内容（说明行可能尚未结束）；first_stdout_line 记录第一行完整写出。
        self.trace.once("first_stdout_text");
        self.writer.write_all(self.leading.as_bytes())?;
        self.leading.clear();
        self.displayed = true;
        Ok(true)
    }

    /// 写出说明、用途或标题行的行首：缩进、示例编号与样式。`basis` 用于判断“注意”行。
    fn begin(&mut self, role: Role, basis: &str) -> io::Result<()> {
        let style = if self.color { role.style(basis) } else { "" };
        let mut column = role.indent();
        let head = if role == Role::Purpose && self.numbered && self.structured {
            self.examples += 1;
            let number = self.examples.to_string();
            column += number.len() + 1;
            let number = if self.color {
                format!("\x1b[2m{number}\x1b[0m")
            } else {
                number
            };
            format!("{:indent$}{number} {style}", "", indent = role.indent())
        } else {
            // 与整行渲染一致：样式在缩进之前。
            format!("{style}{:indent$}", "", indent = role.indent())
        };
        self.writer.write_all(head.as_bytes())?;
        self.open = Some(Open {
            role,
            style,
            emitted: 0,
            column,
            continuation: column,
            space: false,
        });
        Ok(())
    }

    /// 输出一段正文（其中反引号已配对）：行内代码着色、助记符、按宽度折行。
    fn emit(&mut self, segment: &str) -> io::Result<()> {
        let Some(open) = &mut self.open else {
            return Ok(());
        };
        let mut out = String::with_capacity(segment.len() + 16);
        let (color, width, mnemonics) = (self.color, self.width, self.mnemonics);
        let visible = |out: &mut String, text: &str, open: &mut Open| {
            wrap(out, text, width, open);
        };
        let mut rest = segment;
        while !rest.is_empty() {
            let next_code = rest.find('`');
            let next_mnemonic = if mnemonics && open.role == Role::Purpose {
                mnemonic_at(rest)
            } else {
                None
            };
            match (next_code, next_mnemonic) {
                (Some(start), mnemonic)
                    if mnemonic.is_none_or(|(m, _)| start < m)
                        && rest[start + 1..].contains('`') =>
                {
                    let end = start + 1 + rest[start + 1..].find('`').unwrap_or_default();
                    visible(&mut out, &rest[..start], open);
                    if color {
                        out.push_str("\x1b[3;33m");
                    }
                    visible(&mut out, &rest[start + 1..end], open);
                    if color {
                        out.push_str("\x1b[0m");
                        out.push_str(open.style);
                    }
                    rest = &rest[end + 1..];
                }
                (_, Some((start, letter))) => {
                    visible(&mut out, &rest[..start], open);
                    if color {
                        out.push_str("\x1b[4m");
                    }
                    visible(&mut out, letter.encode_utf8(&mut [0; 4]), open);
                    if color {
                        out.push_str("\x1b[24m");
                    }
                    rest = &rest[start + 3..];
                }
                _ => {
                    visible(&mut out, rest, open);
                    rest = "";
                }
            }
        }
        self.writer.write_all(out.as_bytes())
    }

    /// 结束当前逐字行：复位样式、换行，并记录供下一行使用的状态。
    fn end(&mut self, content: &str) -> io::Result<()> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        if !open.style.is_empty() {
            self.writer.write_all(b"\x1b[0m")?;
        }
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        // 模型偶尔用 > 写示例用途：以冒号结尾的说明紧接命令时不插空行，避免拆开用途和命令。
        self.colon_description =
            open.role == Role::Description && content.trim_end().ends_with([':', '：']);
        self.trace.once("first_stdout_line");
        self.previous = Some(open.role);
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
        self.structured = false;
        self.previous = None;
        self.blank_lines.clear();
        self.purpose_pending = false;
        self.format_issue = false;
        self.examples = 0;
        self.open = None;
        true
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        if let Some(open) = &self.open {
            let (_, content) = Role::parse(line, false);
            let content = content.to_owned();
            let rest = content.get(open.emitted..).unwrap_or_default().to_owned();
            self.emit(&rest)?;
            return self.end(&content);
        }
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
        if self.structured && line.trim().is_empty() {
            self.blank_lines.push_str(line);
            self.blank_lines.push('\n');
            return Ok(());
        }
        let (role, line) = Role::parse(line, fenced);
        if !self.prepare(role, line)? {
            return Ok(());
        }
        if matches!(role, Role::Title | Role::Description | Role::Purpose) && !fenced {
            self.begin(role, line)?;
            self.emit(line)?;
            return self.end(line);
        }
        self.colon_description = false;
        let line = if self.structured && role != Role::Plain {
            format!("{:indent$}{line}", "", indent = role.indent())
        } else {
            line.to_owned()
        };
        role.write(&mut self.writer, &line, self.color)?;
        self.writer.flush()?;
        self.trace.once("first_stdout_line");
        if role == Role::Command {
            self.trace.once("first_command_candidate");
            self.trace.mark("command_candidate");
            if destructive(&line) {
                if self.color {
                    writeln!(self.writer, "    \x1b[1;31m{}\x1b[0m", self.warning)?;
                } else {
                    writeln!(self.writer, "    {}", self.warning)?;
                }
                self.writer.flush()?;
            }
        }
        self.previous = Some(role);
        Ok(())
    }

    /// 回答中途失败时调用：结束正在逐字显示的一行（复位样式并换行），让错误信息另起一行。
    pub fn interrupt(&mut self) {
        self.stop_waiting();
        if let Some(open) = self.open.take() {
            let reset = if open.style.is_empty() { "" } else { "\x1b[0m" };
            // 输出端已失效时无法补救，错误本身仍会写到 stderr。
            let _ = writeln!(self.writer, "{reset}");
            let _ = self.writer.flush();
        }
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() || self.open.is_some() {
            let line = self.pending.clone();
            self.write_line(&line)?;
            self.writer.flush()?;
            self.pending.clear();
        }
        if !self.has_text {
            return Err(io::Error::other("模型响应中没有文本回答"));
        }
        if self.previous == Some(Role::Plain) && !self.blank_lines.is_empty() {
            self.writer.write_all(self.blank_lines.as_bytes())?;
            self.writer.flush()?;
            self.blank_lines.clear();
        }
        self.stop_waiting();
        if self.format_issue || self.purpose_pending {
            eprintln!(
                "ruask: 回答格式不完整：部分示例用途缺少独立命令行，已保留原文；未额外请求模型"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Title,
    Description,
    Purpose,
    Command,
    Plain,
}

impl Role {
    /// 说明行使用终端默认前景色；以“注意/警告/Note/Warning”开头的说明行用黄色。
    fn style(self, line: &str) -> &'static str {
        match self {
            Self::Title => "\x1b[1;35m",
            Self::Description if is_note(line) => "\x1b[33m",
            Self::Description => "",
            Self::Purpose => "\x1b[32m",
            Self::Command => "\x1b[36m",
            Self::Plain => "",
        }
    }

    /// 整行写出命令行和普通文本行（说明、用途、标题走 `begin`/`emit`/`end`）。
    fn write(self, writer: &mut impl Write, line: &str, color: bool) -> io::Result<()> {
        let style = if color { self.style(line) } else { "" };
        let mut buffer = String::with_capacity(line.len() + 32);
        buffer.push_str(style);
        if self == Self::Command {
            // 占位符：终端中去掉花括号，改为斜体加下划线（沿用命令颜色）；纯文本保留 {{}}。
            // 多选一（`{{--tcp|--udp}}`）去掉花括号后像管道，保留原文只加样式。
            push_spans(&mut buffer, line, "{{", "}}", |out, body, whole| {
                if color {
                    out.push_str("\x1b[3;4m");
                    out.push_str(if body.contains('|') { whole } else { body });
                    out.push_str("\x1b[23;24m");
                } else {
                    out.push_str(whole);
                }
            });
        } else {
            buffer.push_str(line);
        }
        if !style.is_empty() {
            buffer.push_str("\x1b[0m");
        }
        buffer.push('\n');
        writer.write_all(buffer.as_bytes())
    }

    fn indent(self) -> usize {
        if self == Self::Command { 4 } else { 2 }
    }

    fn separates_from(self, previous: Self) -> bool {
        match self {
            Self::Title => true,
            Self::Description => !matches!(previous, Self::Title | Self::Description),
            Self::Purpose => previous != Self::Purpose,
            Self::Command => matches!(previous, Self::Title | Self::Description),
            Self::Plain => previous == Self::Command,
        }
    }

    fn parse(line: &str, fenced: bool) -> (Self, &str) {
        let trimmed = line.trim();
        if fenced {
            return if trimmed.starts_with('#') {
                (Self::Plain, line)
            } else {
                (Self::Command, line)
            };
        }
        for (prefix, role) in [
            ("# ", Self::Title),
            ("> ", Self::Description),
            ("- ", Self::Purpose),
        ] {
            if trimmed == prefix.trim() {
                return (role, "");
            }
            if let Some(text) = trimmed.strip_prefix(prefix) {
                if role == Self::Purpose
                    && let Some(command) = text
                        .strip_prefix('`')
                        .and_then(|text| text.strip_suffix('`'))
                    && !command.contains('`')
                {
                    return (Self::Command, command);
                }
                return (role, text);
            }
        }
        if let Some(command) = trimmed
            .strip_prefix('`')
            .and_then(|text| text.strip_suffix('`'))
        {
            return (Self::Command, command);
        }
        (Self::Plain, line)
    }
}

/// 把 `opening…closing` 包围的片段交给 `span` 处理（参数：片段内容、含定界符的原文），其余原样写出。
fn push_spans(
    out: &mut String,
    line: &str,
    opening: &str,
    closing: &str,
    mut span: impl FnMut(&mut String, &str, &str),
) {
    let mut rest = line;
    while let Some(start) = rest.find(opening) {
        let body_start = start + opening.len();
        let Some(length) = rest[body_start..].find(closing) else {
            break;
        };
        let end = body_start + length + closing.len();
        out.push_str(&rest[..start]);
        span(
            out,
            &rest[body_start..body_start + length],
            &rest[start..end],
        );
        rest = &rest[end..];
    }
    out.push_str(rest);
}

/// 逐字显示时可以安全输出的字节数：不输出行尾空白（整行解析会去掉它）、未配对反引号之后的内容，
/// 折行时也不输出行尾尚未结束的单词。
fn safe_prefix(rest: &str, wrapping: bool) -> usize {
    let mut end = rest.trim_end().len();
    let mut open_tick = None;
    for (index, character) in rest[..end].char_indices() {
        if character == '`' {
            open_tick = if open_tick.is_some() {
                None
            } else {
                Some(index)
            };
        }
    }
    if let Some(index) = open_tick {
        end = index;
    }
    // 文本恰好停在单词中间时，这个单词可能还没收完，折行需要整词宽度。
    if wrapping && end == rest.len() {
        end = rest
            .char_indices()
            .rev()
            .take_while(|(_, c)| !c.is_whitespace() && !is_wide(*c))
            .last()
            .map_or(end, |(index, _)| index);
    }
    end
}

/// 东亚宽字符（中日韩文字、全角符号等）占两列。
fn is_wide(character: char) -> bool {
    matches!(character as u32,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF | 0x20000..=0x3FFFD)
}

/// 不能出现在行首的标点：折行时留在上一行。
fn closing(character: char) -> bool {
    "，。、；：！？）」』》〉】,.;:!?)".contains(character)
}

/// 按终端宽度写出可见文本，续行缩进到 `open.continuation` 列；`width` 为 0 时不折行。
/// 词间空格先暂存，下一个词放得下才输出，所以折行处没有行尾空格；宽字符后紧跟句末标点时一起换行。
fn wrap(out: &mut String, text: &str, width: usize, open: &mut Open) {
    if width == 0 {
        out.push_str(text);
        return;
    }
    let place = |out: &mut String, piece: &str, cells: usize, open: &mut Open| {
        let space = usize::from(open.space);
        if open.column + space + cells > width && open.column > open.continuation {
            out.push('\n');
            out.push_str(&" ".repeat(open.continuation));
            open.column = open.continuation;
        } else if open.space {
            out.push(' ');
            open.column += 1;
        }
        open.space = false;
        out.push_str(piece);
        open.column += cells;
    };
    let mut word = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == ' ' {
            if !word.is_empty() {
                let cells = word.chars().count();
                place(out, &word, cells, open);
                word.clear();
            }
            if open.column > open.continuation {
                open.space = true;
            }
        } else if is_wide(character) {
            if !word.is_empty() {
                let cells = word.chars().count();
                place(out, &word, cells, open);
                word.clear();
            }
            if closing(character) {
                // 句末标点不放到行首：直接跟在上一个字后面。
                if open.space {
                    out.push(' ');
                    open.column += 1;
                    open.space = false;
                }
                out.push(character);
                open.column += 2;
                continue;
            }
            let follow = characters
                .peek()
                .filter(|next| closing(**next))
                .map_or(0, |next| if is_wide(*next) { 2 } else { 1 });
            let mut piece = String::from(character);
            if follow > 0 {
                piece.push(characters.next().unwrap_or_default());
            }
            place(out, &piece, 2 + follow, open);
        } else {
            word.push(character);
        }
    }
    if !word.is_empty() {
        let cells = word.chars().count();
        place(out, &word, cells, open);
    }
}

/// 下一个 tldr 助记符 `[x]`（如 `[c]reate`、`g[z]ip`）的位置与字母：只认紧邻字母的单个 ASCII 字母或数字，
/// 避免误伤 `[1]` 之类的普通方括号。
fn mnemonic_at(text: &str) -> Option<(usize, char)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    (0..chars.len()).find_map(|i| {
        let (start, open) = chars[i];
        let letter = chars.get(i + 1)?.1;
        let close = chars.get(i + 2)?.1;
        let after = chars.get(i + 3).map(|(_, c)| *c);
        let before = i.checked_sub(1).map(|j| chars[j].1);
        (open == '['
            && close == ']'
            && letter.is_ascii_alphanumeric()
            && (after.is_some_and(char::is_alphabetic) || before.is_some_and(char::is_alphabetic)))
        .then_some((start, letter))
    })
}

fn is_note(line: &str) -> bool {
    let text = line.trim_start();
    [
        "注意：",
        "注意:",
        "警告：",
        "警告:",
        "Note:",
        "Warning:",
        "NOTE:",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
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

/// 常见的不可逆命令；不论模型是否提醒，都在命令下方本地标注。
fn destructive(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let has = |w: &str| words.contains(&w);
    let flags = |short: char, long: &str| {
        words.iter().any(|w| {
            *w == long || (w.starts_with('-') && !w.starts_with("--") && w.contains(short))
        })
    };
    let after = |name: &str| words.iter().position(|w| *w == name);
    if let Some(i) = after("rm") {
        // 递归删除（-r / -R / --recursive，含 -rf 组合）无法撤销。
        let recursive = words[i..].iter().any(|w| {
            *w == "--recursive"
                || (w.starts_with('-')
                    && !w.starts_with("--")
                    && (w.contains('r') || w.contains('R')))
        });
        if recursive {
            return true;
        }
    }
    if has("git") {
        if command.contains("reset --hard") || (has("clean") && flags('f', "--force")) {
            return true;
        }
        if has("push") && (has("--force") || has("-f") || has("--mirror")) {
            return true;
        }
    }
    if words.first().is_some_and(|w| w.starts_with("mkfs"))
        || words.iter().any(|w| w.starts_with("mkfs."))
    {
        return true;
    }
    if has("dd") && words.iter().any(|w| w.starts_with("of=")) {
        return true;
    }
    if has("chmod") && has("777") && flags('R', "--recursive") {
        return true;
    }
    if has("docker") && has("prune") && (has("system") || has("volume")) {
        return true;
    }
    false
}
