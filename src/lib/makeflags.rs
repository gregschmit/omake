//! Encode and decode the `MAKEFLAGS` variable in GNU make's canonical form.
//!
//! GNU make does not store the literal characters a user typed; it reconstructs `MAKEFLAGS` from
//! the effective option set. The canonical form is a leading dashless *cluster* of single-letter
//! flags, then the argument-bearing and long-only options as separate ` -x` words in a fixed order,
//! then any command-line variable assignments after a ` -- ` separator.
//!
//! Assigning to `MAKEFLAGS` inside a makefile feeds the value back into that option set (additively
//! — a flag can be turned on but not off), so [`merge_decoded`] and [`encode`] are near-inverse
//! operations: `encode` runs at startup and again after parsing, `merge_decoded` folds a makefile's
//! assignment back into `Opts` before the re-encode.

use crate::makefile::opts::Opts;

/// Encode `opts` as a canonical `MAKEFLAGS` string, followed by the command-line variable
/// `assignments` (each already `name=value`). Option order matches GNU make's switch table:
/// the short cluster, then `-I`, `-j`, `-l`, `-O`, `--debug`, `--trace`, `--no-print-directory`,
/// `--warn-undefined-variables`, then ` -- ` and the assignments.
pub fn encode(opts: &Opts, assignments: &[String]) -> String {
    let mut out = String::new();

    // Leading dashless cluster of single-letter flags, in GNU make's canonical order.
    if opts.always_make {
        out.push('B');
    }
    if opts.debug_all {
        out.push('d');
    }
    if opts.environment_overrides {
        out.push('e');
    }
    if opts.ignore_errors {
        out.push('i');
    }
    if opts.keep_going {
        out.push('k');
    } else if opts.no_keep_going {
        // `-S` occupies the `-k` slot (they are the same flag family).
        out.push('S');
    }
    if opts.just_print {
        out.push('n');
    }
    if opts.question {
        out.push('q');
    }
    if opts.no_builtin_rules {
        out.push('r');
    }
    if opts.no_builtin_variables {
        out.push('R');
    }
    if opts.silent {
        out.push('s');
    }
    if opts.touch {
        out.push('t');
    }
    if opts.print_dir_on {
        out.push('w');
    }

    // Argument-bearing and long-only options, each a separate ` -x` word, deduplicated.
    for dir in dedup(&opts.include_dirs) {
        out.push_str(" -I");
        out.push_str(&dir);
    }
    match opts.jobs {
        Some(0) => out.push_str(" -j"),
        Some(n) => {
            out.push_str(" -j");
            out.push_str(&n.to_string());
        }
        None => {}
    }
    if let Some(l) = &opts.load_average {
        out.push_str(" -l");
        out.push_str(l);
    }
    if let Some(o) = &opts.output_sync {
        out.push_str(" -O");
        out.push_str(o);
    }
    if let Some(d) = &opts.debug {
        out.push_str(" --debug=");
        out.push_str(d);
    }
    if opts.trace {
        out.push_str(" --trace");
    }
    if opts.print_dir_off {
        out.push_str(" --no-print-directory");
    }
    if opts.warn_undefined_variables {
        out.push_str(" --warn-undefined-variables");
    }

    if !assignments.is_empty() {
        out.push_str(" -- ");
        let escaped: Vec<String> = assignments.iter().map(|t| escape(t)).collect();
        out.push_str(&escaped.join(" "));
    }
    out
}

/// Fold a `MAKEFLAGS` string (as assigned inside a makefile, or inherited from a parent make) back
/// into `opts`, additively. Only switches are merged; any ` -- ` variable-assignment tail is left
/// to the caller, which tracks command-line assignments separately.
pub fn merge_decoded(opts: &mut Opts, makeflags: &str) {
    let head = makeflags.split(" -- ").next().unwrap_or(makeflags);
    for (i, tok) in head.split_whitespace().enumerate() {
        if i == 0 && !tok.starts_with('-') {
            // The first dashless word is the short-flag cluster.
            apply_cluster(opts, tok);
        } else if tok.starts_with('-') {
            apply_option(opts, tok);
        }
    }
}

/// Merge only the argument-bearing and long-only options (`-l`, `-O`, `--debug`, `--trace`,
/// `--warn-undefined-variables`, `--no-print-directory`) from a parent make's `MAKEFLAGS`. The
/// short cluster, `-I`, and `-j` are inherited by the caller through the existing dedicated paths,
/// so this fills only the options those paths drop.
pub fn inherit_long_options(opts: &mut Opts, makeflags: &str) {
    let head = makeflags.split(" -- ").next().unwrap_or(makeflags);
    for tok in head.split_whitespace() {
        match tok {
            _ if tok.starts_with("-l")
                || tok.starts_with("-O")
                || tok.starts_with("--debug")
                || tok == "--trace"
                || tok == "--warn-undefined-variables"
                || tok == "--no-print-directory"
                || tok == "--print-directory" =>
            {
                apply_option(opts, tok);
            }
            _ => {}
        }
    }
}

/// Apply a dashless single-letter cluster (e.g. `Bks`) to `opts`. Unknown letters are ignored.
fn apply_cluster(opts: &mut Opts, cluster: &str) {
    for c in cluster.chars() {
        match c {
            'B' => opts.always_make = true,
            'd' => opts.debug_all = true,
            'e' => opts.environment_overrides = true,
            'i' => opts.ignore_errors = true,
            'k' => {
                opts.keep_going = true;
                opts.no_keep_going = false;
            }
            'S' => {
                opts.keep_going = false;
                opts.no_keep_going = true;
            }
            'n' => opts.just_print = true,
            'q' => opts.question = true,
            'r' => opts.no_builtin_rules = true,
            'R' => {
                opts.no_builtin_rules = true;
                opts.no_builtin_variables = true;
            }
            's' => opts.silent = true,
            't' => opts.touch = true,
            'w' => opts.print_dir_on = true,
            _ => {}
        }
    }
}

/// Apply a single dashed option token (e.g. `-Ifoo`, `-Onone`, `--trace`) to `opts`.
fn apply_option(opts: &mut Opts, tok: &str) {
    if let Some(dir) = tok.strip_prefix("-I") {
        if dir == "-" {
            opts.include_dirs.clear();
        } else if !dir.is_empty() {
            opts.include_dirs.push(dir.to_string());
        }
    } else if let Some(n) = tok.strip_prefix("-j") {
        opts.jobs = Some(n.parse().unwrap_or(0));
    } else if let Some(l) = tok.strip_prefix("-l") {
        if !l.is_empty() {
            opts.load_average = Some(l.to_string());
        }
    } else if let Some(o) = tok.strip_prefix("-O") {
        opts.output_sync = Some(if o.is_empty() {
            "target".to_string()
        } else {
            o.to_string()
        });
    } else if let Some(d) = tok.strip_prefix("--debug=") {
        opts.debug = Some(d.to_string());
    } else if tok == "--trace" {
        opts.trace = true;
    } else if tok == "--warn-undefined-variables" {
        opts.warn_undefined_variables = true;
    } else if tok == "--no-print-directory" {
        opts.print_dir_off = true;
        opts.print_dir_on = false;
    } else if tok == "--print-directory" {
        opts.print_dir_on = true;
        opts.print_dir_off = false;
    } else if let Some(cluster) = tok.strip_prefix('-')
        && !cluster.starts_with('-')
    {
        // A single-dash token of short flags, e.g. `-B` or `-ik` (as `MAKEFLAGS += -B` appends).
        apply_cluster(opts, cluster);
    }
}

/// Backslash-escape whitespace (and the backslash itself) so a `MAKEFLAGS` assignment token
/// survives whitespace-splitting when a sub-make re-parses it.
pub fn escape(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    for c in token.chars() {
        if c == '\\' || c.is_whitespace() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Deduplicate a list of `-I` directories, keeping the first occurrence of each in order.
fn dedup(dirs: &[String]) -> Vec<String> {
    let mut seen = Vec::new();
    for d in dirs {
        if !seen.contains(d) {
            seen.push(d.clone());
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(makeflags: &str) -> String {
        let mut opts = Opts::default();
        merge_decoded(&mut opts, makeflags);
        encode(&opts, &[])
    }

    #[test]
    fn short_cluster_is_canonically_ordered() {
        // Regardless of input order, the cluster follows GNU make's B d e i k n q r R s t w order.
        assert_eq!(encoded("tsrnkieB"), "Beiknrst");
        assert_eq!(encoded("nqw"), "nqw");
        assert_eq!(encoded("R"), "rR");
    }

    #[test]
    fn long_options_follow_the_cluster_in_order() {
        assert_eq!(
            encoded("k -Ii -Onone -l2 --trace --no-print-directory --warn-undefined-variables"),
            "k -Ii -l2 -Onone --trace --no-print-directory --warn-undefined-variables"
        );
        assert_eq!(encoded("-Onone --debug=b -k"), "k -Onone --debug=b");
    }

    #[test]
    fn merge_is_additive_across_flavors() {
        // A makefile's `MAKEFLAGS=B` (replace) or `+= -B` (append) both add B to the existing set.
        // `silent` stands in for a command-line `-s`.
        let mut opts = Opts {
            silent: true,
            ..Default::default()
        };
        merge_decoded(&mut opts, "B");
        assert_eq!(encode(&opts, &[]), "Bs");

        let mut opts = Opts {
            silent: true,
            ..Default::default()
        };
        merge_decoded(&mut opts, "s -B");
        assert_eq!(encode(&opts, &[]), "Bs");
    }

    #[test]
    fn include_dirs_are_deduplicated() {
        let mut opts = Opts {
            include_dirs: vec!["localtmp".into()],
            ..Default::default()
        };
        merge_decoded(&mut opts, "B -Ilocaltmp");
        assert_eq!(encode(&opts, &[]), "B -Ilocaltmp");
    }

    #[test]
    fn assignments_follow_a_separator() {
        let opts = Opts::default();
        assert_eq!(
            encode(&opts, &["FOO=bar".into(), "hello:=world".into()]),
            " -- FOO=bar hello:=world"
        );
        // Whitespace in a value is escaped so a sub-make can recover it.
        assert_eq!(encode(&opts, &["FOO=a b".into()]), " -- FOO=a\\ b");
    }
}
