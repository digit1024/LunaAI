//! `@file:` / `@folder:` reference expansion for user messages.
//!
//! References are resolved within a configured root (default HOME); symlinks are
//! resolved *before* the allow-check, credential paths are denied, and expansion
//! is bounded by count and bytes. Unresolved or disallowed references are left
//! verbatim so the user's text is never silently mangled.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Expansion policy for one message.
pub struct RefPolicy {
    pub root: PathBuf,
    pub max_refs: usize,
    pub max_bytes: usize,
}

impl RefPolicy {
    pub fn new(root: PathBuf, max_refs: usize, max_bytes: usize) -> Self {
        Self { root, max_refs, max_bytes }
    }
}

fn ref_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // No look-behind (unsupported by `regex`); the preceding-character boundary
    // is enforced manually in `expand`. Quoted values carry an optional range in
    // the grammar; bare values are split by `file_value_regex`.
    RE.get_or_init(|| {
        Regex::new(
            r#"(?x)@(?P<kind>file|folder):(?:(?P<qval>`[^`\n]+`|"[^"\n]+"|'[^'\n]+')(?::(?P<start>\d+)(?:-(?P<end>\d+))?)?|(?P<bval>\S+))"#,
        )
        .expect("reference regex")
    })
}

fn file_value_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<path>.+?)(?::(?P<start>\d+)(?:-(?P<end>\d+))?)?$").expect("file value regex")
    })
}

/// Expand every `${@file:…}` / `${@folder:…}` reference in `content`.
pub fn expand(content: &str, policy: &RefPolicy) -> String {
    if !content.contains("@file:") && !content.contains("@folder:") {
        return content.to_string();
    }

    let root = policy.root.canonicalize().unwrap_or_else(|_| policy.root.clone());
    let mut matched = 0usize;
    let mut budget = policy.max_bytes;
    let mut out = String::with_capacity(content.len());
    let mut last = 0usize;

    for caps in ref_regex().captures_iter(content) {
        let whole = caps.get(0).expect("whole match");
        out.push_str(&content[last..whole.start()]);
        last = whole.end();

        // Boundary: `@` must not follow a word character or `/`.
        let preceded_ok = whole.start() == 0
            || content[..whole.start()]
                .chars()
                .next_back()
                .map(|c| !(c.is_alphanumeric() || c == '_' || c == '/'))
                .unwrap_or(true);
        if !preceded_ok {
            out.push_str(whole.as_str());
            continue;
        }

        let kind = caps.name("kind").expect("kind").as_str();

        // Split the value into path + optional line range.
        let (path_raw, start, end) = if let Some(quoted) = caps.name("qval") {
            (
                strip_quotes(quoted.as_str()).to_string(),
                caps.name("start").and_then(|m| m.as_str().parse::<usize>().ok()),
                caps.name("end").and_then(|m| m.as_str().parse::<usize>().ok()),
            )
        } else {
            let bare = caps.name("bval").map(|m| m.as_str()).unwrap_or("");
            match file_value_regex().captures(bare) {
                Some(c) => (
                    c.name("path").map(|m| m.as_str().trim().to_string()).unwrap_or_default(),
                    c.name("start").and_then(|m| m.as_str().parse::<usize>().ok()),
                    c.name("end").and_then(|m| m.as_str().parse::<usize>().ok()),
                ),
                None => (bare.to_string(), None, None),
            }
        };

        if matched >= policy.max_refs {
            out.push_str(whole.as_str());
            continue;
        }

        let expansion = match kind {
            "folder" => expand_folder(&root, &path_raw, &mut budget),
            _ => expand_file(&root, &path_raw, start, end, &mut budget),
        };

        match expansion {
            Some(text) => {
                matched += 1;
                out.push_str(&text);
            }
            None => out.push_str(whole.as_str()),
        }
    }

    out.push_str(&content[last..]);
    out
}

fn strip_quotes(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        let paired = (first == b'`' && last == b'`')
            || (first == b'"' && last == b'"')
            || (first == b'\'' && last == b'\'');
        if paired {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

/// Canonicalize `raw` inside `root`; `None` when missing, outside the root, or
/// on a denied path. Symlink resolution happens via `canonicalize`, so an
/// in-root symlink pointing outside the root is rejected.
fn resolve(root: &Path, raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = Path::new(trimmed);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let canon = joined.canonicalize().ok()?;
    if !canon.starts_with(root) {
        return None;
    }
    if is_denied(&canon) {
        return None;
    }
    Some(canon)
}

fn is_denied(path: &Path) -> bool {
    const DENY_COMPONENTS: &[&str] = &[".ssh", ".gnupg", ".aws"];
    const DENY_NAMES: &[&str] = &[
        ".env", "id_rsa", "id_ed25519", "authorized_keys", "known_hosts", ".netrc", ".pgpass",
        ".npmrc", ".pypirc", "credentials", "credentials.json",
    ];
    const DENY_EXTS: &[&str] = &["pem", "key", "p12", "pfx"];

    for component in path.components() {
        if let std::path::Component::Normal(os) = component {
            if let Some(s) = os.to_str() {
                if DENY_COMPONENTS.contains(&s) {
                    return true;
                }
            }
        }
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if DENY_NAMES.contains(&name) {
            return true;
        }
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if DENY_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
            return true;
        }
    }
    let s = path.to_string_lossy();
    s.contains("/skills/.hub/") || s.contains("/.config/gcloud/")
}

fn expand_file(
    root: &Path,
    path_raw: &str,
    start: Option<usize>,
    end: Option<usize>,
    budget: &mut usize,
) -> Option<String> {
    let canon = resolve(root, path_raw)?;
    if !std::fs::metadata(&canon).ok()?.is_file() {
        return None;
    }

    let display = canon.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let bytes = std::fs::read(&canon).ok()?;

    let is_text = crate::llm::mime::sniff(&bytes)
        .map(|s| s.mime.starts_with("text/"))
        .unwrap_or(false);
    if !is_text {
        return Some(format!(
            "\n```\n[binary file: {} ({} bytes)]\n```\n",
            display,
            bytes.len()
        ));
    }

    let text = String::from_utf8_lossy(&bytes);
    let selected = apply_range(&text, start, end);
    let (slice, truncated) = cap_bytes(selected, budget);

    let mut out = String::new();
    out.push_str("\n```");
    out.push_str(language_for(&canon));
    out.push('\n');
    out.push_str(&slice);
    if truncated {
        out.push_str("\n[... truncated ...]");
    }
    out.push_str("\n```\n");
    Some(out)
}

fn expand_folder(root: &Path, raw: &str, budget: &mut usize) -> Option<String> {
    let canon = resolve(root, raw)?;
    if !std::fs::metadata(&canon).ok()?.is_dir() {
        return None;
    }

    let mut entries: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&canon).ok()?.flatten() {
        if entries.len() >= 200 {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        entries.push(if is_dir { format!("{name}/") } else { name });
    }
    entries.sort();

    let (slice, truncated) = cap_bytes(entries.join("\n"), budget);
    let mut out = format!("\n[folder listing: {}]\n", canon.display());
    out.push_str(&slice);
    if truncated {
        out.push_str("\n[... truncated ...]");
    }
    out.push('\n');
    Some(out)
}

/// Apply an optional 1-based inclusive line range.
fn apply_range(text: &str, start: Option<usize>, end: Option<usize>) -> String {
    if start.is_none() && end.is_none() {
        return text.to_string();
    }
    let first = start.unwrap_or(1).max(1);
    let count = match end {
        Some(e) => e.saturating_sub(first).saturating_add(1),
        None => usize::MAX,
    };
    text.lines().skip(first - 1).take(count).collect::<Vec<_>>().join("\n")
}

/// Consume up to `*budget` bytes from `text`, returning `(slice, truncated)`.
fn cap_bytes(text: String, budget: &mut usize) -> (String, bool) {
    if text.len() <= *budget {
        *budget -= text.len();
        return (text, false);
    }
    let mut end = (*budget).min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    *budget = 0;
    (text[..end].to_string(), true)
}

fn language_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("tsx") => "tsx",
        Some("jsx") => "jsx",
        Some("json") => "json",
        Some("md") => "markdown",
        Some("sh") | Some("bash") => "bash",
        Some("yml") | Some("yaml") => "yaml",
        Some("toml") => "toml",
        Some("html") | Some("htm") => "html",
        Some("css") => "css",
        Some("sql") => "sql",
        Some("go") => "go",
        Some("java") => "java",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("hpp") | Some("cc") => "cpp",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("luna_refs_{}_{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn policy(root: &Path) -> RefPolicy {
        RefPolicy::new(root.to_path_buf(), 16, 100_000)
    }

    #[test]
    fn expands_text_file_with_fence() {
        let root = temp_root();
        std::fs::write(root.join("main.rs"), "fn main() {}").unwrap();
        let out = expand("see @file:main.rs here", &policy(&root));
        assert!(out.contains("```rust"), "{out}");
        assert!(out.contains("fn main() {}"), "{out}");
        assert!(out.starts_with("see \n```rust"), "{out}");
    }

    #[test]
    fn supports_line_range_and_quotes() {
        let root = temp_root();
        std::fs::write(root.join("notes.txt"), "one\ntwo\nthree\n").unwrap();
        let out = expand("read @file:`notes.txt`:2-3", &policy(&root));
        assert!(out.contains("two\nthree"), "{out}");
        assert!(!out.contains("one"), "{out}");
    }

    #[test]
    fn folder_listing_marks_dirs() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        let out = expand("ls @folder:.", &policy(&root));
        assert!(out.contains("src/"), "{out}");
        assert!(out.contains("Cargo.toml"), "{out}");
    }

    #[test]
    fn outside_root_is_left_literal() {
        let root = temp_root();
        let outside = root.parent().unwrap().join("luna_refs_outside.txt");
        std::fs::write(&outside, "secret").unwrap();
        let token = "@file:../luna_refs_outside.txt";
        let out = expand(&format!("x {token} y"), &policy(&root));
        assert!(out.contains(token), "{out}");
        assert!(!out.contains("secret"), "{out}");
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn denied_credentials_left_literal() {
        let root = temp_root();
        std::fs::create_dir_all(root.join(".ssh")).unwrap();
        std::fs::write(root.join(".ssh/id_rsa"), "PRIVATE").unwrap();
        std::fs::write(root.join(".env"), "API_KEY=1").unwrap();
        let out = expand("a @file:.env b @file:.ssh/id_rsa c", &policy(&root));
        assert!(out.contains("@file:.env"), "{out}");
        assert!(out.contains("@file:.ssh/id_rsa"), "{out}");
        assert!(!out.contains("API_KEY"), "{out}");
        assert!(!out.contains("PRIVATE"), "{out}");
    }

    #[test]
    fn respects_max_refs() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "AAAA").unwrap();
        std::fs::write(root.join("b.txt"), "BBBB").unwrap();
        let p = RefPolicy::new(root.clone(), 1, 100_000);
        let out = expand("@file:a.txt @file:b.txt", &p);
        assert!(out.contains("AAAA"), "{out}");
        assert!(out.contains("@file:b.txt"), "{out}");
        assert!(!out.contains("BBBB"), "{out}");
    }

    #[test]
    fn binary_reference_becomes_stub() {
        let root = temp_root();
        std::fs::write(root.join("img.png"), b"\x89PNG\r\n\x1a\n\x00\x00").unwrap();
        let out = expand("look @file:img.png", &policy(&root));
        assert!(out.contains("[binary file: img.png"), "{out}");
    }

    #[test]
    fn no_refs_returns_input() {
        let root = temp_root();
        let input = "just a normal message";
        assert_eq!(expand(input, &policy(&root)), input);
    }
}
