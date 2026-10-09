//! Message text as plain words, for notifications.
//!
//! Bodies are GitHub-flavored markdown (the UI renders them), but a
//! notification is one plain line: no asterisks, backticks, pipes or link
//! URLs. This is a small line-by-line stripper, not a parser; it only has to
//! produce a readable preview.

/// The first `max` characters of `body` as plain one-line text.
pub fn plain_preview(body: &str, max: usize) -> String {
    let mut out = String::new();
    let mut fence: Option<(char, usize)> = None;
    for raw in body.lines() {
        let line = raw.trim();
        if let Some((c, n)) = fence {
            // Inside a code block: nothing of it is shown; the closing fence ends it.
            if is_fence(line, c, n) {
                fence = None;
            }
            continue;
        }
        if let Some((c, n)) = fence_open(line) {
            fence = Some((c, n));
            out.push_str(" [code] ");
            continue;
        }
        let mut s = line;
        // quotes, then headings, then list and task markers
        while let Some(r) = s.strip_prefix('>') {
            s = r.trim_start();
        }
        let hashes = s.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && s[hashes..].starts_with(' ') {
            s = s[hashes..].trim_start();
        }
        s = strip_list_marker(s);
        if is_rule(s) || is_table_separator(s) {
            continue;
        }
        out.push(' ');
        out.push_str(&inline(s));
        if out.chars().count() > max * 2 {
            break; // plenty for one preview; a huge body is not walked to its end
        }
    }
    let flat: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(max).collect()
}

/// An opening ``` or ~~~ fence: its character and length.
fn fence_open(line: &str) -> Option<(char, usize)> {
    let c = line.chars().next().filter(|&c| c == '`' || c == '~')?;
    let n = line.chars().take_while(|&x| x == c).count();
    // a backtick fence's info string may not hold a backtick (that is inline code)
    (n >= 3 && !(c == '`' && line[n..].contains('`'))).then_some((c, n))
}

fn is_fence(line: &str, c: char, n: usize) -> bool {
    let k = line.chars().take_while(|&x| x == c).count();
    k >= n && line[k..].trim().is_empty()
}

fn strip_list_marker(s: &str) -> &str {
    let mut t = s;
    if let Some(r) = ["- ", "* ", "+ "].iter().find_map(|m| t.strip_prefix(m)) {
        t = r.trim_start();
    } else {
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        if (1..=9).contains(&digits) {
            let r = &t[digits..];
            if let Some(r) = r.strip_prefix(". ").or_else(|| r.strip_prefix(") ")) {
                t = r.trim_start();
            }
        }
    }
    // task list box
    for b in ["[ ] ", "[x] ", "[X] "] {
        if let Some(r) = t.strip_prefix(b) {
            return r.trim_start();
        }
    }
    t
}

/// ---, ***, ___ (three or more, spaces allowed).
fn is_rule(s: &str) -> bool {
    let mut cs = s.chars().filter(|c| !c.is_whitespace());
    match cs.next() {
        Some(c @ ('-' | '*' | '_')) => {
            let rest: Vec<char> = cs.collect();
            rest.len() >= 2 && rest.iter().all(|&x| x == c)
        }
        _ => false,
    }
}

/// | --- | :-: | row under a table header.
fn is_table_separator(s: &str) -> bool {
    s.contains('-') && s.contains('|') && s.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// Inline markup out: emphasis marks, backticks, link and image syntax (the
/// text stays, the address goes), table pipes.
fn inline(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        match c {
            '\\' if i + 1 < cs.len() && cs[i + 1].is_ascii_punctuation() => {
                out.push(cs[i + 1]);
                i += 2;
            }
            '!' | '[' => {
                let open = if c == '!' { i + 1 } else { i };
                if cs.get(open) == Some(&'[') {
                    if let Some((text, next)) = link_at(&cs, open) {
                        out.push_str(&inline(&text));
                        i = next;
                        continue;
                    }
                }
                out.push(c);
                i += 1;
            }
            '`' | '*' | '~' | '|' => {
                // a lone ~ is a tilde ("~5 min"); only ~~ is strikethrough
                if c == '~' && cs.get(i + 1) != Some(&'~') && (i == 0 || cs[i - 1] != '~') {
                    out.push(c);
                } else if c == '|' {
                    out.push(' ');
                }
                i += 1;
            }
            '_' => {
                // emphasis only at a word edge: snake_case keeps its underscores
                let before = i > 0 && cs[i - 1].is_alphanumeric();
                let after = cs.get(i + 1).is_some_and(|x| x.is_alphanumeric());
                if before && after {
                    out.push('_');
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// `[text](url)` starting at `cs[open]`: the text, and the index after it.
fn link_at(cs: &[char], open: usize) -> Option<(String, usize)> {
    let mut depth = 0;
    let mut close = None;
    for (j, &c) in cs.iter().enumerate().skip(open) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(j);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    if cs.get(close + 1) != Some(&'(') {
        return None;
    }
    let mut parens = 0;
    for (j, &c) in cs.iter().enumerate().skip(close + 1) {
        match c {
            '(' => parens += 1,
            ')' => {
                parens -= 1;
                if parens == 0 {
                    return Some((cs[open + 1..close].iter().collect(), j + 1));
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::plain_preview as p;

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(p("Are we still on for the demo on Friday?", 140), "Are we still on for the demo on Friday?");
        assert_eq!(p("a\nb\n\nc", 140), "a b c");
        assert_eq!(p("snake_case_name and C:\\dir\\file_a.txt ~5 min", 140), "snake_case_name and C:\\dir\\file_a.txt ~5 min");
        assert_eq!(p("Meeting at 3. Then 4. Then done", 140), "Meeting at 3. Then 4. Then done");
    }

    #[test]
    fn emphasis_and_code_lose_their_marks() {
        assert_eq!(p("rc3 is ready: **1,286 tests passed**, 0 *failed*.", 140), "rc3 is ready: 1,286 tests passed, 0 failed.");
        assert_eq!(p("this is _italic_ and __bold__ and ~~gone~~", 140), "this is italic and bold and gone");
        assert_eq!(p("quarantined (`net::retry_backoff`)", 140), "quarantined (net::retry_backoff)");
        assert_eq!(p("use `a * b`", 140), "use a b");
    }

    #[test]
    fn links_keep_their_words_not_their_address() {
        assert_eq!(p("See [the docs](https://example.com/a_(b)?x=1) now", 140), "See the docs now");
        assert_eq!(p("![diagram](https://example.com/d.png) above", 140), "diagram above");
        assert_eq!(p("a [broken](link and [x] y", 140), "a [broken](link and [x] y");
        assert_eq!(p("a **[bold link](https://x.org)**", 140), "a bold link");
    }

    #[test]
    fn headings_lists_quotes_and_rules_lose_their_markers() {
        assert_eq!(p("## Review: transfer code\n\nOverall fine.", 140), "Review: transfer code Overall fine.");
        assert_eq!(p("#hashtag stays", 140), "#hashtag stays");
        assert_eq!(p("- one\n  - nested\n* two\n+ three\n1. first\n2) second", 140), "one nested two three first second");
        assert_eq!(p("- [x] done\n- [ ] todo", 140), "done todo");
        assert_eq!(p("> quoted\n> > deeper\nafter", 140), "quoted deeper after");
        assert_eq!(p("above\n\n---\n\nbelow\n***\nend", 140), "above below end");
    }

    #[test]
    fn code_blocks_become_a_marker() {
        assert_eq!(p("Done:\n```diff\n- old()\n+ new()\n```\nSummary attached.", 140), "Done: [code] Summary attached.");
        assert_eq!(p("```\nno closing fence\nstill code", 140), "[code]");
        assert_eq!(p("~~~rust\nfn x() {}\n~~~\nok", 140), "[code] ok");
        // a closing fence needs at least as many marks as the opening one
        assert_eq!(p("````\n```\nstill code\n````\nafter", 140), "[code] after");
        // three backticks with text after them on one line are inline code, not a fence
        assert_eq!(p("```x``` and more", 140), "x and more");
    }

    #[test]
    fn tables_lose_pipes_and_the_dash_row() {
        assert_eq!(p("| Name | Qty |\n|:-----|----:|\n| a | 1 |", 140), "Name Qty a 1");
    }

    #[test]
    fn the_preview_is_cut_to_the_limit_by_characters() {
        assert_eq!(p("äöüäöüäöü", 4), "äöüä");
        let long = "word ".repeat(100000);
        assert_eq!(p(&long, 20).chars().count(), 20);
    }

    #[test]
    fn raw_html_stays_as_typed_text() {
        // the notification shows text, so there is nothing to execute; it just isn't hidden
        assert_eq!(p("<script>alert(1)</script> hi", 140), "<script>alert(1)</script> hi");
    }
}
