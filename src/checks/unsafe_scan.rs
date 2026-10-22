//! Lexical scan of Rust sources for `unsafe` usage.
//!
//! Comments, string/char literals (including raw strings) and lifetimes are
//! skipped, so `"unsafe"` in a string or `// unsafe` in a comment is not
//! counted. Every `unsafe { .. }` block and `unsafe impl` must be preceded by a
//! `// SAFETY:` comment (same line, the comment lines directly above, or the
//! first line inside the block); `unsafe fn` must have a `# Safety` doc section.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Block,
    Fn,
    Impl,
    Trait,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub kind: Kind,
    pub line: u32,
    pub justified: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FileScan {
    pub sites: Vec<Site>,
    pub forbids_unsafe: bool,
}

/// Returns the source with comments and literals blanked out (newlines kept,
/// so line numbers survive) plus the comment text per line.
fn strip(src: &str) -> (Vec<u8>, Vec<String>) {
    let b = src.as_bytes();
    let mut code = b.to_vec();
    let lines = src.lines().count() + 1;
    let mut comments = vec![String::new(); lines + 1];
    let mut line = 0usize;
    let mut i = 0usize;
    let blank = |code: &mut Vec<u8>, from: usize, to: usize| {
        for c in &mut code[from..to] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    while i < b.len() {
        match b[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(b.len(), |p| i + p);
                comments[line].push_str(&src[i..end]);
                blank(&mut code, i, end);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let start = i;
                let mut depth = 0;
                while i < b.len() {
                    if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        i += 2;
                    } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if b[i] == b'\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                }
                comments[line].push_str(&src[start..i.min(b.len())]);
                blank(&mut code, start, i.min(b.len()));
            }
            b'r' if matches!(b.get(i + 1), Some(b'"') | Some(b'#')) && !is_ident(b, i) => {
                // raw string r"..." / r#"..."#
                let mut j = i + 1;
                let mut hashes = 0;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                j += 1;
                let close: Vec<u8> = std::iter::once(b'"')
                    .chain(std::iter::repeat(b'#').take(hashes))
                    .collect();
                let end = find(b, j, &close).map_or(b.len(), |p| p + close.len());
                line += b[i..end].iter().filter(|&&c| c == b'\n').count();
                blank(&mut code, i, end);
                i = end;
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    if i < b.len() && b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(b.len());
                blank(&mut code, start, i);
            }
            b'\'' => {
                // char literal ('a', '\n', '\u{..}') vs lifetime ('a)
                if b.get(i + 1) == Some(&b'\\') {
                    let end = b[i + 2..]
                        .iter()
                        .position(|&c| c == b'\'')
                        .map_or(b.len(), |p| i + 2 + p + 1);
                    blank(&mut code, i, end);
                    i = end;
                } else if b.get(i + 2) == Some(&b'\'') {
                    blank(&mut code, i, i + 3);
                    i += 3;
                } else {
                    i += 1; // lifetime or multi-byte char; harmless either way
                }
            }
            _ => i += 1,
        }
    }
    (code, comments)
}

fn is_ident(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')
}

fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// A `SAFETY:` comment on the same line, in the run of comment/attribute lines
/// directly above, or as the first line inside the block.
fn safety_comment_near(raw: &[&str], comments: &[String], ln: usize) -> bool {
    if comments.get(ln).is_some_and(|c| c.contains("SAFETY:")) {
        return true;
    }
    let mut k = ln;
    while k > 0 {
        k -= 1;
        let t = raw.get(k).map_or("", |s| s.trim());
        if t.starts_with("//") {
            if t.contains("SAFETY:") {
                return true;
            }
        } else if !t.starts_with("#[") {
            break;
        }
    }
    raw.get(ln + 1)
        .is_some_and(|t| t.trim().starts_with("//") && t.contains("SAFETY:"))
}

pub fn scan(src: &str) -> FileScan {
    let (code, comments) = strip(src);
    let text = String::from_utf8_lossy(&code).into_owned();
    let mut out = FileScan {
        forbids_unsafe: text.contains("#![forbid(unsafe_code)]")
            || text.replace(' ', "").contains("#![forbid(unsafe_code)]"),
        ..FileScan::default()
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let raw_lines: Vec<&str> = src.split('\n').collect();
    for (ln, l) in lines.iter().enumerate() {
        let lb = l.as_bytes();
        let mut from = 0;
        while let Some(p) = l[from..].find("unsafe") {
            let at = from + p;
            from = at + 6;
            let before_ok = at == 0 || !(lb[at - 1].is_ascii_alphanumeric() || lb[at - 1] == b'_');
            let after_ok = lb
                .get(at + 6)
                .map_or(true, |c| !(c.is_ascii_alphanumeric() || *c == b'_'));
            if !before_ok || !after_ok {
                continue;
            }
            // next token, possibly on a later line
            let rest: String = std::iter::once(&l[at + 6..])
                .chain(lines[ln + 1..].iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            let next = rest.trim_start();
            let kind = if next.starts_with('{') {
                Kind::Block
            } else if next.starts_with("fn") || next.starts_with("extern") {
                Kind::Fn
            } else if next.starts_with("impl") {
                Kind::Impl
            } else if next.starts_with("trait") {
                Kind::Trait
            } else {
                continue; // e.g. `unsafe_op_in_unsafe_fn` attribute args are filtered above
            };
            let justified = match kind {
                Kind::Block | Kind::Impl => safety_comment_near(&raw_lines, &comments, ln),
                Kind::Fn | Kind::Trait => {
                    // walk up through attributes/doc comments looking for "# Safety"
                    let mut k = ln;
                    let mut found = false;
                    while k > 0 {
                        k -= 1;
                        let t = raw_lines.get(k).map_or("", |s| s.trim());
                        if t.starts_with("///") || t.starts_with("#[") || t.starts_with("//!") {
                            if t.contains("# Safety") {
                                found = true;
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    found
                }
            };
            out.sites.push(Site {
                kind,
                line: ln as u32 + 1,
                justified,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_comments_strings_and_identifiers() {
        let src = r##"
// unsafe { in a comment }
/* unsafe { nested /* unsafe */ } */
fn f() -> &'static str { "unsafe { }" }
const R: &str = r#"unsafe { "raw" }"#;
let not_unsafe_ident = 1;
let c = '{';
#[allow(unsafe_code)]
fn g() {}
"##;
        assert!(scan(src).sites.is_empty(), "{:?}", scan(src));
    }

    #[test]
    fn classifies_and_checks_justification() {
        let src = r#"
fn a(p: *const u8) -> u8 {
    // SAFETY: caller guarantees p is valid.
    unsafe { *p }
}
fn b(p: *const u8) -> u8 {
    unsafe { *p }
}
/// Reads a byte.
///
/// # Safety
/// `p` must be valid.
#[inline]
pub unsafe fn c(p: *const u8) -> u8 { *p }
pub unsafe fn d() {}
// SAFETY: no interior pointers.
unsafe impl Send for X {}
unsafe impl Sync for X {}
"#;
        let s = scan(src);
        let got: Vec<(Kind, u32, bool)> = s
            .sites
            .iter()
            .map(|x| (x.kind, x.line, x.justified))
            .collect();
        assert_eq!(
            got,
            vec![
                (Kind::Block, 4, true),
                (Kind::Block, 7, false),
                (Kind::Fn, 14, true),
                (Kind::Fn, 15, false),
                (Kind::Impl, 17, true),
                (Kind::Impl, 18, false),
            ]
        );
    }

    #[test]
    fn detects_forbid_attribute_and_multiline_block() {
        let s = scan("#![forbid(unsafe_code)]\nfn f() {}\n");
        assert!(s.forbids_unsafe && s.sites.is_empty());
        let s = scan("fn f() {\n    unsafe\n    {\n    }\n}\n");
        assert_eq!(s.sites.len(), 1);
        assert_eq!(s.sites[0].kind, Kind::Block);
        assert!(!s.sites[0].justified);
    }

    #[test]
    fn escaped_quotes_and_char_literals_do_not_confuse_the_lexer() {
        let src = "let a = \"\\\"unsafe {\";\nlet q = '\\'';\nlet b = 1; unsafe { x() }\n";
        let s = scan(src);
        assert_eq!(s.sites.len(), 1);
        assert_eq!(s.sites[0].line, 3);
    }
}
