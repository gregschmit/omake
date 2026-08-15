//! Filename globbing (`*`, `?`, `[...]`), matching the semantics of `glob(3)` as GNU make uses it.
//!
//! Used for the `$(wildcard ...)` function, wildcard prerequisites/targets, and `include` operands.
//! Only existing paths are returned, sorted; a pattern that matches nothing yields an empty list.

use std::fs;
use std::path::Path;

/// Characters that make a word a glob pattern rather than a literal filename.
const GLOB_CHARS: [char; 3] = ['*', '?', '['];

/// Whether `s` contains any glob metacharacter (so it needs expansion).
pub fn has_glob_chars(s: &str) -> bool {
    s.contains(GLOB_CHARS)
}

/// Expand a single glob `pattern` against the filesystem, returning the matching paths sorted.
/// A pattern with no metacharacters returns `[pattern]` if that path exists, else `[]` — so a
/// literal name is effectively an existence check (matching GNU `$(wildcard ...)`).
pub fn glob(pattern: &str) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }

    // Walk one `/`-separated component at a time, expanding each against the paths found so far.
    // Seed with the filesystem root for an absolute pattern, else the (empty) current directory.
    let mut bases: Vec<String> = vec![if pattern.starts_with('/') {
        "/".to_string()
    } else {
        String::new()
    }];

    for part in pattern.split('/') {
        // Skip empty parts from a leading, trailing, or doubled `/`.
        if part.is_empty() {
            continue;
        }
        bases = expand_component(bases, part);
        if bases.is_empty() {
            return Vec::new();
        }
    }

    bases.retain(|s| !s.is_empty());

    // A pattern ending in `/` matches only directories and keeps the trailing slash (like glob(3)).
    if pattern.ends_with('/') && pattern != "/" {
        bases.retain(|p| is_dir(p));
        for p in &mut bases {
            p.push('/');
        }
    }

    // Byte-order sort: locale-independent (reproducible), and identical to GNU make under
    // `LC_COLLATE=C`. GNU make sorts via the locale's collation, so mixed-case results may order
    // differently in other locales.
    bases.sort();
    bases.dedup();
    bases
}

/// Expand one path `part` (a literal or a glob) against each base directory.
fn expand_component(bases: Vec<String>, part: &str) -> Vec<String> {
    let mut out = Vec::new();

    if !has_glob_chars(part) {
        // Literal component: extend each base and keep those that exist.
        for base in bases {
            let joined = join(&base, part);
            if exists(&joined) {
                out.push(joined);
            }
        }
        return out;
    }

    for base in bases {
        let dir = if base.is_empty() { "." } else { &base };
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // A leading-dot name is only matched by a pattern that also starts with a dot.
            if name.starts_with('.') && !part.starts_with('.') {
                continue;
            }
            if fnmatch(part, &name) {
                out.push(join(&base, &name));
            }
        }
    }
    out
}

/// Join `base`/`part`, keeping a relative result when `base` is the (empty) current directory.
fn join(base: &str, part: &str) -> String {
    match base {
        "" => part.to_string(),
        "/" => format!("/{part}"),
        _ => format!("{base}/{part}"),
    }
}

fn exists(path: &str) -> bool {
    let path = if path.is_empty() { "." } else { path };
    Path::new(path).symlink_metadata().is_ok()
}

/// Whether `path` is a directory (following symlinks, so a symlink to a directory counts).
fn is_dir(path: &str) -> bool {
    let path = if path.is_empty() { "." } else { path };
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// Match a single path component `name` against a glob `pattern` (`*`, `?`, `[...]`). A `/` never
/// appears here because matching is done component by component.
pub fn fnmatch(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    match_here(&p, &n)
}

fn match_here(p: &[char], n: &[char]) -> bool {
    match p.first() {
        None => n.is_empty(),
        Some('*') => {
            // Match zero characters, or consume one and retry the same `*`.
            match_here(&p[1..], n) || (!n.is_empty() && match_here(p, &n[1..]))
        }
        Some('?') => !n.is_empty() && match_here(&p[1..], &n[1..]),
        Some('[') => match class_len(p) {
            Some(consumed) => {
                !n.is_empty()
                    && class_matches(&p[..consumed], n[0])
                    && match_here(&p[consumed..], &n[1..])
            }
            // A malformed class (`[` with no close) matches a literal `[`.
            None => n.first() == Some(&'[') && match_here(&p[1..], &n[1..]),
        },
        Some(&c) => n.first() == Some(&c) && match_here(&p[1..], &n[1..]),
    }
}

/// Length of a `[...]` character class at the start of `p` (including both brackets), or `None`
/// when unterminated. A `]` immediately after the (optional) negation is a literal member.
fn class_len(p: &[char]) -> Option<usize> {
    debug_assert_eq!(p.first(), Some(&'['));
    let mut i = 1;
    if matches!(p.get(i), Some('!') | Some('^')) {
        i += 1;
    }
    let mut first = true;
    while let Some(&c) = p.get(i) {
        if c == ']' && !first {
            return Some(i + 1);
        }
        first = false;
        i += 1;
    }
    None
}

/// Whether `c` is a member of the class `p`, where `p` is exactly one `[...]` class (including
/// brackets). Handles `!`/`^` negation and `a-z` ranges (a trailing `-` is a literal member).
fn class_matches(p: &[char], c: char) -> bool {
    let mut i = 1;
    let negated = matches!(p.get(i), Some('!') | Some('^'));
    if negated {
        i += 1;
    }
    let end = p.len() - 1; // Index of the closing `]`.
    let mut matched = false;
    while i < end {
        if p.get(i + 1) == Some(&'-') && i + 2 < end {
            matched |= p[i] <= c && c <= p[i + 2];
            i += 3;
        } else {
            matched |= p[i] == c;
            i += 1;
        }
    }
    matched != negated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_literals_and_star() {
        assert!(fnmatch("a.c", "a.c"));
        assert!(!fnmatch("a.c", "b.c"));
        assert!(fnmatch("*.c", "a.c"));
        assert!(fnmatch("*.c", "foo.c"));
        assert!(!fnmatch("*.c", "a.h"));
        assert!(fnmatch("*", "anything"));
        assert!(fnmatch("a*b*c", "axbyc"));
        assert!(!fnmatch("a*b*c", "axby"));
    }

    #[test]
    fn fnmatch_question() {
        assert!(fnmatch("?.c", "a.c"));
        assert!(!fnmatch("?.c", "ab.c"));
        assert!(fnmatch("a?c", "abc"));
    }

    #[test]
    fn fnmatch_classes() {
        assert!(fnmatch("[ab].c", "a.c"));
        assert!(fnmatch("[ab].c", "b.c"));
        assert!(!fnmatch("[ab].c", "c.c"));
        assert!(fnmatch("[a-z].c", "m.c"));
        assert!(!fnmatch("[a-z].c", "M.c"));
        assert!(fnmatch("[!ab].c", "c.c"));
        assert!(!fnmatch("[!ab].c", "a.c"));
        assert!(fnmatch("[^0-9]", "x"));
        // A `]` right after `[` is a literal member.
        assert!(fnmatch("[]x]", "]"));
        assert!(fnmatch("[]x]", "x"));
    }

    #[test]
    fn fnmatch_malformed_class_is_literal() {
        assert!(fnmatch("[abc", "[abc"));
    }

    #[test]
    fn has_glob_chars_detects_metacharacters() {
        assert!(has_glob_chars("*.c"));
        assert!(has_glob_chars("foo?"));
        assert!(has_glob_chars("[ab]"));
        assert!(!has_glob_chars("plain.c"));
        assert!(!has_glob_chars("dir/plain.c"));
    }

    #[test]
    fn glob_walks_the_filesystem() {
        // The library is built to a known target dir; use it as a stable place to glob.
        let base = env!("CARGO_MANIFEST_DIR");
        // A literal path returns itself when it exists, nothing when it does not.
        assert_eq!(
            glob(&format!("{base}/Cargo.toml")),
            vec![format!("{base}/Cargo.toml")]
        );
        assert!(glob(&format!("{base}/does-not-exist.xyz")).is_empty());

        // `*.toml` in the crate root finds Cargo.toml (sorted, existing).
        let tomls = glob(&format!("{base}/*.toml"));
        assert!(tomls.contains(&format!("{base}/Cargo.toml")));

        // A trailing slash keeps only directories (and the slash).
        let dirs = glob(&format!("{base}/*/"));
        assert!(dirs.iter().all(|d| d.ends_with('/')));
        assert!(dirs.contains(&format!("{base}/src/")));
    }
}
