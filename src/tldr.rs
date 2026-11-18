//! 本地 tldr-pages（CC BY 4.0，https://github.com/tldr-pages/tldr）。
//!
//! 页面目录与官方归档 `tldr.zip` 解压后的布局一致：`pages/`（英文）、`pages.zh/`（简体中文），
//! 其下分 `common/`、`linux/`、`osx/`。按以下顺序定位，均不存在时完全沿用纯模型路径：
//! `RUASK_TLDR_DIR` → `$XDG_DATA_HOME/ruask/tldr` → `$HOME/.local/share/ruask/tldr`。
//! `ruask --update-pages` 下载并校验官方归档、只解压英文与简体中文页面，并生成
//! `ruask-index.tsv`（页面名与关键词），让参考检索不必逐个读取同族页面。没有索引时退回扫描目录。

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::query::{Language, Platform};

pub const ATTRIBUTION: &str = "tldr-pages · CC BY 4.0";
const INDEX_FILE: &str = "ruask-index.tsv";
const INDEX_HEADER: &str = "#ruask-index v1";
const ARCHIVE_URL: &str = "https://github.com/tldr-pages/tldr/releases/latest/download/tldr.zip";
const SUMS_URL: &str =
    "https://github.com/tldr-pages/tldr/releases/latest/download/tldr.sha256sums";
const PLATFORMS: [&str; 3] = ["linux", "osx", "common"];
const LANG_DIRS: [(&str, &str); 2] = [("en", "pages"), ("zh", "pages.zh")];

pub struct Pages {
    root: PathBuf,
    index: OnceLock<Option<Vec<Entry>>>,
}

struct Entry {
    platform: String,
    name: String,
    keywords: String,
}

/// 查到的页面及其所在平台目录（`linux` / `osx` / `common`）。
pub struct Page {
    pub text: String,
    pub platform: &'static str,
}

impl Page {
    /// 页面只存在于另一个平台时返回该平台的显示名，例如目标为 macOS 却只有 Linux 页。
    pub fn foreign_platform(&self, target: Platform) -> Option<&'static str> {
        match (self.platform, target) {
            ("linux", Platform::Osx) => Some("Linux"),
            ("osx", Platform::Linux) => Some("macOS"),
            _ => None,
        }
    }
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("RUASK_TLDR_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(|d| PathBuf::from(d).join("ruask/tldr"))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/ruask/tldr"))
        })
}

fn platform_dir(platform: Platform) -> &'static str {
    match platform {
        Platform::Linux => "linux",
        Platform::Osx => "osx",
    }
}

fn platform_order(platform: Platform) -> [&'static str; 3] {
    match platform {
        Platform::Linux => ["linux", "common", "osx"],
        Platform::Osx => ["osx", "common", "linux"],
    }
}

fn dirs(language: Language) -> &'static [&'static str] {
    match language {
        Language::Zh => &["pages.zh", "pages"],
        Language::En => &["pages"],
    }
}

impl Pages {
    pub fn locate() -> Option<Self> {
        let candidates = [
            std::env::var_os("RUASK_TLDR_DIR").map(PathBuf::from),
            std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(|d| PathBuf::from(d).join("ruask/tldr")),
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/ruask/tldr")),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|root| root.join("pages").is_dir())
            .map(|root| Self {
                root,
                index: OnceLock::new(),
            })
    }

    fn index(&self) -> Option<&Vec<Entry>> {
        self.index
            .get_or_init(|| read_index(&self.root.join(INDEX_FILE)))
            .as_ref()
    }

    fn path(
        &self,
        name: &str,
        language: Language,
        platform: Platform,
    ) -> Option<(PathBuf, &'static str)> {
        // 平台优先于语言：目标平台的英文页比其他平台的中文页更准确。
        for p in platform_order(platform) {
            for d in dirs(language) {
                let path = self.root.join(d).join(p).join(format!("{name}.md"));
                if path.is_file() {
                    return Some((path, p));
                }
            }
        }
        None
    }

    fn exists(&self, name: &str) -> bool {
        PLATFORMS.iter().any(|p| {
            self.root
                .join("pages")
                .join(p)
                .join(format!("{name}.md"))
                .is_file()
        })
    }

    /// 页面原文；跟随 tldr 的别名页（唯一示例为 `tldr 原命令`）。
    pub fn page(&self, name: &str, language: Language, platform: Platform) -> Option<Page> {
        let (path, found) = self.path(name, language, platform)?;
        let text = fs::read_to_string(path).ok()?;
        let commands: Vec<&str> = commands(&text).collect();
        if let [only] = commands.as_slice()
            && let Some(original) = only.strip_prefix("tldr ")
        {
            let target = original
                .split_whitespace()
                .filter(|w| !w.starts_with('-'))
                .collect::<Vec<_>>()
                .join("-");
            if target != name
                && let Some((path, found)) = self.path(&target, language, platform)
                && let Ok(text) = fs::read_to_string(path)
            {
                return Some(Page {
                    text,
                    platform: found,
                });
            }
        }
        Some(Page {
            text,
            platform: found,
        })
    }

    /// 最长可命中的页面名（最多三个非选项词）及消耗的词数，例如 `git commit --amend` → (`git-commit`, 2)。
    pub fn resolve(&self, words: &[&str]) -> Option<(String, usize)> {
        if words.iter().any(|w| w.chars().any(is_cjk)) {
            return None;
        }
        let plain: Vec<&str> = words
            .iter()
            .copied()
            .take_while(|w| !w.starts_with('-'))
            .collect();
        (1..=plain.len().min(3)).rev().find_map(|n| {
            let name = plain[..n].join("-").to_lowercase();
            self.exists(&name).then_some((name, n))
        })
    }

    /// 首词拼写纠正：唯一的近似页面名（相邻换位，或长度 ≥ 5 时一处增删改）。
    pub fn suggest(&self, word: &str) -> Option<String> {
        let word = word.to_lowercase();
        if word.len() < 3
            || !word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || self.exists(&word)
        {
            return None;
        }
        let mut names: Vec<String> = match self.index() {
            Some(index) => index.iter().map(|e| e.name.clone()).collect(),
            None => PLATFORMS
                .iter()
                .filter_map(|p| fs::read_dir(self.root.join("pages").join(p)).ok())
                .flat_map(|rd| rd.flatten())
                .filter_map(|e| {
                    e.file_name()
                        .to_str()?
                        .strip_suffix(".md")
                        .map(String::from)
                })
                .collect(),
        };
        names.sort();
        names.dedup();
        let mut found = names.into_iter().filter(|name| close(&word, name));
        let first = found.next()?;
        found.next().is_none().then_some(first)
    }

    /// 给模型的参考：命令页 + 同族页面中与问题最相关的两页（各取前 8 条示例），
    /// 末尾列出其余同族页面名，让模型知道有哪些子命令。
    pub fn reference(
        &self,
        name: &str,
        question: Option<&str>,
        language: Language,
        platform: Platform,
    ) -> Option<String> {
        let mut out = String::new();
        append_reference(
            &mut out,
            name,
            &self.page(name, Language::En, platform)?.text,
        );
        let family = self.family(name, language, platform);
        let mut chosen = Vec::new();
        if let Some(question) = question {
            let wanted = query_tokens(question);
            let mut scored: Vec<(usize, &String)> = family
                .iter()
                .map(|(member, keywords)| {
                    let have = tokens(keywords);
                    let words = tokens(&member.replace('-', " "));
                    let score = wanted
                        .iter()
                        .map(|t| {
                            if words.contains(t) {
                                3
                            } else {
                                usize::from(have.contains(t))
                            }
                        })
                        .sum();
                    (score, member)
                })
                .filter(|(score, _)| *score > 0)
                .collect();
            scored.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then(a.1.matches('-').count().cmp(&b.1.matches('-').count()))
                    .then(a.1.cmp(b.1))
            });
            for (_, member) in scored.into_iter().take(2) {
                if let Some(page) = self.page(member, Language::En, platform) {
                    append_reference(&mut out, member, &page.text);
                    chosen.push(member.clone());
                }
            }
        }
        let others: Vec<&str> = family
            .keys()
            .filter(|member| !chosen.contains(member))
            .map(|member| member.as_str())
            .take(50)
            .collect();
        if !others.is_empty() {
            out.push_str(&format!("## other {name} pages: {}\n", others.join(", ")));
        }
        Some(out)
    }

    /// 同族页面（`name-*`，目标平台与 common）及合并后的中英文关键词。
    fn family(
        &self,
        name: &str,
        language: Language,
        platform: Platform,
    ) -> BTreeMap<String, String> {
        let prefix = format!("{name}-");
        let wanted_platforms = [platform_dir(platform), "common"];
        let mut family: BTreeMap<String, String> = BTreeMap::new();
        if let Some(index) = self.index() {
            for e in index.iter().filter(|e| {
                e.name.starts_with(&prefix) && wanted_platforms.contains(&e.platform.as_str())
            }) {
                let entry = family.entry(e.name.clone()).or_default();
                entry.push(' ');
                entry.push_str(&e.keywords);
            }
            return family;
        }
        for p in wanted_platforms {
            for d in dirs(language) {
                let Ok(entries) = fs::read_dir(self.root.join(d).join(p)) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let file = entry.file_name().to_string_lossy().into_owned();
                    let Some(member) = file.strip_suffix(".md").filter(|f| f.starts_with(&prefix))
                    else {
                        continue;
                    };
                    let text = fs::read_to_string(entry.path()).unwrap_or_default();
                    let slot = family.entry(member.to_string()).or_default();
                    slot.push(' ');
                    slot.push_str(&keywords(&text));
                }
            }
        }
        family
    }
}

fn read_index(path: &Path) -> Option<Vec<Entry>> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != INDEX_HEADER {
        return None;
    }
    Some(
        lines
            .filter_map(|line| {
                let mut it = line.splitn(4, '\t');
                let (_lang, platform, name, keywords) =
                    (it.next()?, it.next()?, it.next()?, it.next()?);
                Some(Entry {
                    platform: platform.into(),
                    name: name.into(),
                    keywords: keywords.into(),
                })
            })
            .collect(),
    )
}

/// 页面描述与示例用途，作为检索关键词（不含命令本身，避免匹配到占位符）。
fn keywords(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let rest = line.strip_prefix("> ").or_else(|| line.strip_prefix("- "));
        if let Some(rest) = rest
            && !rest.contains("More information")
            && !rest.contains("更多信息")
        {
            out.push_str(&rest.replace('\t', " "));
            out.push(' ');
        }
    }
    out
}

/// `ruask --version` 中的页面状态：位置、页面数与安装日期（取索引文件的修改时间）。
pub fn status() -> String {
    let Some(pages) = Pages::locate() else {
        return "tldr 页面：未安装（运行 ruask --update-pages 安装）".into();
    };
    let index = pages.root.join(INDEX_FILE);
    let count = fs::read_to_string(&index)
        .ok()
        .map(|text| text.lines().filter(|l| !l.starts_with('#')).count());
    let date = fs::metadata(&index)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| civil_date(d.as_secs() / 86_400));
    match (count, date) {
        (Some(count), Some(date)) => format!(
            "tldr 页面：{}（{count} 页，安装于 {date}）",
            pages.root.display()
        ),
        _ => format!(
            "tldr 页面：{}（没有索引；运行 ruask --update-pages 重新安装）",
            pages.root.display()
        ),
    }
}

/// 自 1970-01-01 起的天数 → `YYYY-MM-DD`（UTC，Howard Hinnant 的公历算法）。
fn civil_date(days: u64) -> String {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// 遍历页面目录生成索引，返回页面数。
pub fn build_index(root: &Path) -> std::io::Result<usize> {
    let mut out = String::from(INDEX_HEADER);
    out.push('\n');
    let mut count = 0;
    for (lang, dir) in LANG_DIRS {
        for platform in PLATFORMS {
            let Ok(entries) = fs::read_dir(root.join(dir).join(platform)) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            files.sort();
            for file in files {
                let Some(name) = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_suffix(".md"))
                else {
                    continue;
                };
                let text = fs::read_to_string(&file)?;
                out.push_str(&format!(
                    "{lang}\t{platform}\t{name}\t{}\n",
                    keywords(&text)
                ));
                count += 1;
            }
        }
    }
    fs::write(root.join(INDEX_FILE), out)?;
    Ok(count)
}

/// `ruask --update-pages [--from 路径]`：取得官方归档、校验、解压英文和简体中文页面并生成索引。
pub fn update(from: Option<&Path>) -> Result<String, Box<dyn Error>> {
    let target = data_dir().ok_or("无法确定页面目录：请设置 HOME 或 RUASK_TLDR_DIR")?;
    let (archive, verified) = match from {
        Some(path) => (
            fs::read(path).map_err(|_| format!("无法读取归档：{}", path.display()))?,
            false,
        ),
        None => {
            let archive = download(ARCHIVE_URL, 64 << 20)?;
            let sums =
                String::from_utf8(download(SUMS_URL, 1 << 20)?).map_err(|_| "校验文件不是文本")?;
            let expected = sums
                .lines()
                .find_map(|l| {
                    let (hash, file) = l.split_once(char::is_whitespace)?;
                    (file.trim().trim_start_matches('*') == "tldr.zip").then(|| hash.to_lowercase())
                })
                .ok_or("校验文件中没有 tldr.zip")?;
            let actual = hex(&Sha256::digest(&archive));
            if actual != expected {
                return Err(format!(
                    "tldr.zip 校验失败：期望 {expected}，实际 {actual}；未修改本地页面"
                )
                .into());
            }
            (archive, true)
        }
    };
    let digest = hex(&Sha256::digest(&archive));
    let parent = target.parent().ok_or("页面目录无效")?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".tldr-staging-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let result =
        extract(&archive, &staging).and_then(|_| build_index(&staging).map_err(Into::into));
    let count = match result {
        Ok(count) => count,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    let old = parent.join(format!(".tldr-old-{}", std::process::id()));
    if target.exists() {
        fs::rename(&target, &old)?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        let _ = fs::rename(&old, &target);
        return Err(error.into());
    }
    let _ = fs::remove_dir_all(&old);
    Ok(format!(
        "已安装 {count} 个 tldr 页面（英文与简体中文）：{}\nSHA-256 {digest}{}\n页面来自 tldr-pages，按 CC BY 4.0 许可使用。",
        target.display(),
        if verified {
            "（已与官方校验文件核对）"
        } else {
            "（本地归档，未联网核对）"
        }
    ))
}

fn download(url: &str, limit: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut response = ureq::get(url)
        .config()
        .timeout_global(Some(Duration::from_secs(300)))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|e| format!("下载失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("下载失败：HTTP {}（{url}）", response.status().as_u16()).into());
    }
    Ok(response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("下载中断：{e}"))?)
}

fn extract(archive: &[u8], staging: &Path) -> Result<(), Box<dyn Error>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive))
        .map_err(|_| "不是有效的 tldr.zip 归档")?;
    let mut pages = 0;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let keep = name == Path::new("LICENSE.md")
            || LANG_DIRS.iter().any(|(_, dir)| {
                PLATFORMS
                    .iter()
                    .any(|p| name.starts_with(Path::new(dir).join(p)))
            }) && name.extension().is_some_and(|e| e == "md");
        if !keep || file.is_dir() {
            continue;
        }
        let path = staging.join(&name);
        fs::create_dir_all(path.parent().unwrap_or(staging))?;
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)?;
        fs::File::create(&path)?.write_all(&bytes)?;
        pages += 1;
    }
    if !staging.join("pages").is_dir() || pages < 10 {
        return Err("归档中没有找到 tldr 页面".into());
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn close(word: &str, name: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (word.chars().collect(), name.chars().collect());
    if a.len() == b.len() {
        let diff: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
        return match diff.as_slice() {
            [i, j] => *j == i + 1 && a[*i] == b[*j] && a[*j] == b[*i],
            [_] => a.len() >= 5,
            _ => false,
        };
    }
    let (short, long) = if a.len() < b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if long.len() != short.len() + 1 || short.len() < 5 {
        return false;
    }
    (0..long.len()).any(|skip| {
        long.iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, c)| c)
            .eq(short.iter())
    })
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF)
}

fn commands(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter_map(|l| {
        let l = l.trim_end();
        (l.len() > 1 && l.starts_with('`') && l.ends_with('`')).then(|| &l[1..l.len() - 1])
    })
}

/// 中文问题常用词 → 英文页面关键词，补足中文页缺失时的匹配（例如 kubectl-logs 没有中文页）。
const TERMS: &[(&str, &str)] = &[
    ("日志", "logs log"),
    ("删除", "delete remove rm prune"),
    ("镜像", "image images"),
    ("容器", "container containers"),
    ("查看", "show list view get display"),
    ("列出", "list"),
    ("停止", "stop"),
    ("启动", "start run"),
    ("重启", "restart"),
    ("端口转发", "port-forward forward"),
    ("端口", "port"),
    ("进程", "process"),
    ("分支", "branch"),
    ("提交", "commit"),
    ("撤销", "undo revert reset restore"),
    ("回滚", "rollback undo"),
    ("合并", "merge"),
    ("变基", "rebase"),
    ("远程", "remote"),
    ("推送", "push"),
    ("拉取", "pull fetch"),
    ("标签", "tag"),
    ("暂存", "stash stage staged"),
    ("差异", "diff"),
    ("历史", "log history"),
    ("权限", "permission permissions"),
    ("压缩", "compress archive"),
    ("解压", "extract decompress"),
    ("下载", "download"),
    ("上传", "upload"),
    ("安装", "install"),
    ("卸载", "uninstall remove"),
    ("更新", "update upgrade"),
    ("升级", "upgrade"),
    ("搜索", "search find"),
    ("替换", "replace substitute"),
    ("网络", "network"),
    ("服务", "service"),
    ("用户", "user"),
    ("密钥", "key keys"),
    ("证书", "certificate"),
    ("环境变量", "environment variable env"),
    ("配置", "config configuration"),
    ("构建", "build"),
    ("测试", "test"),
    ("运行", "run"),
    ("执行", "exec run execute"),
    ("进入", "exec attach shell"),
    ("描述", "describe"),
    ("详情", "describe details inspect"),
    ("悬空", "dangling"),
    ("清理", "prune clean"),
    ("卷", "volume volumes"),
    ("节点", "node nodes"),
    ("命名空间", "namespace"),
    ("部署", "deploy deployment apply"),
    ("扩容", "scale"),
    ("复制", "copy"),
    ("移动", "move"),
    ("重命名", "rename"),
    ("磁盘", "disk"),
    ("内存", "memory"),
    ("监听", "listen"),
    ("连接", "connect connection"),
    ("同步", "sync"),
    ("备份", "backup"),
    ("恢复", "restore recover"),
    ("签名", "sign signature"),
    ("加密", "encrypt"),
    ("解密", "decrypt"),
    ("排序", "sort"),
    ("统计", "count stats"),
    ("修改", "edit modify amend change"),
    ("信息", "message info"),
    ("克隆", "clone"),
    ("初始化", "init initialize"),
    ("状态", "status"),
    ("清除", "clear clean"),
];

fn query_tokens(question: &str) -> Vec<String> {
    let mut text = question.to_string();
    for (zh, en) in TERMS {
        if question.contains(zh) {
            text.push(' ');
            text.push_str(en);
        }
    }
    tokens(&text)
}

/// 简单词集合：ASCII 小写词（≥3 字符，去掉复数 s）与 CJK 双字。
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = text.to_lowercase();
    for w in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
        if w.len() >= 3
            && !matches!(
                w,
                "the"
                    | "and"
                    | "for"
                    | "how"
                    | "with"
                    | "from"
                    | "file"
                    | "path"
                    | "all"
                    | "specific"
            )
        {
            out.push(
                w.strip_suffix('s')
                    .filter(|s| s.len() >= 3)
                    .unwrap_or(w)
                    .to_string(),
            );
        }
    }
    let cjk: Vec<char> = lower.chars().collect();
    for pair in cjk.windows(2) {
        if is_cjk(pair[0]) && is_cjk(pair[1]) {
            out.push(pair.iter().collect());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `{{[-m|--message]}}` → `-m`；`{{[b|build]}}` → `build`（选项取短形式，子命令取完整名称）。
pub fn simplify(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut rest = command;
    while let Some(start) = rest.find("{{[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let Some(end) = after.find("]}}") else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let alternatives: Vec<&str> = after[..end].split('|').collect();
        let pick = if alternatives[0].starts_with('-') {
            alternatives[0]
        } else {
            alternatives
                .iter()
                .copied()
                .max_by_key(|a| a.len())
                .unwrap_or_default()
        };
        out.push_str(pick);
        rest = &after[end + 3..];
    }
    out.push_str(rest);
    out
}

/// 转成 ruask 渲染器的行标记：保留 `#`/`>`/`-`/整行反引号，去掉“更多信息”链接行；
/// 页面只适用于另一个平台时，在简介后补一行提示。
pub fn for_display(page: &Page, target: Platform, language: Language) -> String {
    let mut out = String::with_capacity(page.text.len() + 128);
    let mut noted = page.foreign_platform(target).is_none();
    let note = page.foreign_platform(target).map(|only| {
        let target = match target {
            Platform::Linux => "Linux",
            Platform::Osx => "macOS",
        };
        match language {
            Language::Zh => format!(
                "> 注意：该命令页面仅适用于 {only}，当前目标平台 {target} 上可能没有这个命令。"
            ),
            Language::En => format!(
                "> Note: this page is for {only} only; the command may be unavailable on {target}."
            ),
        }
    });
    for line in page.text.lines() {
        let t = line.trim_end();
        if t.starts_with("> ") && (t.contains("More information") || t.contains("更多信息")) {
            continue;
        }
        if !noted && t.starts_with("- ") {
            out.push_str(note.as_deref().unwrap_or_default());
            out.push('\n');
            noted = true;
        }
        if t.len() > 1 && t.starts_with('`') && t.ends_with('`') {
            out.push('`');
            out.push_str(&simplify(&t[1..t.len() - 1]));
            out.push('`');
        } else {
            out.push_str(t);
        }
        out.push('\n');
    }
    out
}

fn append_reference(out: &mut String, name: &str, text: &str) {
    out.push_str(&format!("## {name}\n"));
    let mut purpose = String::new();
    let mut examples = 0;
    for line in text.lines() {
        if let Some(d) = line.strip_prefix("> ") {
            if !d.contains("More information") && !d.contains("更多信息") {
                out.push_str(&format!("> {d}\n"));
            }
        } else if let Some(p) = line.strip_prefix("- ") {
            purpose = p.trim_end_matches([':', '：']).to_string();
        } else if line.len() > 1 && line.starts_with('`') && line.ends_with('`') && examples < 8 {
            out.push_str(&format!(
                "- {purpose}: `{}`\n",
                simplify(&line[1..line.len() - 1])
            ));
            examples += 1;
        }
    }
}
