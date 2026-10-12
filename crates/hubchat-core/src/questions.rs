//! Question cards and multi-reply over the hub (docs/question-cards.md): the
//! `meta` a message carries, checked before anything trusts it (the hub only
//! passes it on), and the plain-text lines older clients read instead.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_OPTIONS: usize = 10;
/// Characters in one option's label.
pub const MAX_LABEL: usize = 100;
pub const MAX_REPLIES: usize = 50;
/// Characters in a message id a multi-reply names.
pub const MAX_ID: usize = 200;
/// The most one message's `meta` may hold, serialized (the hub's limit).
pub const META_MAX: usize = 4096;
/// Hubs that carry `meta` list this in /healthz.
pub const FEATURE: &str = "meta";
/// Characters of a quoted message's first line in the plain-text version.
const QUOTE_CHARS: usize = 120;

/// A question card's preset options: pick one, or (multiple) one or more.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuestionMeta {
    pub options: Vec<String>,
    #[serde(default)]
    pub multiple: bool,
}

/// The options one message picked on one card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub question: String,
    pub picked: Vec<String>,
}

/// What a message keeps of its `meta` (the `meta` column), every part
/// checked. Flattened into the message the UI gets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// A multi-reply: every message it answers, oldest first (reply_to is the first).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<QuestionMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<Answer>,
    /// The body's plain-text part (byte range), kept so a long body fetched
    /// later is shown without it too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<(usize, usize)>,
}

impl Meta {
    pub fn is_empty(&self) -> bool {
        self.replies.is_empty() && self.question.is_none() && self.answers.is_empty() && self.fallback.is_none()
    }

    /// The column's value: None when there is nothing to keep.
    pub fn stored(&self) -> Option<String> {
        (!self.is_empty()).then(|| serde_json::to_string(self).unwrap_or_default())
    }

    /// Read the column back; anything unreadable counts as nothing.
    pub fn load(s: Option<&str>) -> Meta {
        s.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
    }
}

/// What the UI asks a message to answer, beside NewOutgoing: the messages it
/// replies to (oldest first) and the options picked on cards among them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplyMeta {
    #[serde(default)]
    pub replies: Vec<String>,
    #[serde(default)]
    pub answers: Vec<Answer>,
}

fn label_ok(l: &str) -> bool {
    let n = l.chars().count();
    (1..=MAX_LABEL).contains(&n) && l.trim() == l && !l.chars().any(char::is_control)
}

fn id_ok(s: &str) -> bool {
    (1..=MAX_ID).contains(&s.chars().count()) && !s.chars().any(char::is_control)
}

/// A card's options, if they keep every rule; else None (a free-response card).
pub fn check_question(v: &Value) -> Option<QuestionMeta> {
    let opts = v.get("options")?.as_array()?;
    if !(2..=MAX_OPTIONS).contains(&opts.len()) {
        return None;
    }
    let options: Vec<String> = opts
        .iter()
        .map(|o| o.as_str().filter(|l| label_ok(l)).map(str::to_owned))
        .collect::<Option<_>>()?;
    let mut seen = std::collections::HashSet::new();
    if !options.iter().all(|o| seen.insert(o.to_lowercase())) {
        return None;
    }
    let multiple = match v.get("multiple") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    Some(QuestionMeta { options, multiple })
}

/// A multi-reply's ids: 2-50 different ones, the first being reply_to; else none.
fn check_replies(v: &Value, reply_to: Option<&str>) -> Vec<String> {
    let Some(a) = v.as_array() else { return vec![] };
    let mut out: Vec<String> = Vec::new();
    for x in a {
        let Some(s) = x.as_str().filter(|s| id_ok(s)) else { return vec![] };
        if !out.iter().any(|o| o == s) {
            out.push(s.to_owned());
        }
    }
    if out.len() < 2 || out.len() > MAX_REPLIES || reply_to != Some(out[0].as_str()) {
        return vec![];
    }
    out
}

/// Picks on cards this message replies to, one entry per card; whether the
/// labels are that card's own is for whoever holds the card to check.
fn check_answers(v: &Value, set: &[String]) -> Vec<Answer> {
    let Some(a) = v.as_array() else { return vec![] };
    let mut out: Vec<Answer> = Vec::new();
    for e in a.iter().take(MAX_REPLIES) {
        let Some(q) = e.get("question").and_then(Value::as_str) else { continue };
        let Some(p) = e.get("picked").and_then(Value::as_array) else { continue };
        if !set.iter().any(|s| s == q) || out.iter().any(|x| x.question == q) {
            continue;
        }
        let mut picked: Vec<String> = Vec::new();
        for l in p {
            match l.as_str().filter(|l| label_ok(l)) {
                Some(l) if !picked.iter().any(|x| x == l) => picked.push(l.to_owned()),
                Some(_) => {}
                None => {
                    picked.clear();
                    break;
                }
            }
        }
        if !picked.is_empty() && picked.len() <= MAX_OPTIONS {
            out.push(Answer { question: q.to_owned(), picked });
        }
    }
    out
}

fn check_fallback(v: &Value) -> Option<(usize, usize)> {
    let a = v.as_array().filter(|a| a.len() == 2)?;
    let (s, e) = (usize::try_from(a[0].as_u64()?).ok()?, usize::try_from(a[1].as_u64()?).ok()?);
    (s <= e).then_some((s, e))
}

/// The body without its plain-text part, if the range fits it: a prefix or
/// a suffix, on character boundaries.
pub fn without(body: &str, (s, e): (usize, usize)) -> Option<String> {
    if e > body.len() || !body.is_char_boundary(s) || !body.is_char_boundary(e) || (s != 0 && e != body.len()) {
        return None;
    }
    Some(format!("{}{}", &body[..s], &body[e..]))
}

/// The text a received body shows: without its plain-text part when `meta`
/// names one that fits, else without the one quote line a reply carries.
pub fn shown(body: &str, reply: bool, meta: &Meta) -> String {
    if let Some(t) = meta.fallback.and_then(|r| without(body, r)) {
        return t;
    }
    if reply {
        crate::engine::strip_quote(body).to_owned()
    } else {
        body.to_owned()
    }
}

/// A received message as Hubchat keeps it: the text it shows and what of its
/// `meta` passed the checks (each part on its own).
pub fn incoming(body: &str, kind: Option<&str>, reply_to: Option<&str>, meta: Option<&Value>) -> (String, Meta) {
    let Some(v) = meta.filter(|v| v.is_object()) else {
        return (shown(body, reply_to.is_some(), &Meta::default()), Meta::default());
    };
    let replies = v.get("replies").map(|r| check_replies(r, reply_to)).unwrap_or_default();
    let set: Vec<String> = if replies.is_empty() { reply_to.map(str::to_owned).into_iter().collect() } else { replies.clone() };
    let m = Meta {
        question: if kind == Some("question") { v.get("question").and_then(check_question) } else { None },
        answers: v.get("answers").map(|a| check_answers(a, &set)).unwrap_or_default(),
        replies,
        fallback: v.get("fallback").and_then(check_fallback),
    };
    (shown(body, reply_to.is_some(), &m), m)
}

/// One message a reply answers, as its sender holds it.
pub struct Quoted<'a> {
    pub id: &'a str,
    /// Its text (the first line is quoted).
    pub body: &'a str,
    /// A question card (kind "question").
    pub card: bool,
}

/// The body and `meta` a reply goes out with (docs/question-cards.md): a
/// `> ` line per message it answers, `Answer: …` after a card it picked
/// options on, `Skipped` after a card it answers with no text and no pick,
/// then the text. A single plain reply keeps the one-line form older Hubchat
/// reads, with no `meta`; `meta` also stays home on a hub without it.
pub fn outgoing(text: &str, quoted: &[Quoted], answers: &[Answer], hub_meta: bool) -> (String, Option<Value>) {
    if quoted.is_empty() {
        return (text.to_owned(), None);
    }
    let mut prefix = String::new();
    let mut extra = false;
    for q in quoted {
        let line: String = q.body.lines().next().unwrap_or("").chars().take(QUOTE_CHARS).collect();
        prefix.push_str(crate::engine::QUOTE_PREFIX);
        prefix.push_str(&line);
        prefix.push('\n');
        if let Some(a) = answers.iter().find(|a| a.question == q.id) {
            prefix.push_str("Answer: ");
            prefix.push_str(&a.picked.join(", "));
            prefix.push('\n');
            extra = true;
        } else if q.card && text.is_empty() {
            prefix.push_str("Skipped\n");
            extra = true;
        }
    }
    let body = format!("{prefix}{text}");
    if (quoted.len() < 2 && !extra) || !hub_meta {
        return (body, None);
    }
    let mut m = serde_json::Map::new();
    if quoted.len() > 1 {
        m.insert("replies".into(), json!(quoted.iter().map(|q| q.id).collect::<Vec<_>>()));
    }
    if !answers.is_empty() {
        m.insert("answers".into(), json!(answers));
    }
    m.insert("fallback".into(), json!([0, prefix.len()]));
    (body, Some(Value::Object(m)))
}

/// Whether a reply's `meta` fits the hub's limit (checked before it is
/// queued, so the composer can say so): the largest it can become.
pub fn fits(r: &ReplyMeta) -> bool {
    let v = json!({ "replies": r.replies, "answers": r.answers, "fallback": [0, u32::MAX] });
    serde_json::to_vec(&v).map(|b| b.len() <= META_MAX).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_keeps_only_options_that_follow_every_rule() {
        let ok = json!({"options": ["Beta", "Stable"], "multiple": true});
        assert_eq!(check_question(&ok), Some(QuestionMeta { options: vec!["Beta".into(), "Stable".into()], multiple: true }));
        assert!(!check_question(&json!({"options": ["Beta", "Stable"]})).unwrap().multiple);
        for bad in [
            json!({"options": ["only one"]}),
            json!({"options": ["a", "A"]}),
            json!({"options": ["a", " b"]}),
            json!({"options": ["a", "b\nc"]}),
            json!({"options": ["a", ""]}),
            json!({"options": ["a", 2]}),
            json!({"options": ["a", "b"], "multiple": "yes"}),
            json!({"options": (0..11).map(|i| i.to_string()).collect::<Vec<_>>()}),
            json!({"options": ["a", "x".repeat(101)]}),
        ] {
            assert_eq!(check_question(&bad), None, "{bad}");
        }
    }

    #[test]
    fn untrusted_meta_is_checked_part_by_part() {
        let meta = json!({
            "question": {"options": ["Yes", "No"]},
            "replies": ["a", "b", "a"],
            "answers": [{"question": "b", "picked": ["Yes", "Yes"]}, {"question": "zz", "picked": ["Yes"]}, {"question": "a", "picked": [3]}],
            "fallback": [0, 4],
        });
        // a question meta on something that isn't a question is ignored
        let (body, m) = incoming("> x\nhi", None, Some("a"), Some(&meta));
        assert_eq!(body, "hi");
        assert_eq!(m.question, None);
        assert_eq!(m.replies, vec!["a".to_string(), "b".into()]);
        assert_eq!(m.answers, vec![Answer { question: "b".into(), picked: vec!["Yes".into()] }]);
        // replies whose first isn't reply_to don't count; answers then only for reply_to
        let (_, m) = incoming("> x\nhi", None, Some("b"), Some(&meta));
        assert!(m.replies.is_empty());
        assert_eq!(m.answers, vec![Answer { question: "b".into(), picked: vec!["Yes".into()] }]);
        // not an object: the old one-line quote rule
        assert_eq!(incoming("> x\nhi", None, Some("a"), Some(&json!([1]))).0, "hi");
    }

    #[test]
    fn the_plain_text_part_goes_only_when_it_fits() {
        let body = "Which one?\n\nPick one:\n1. A\n2. B";
        let at = body.find("\n\nPick").unwrap();
        let (text, m) = incoming(body, Some("question"), None, Some(&json!({"question": {"options": ["A", "B"]}, "fallback": [at, body.len()]})));
        assert_eq!(text, "Which one?");
        assert_eq!(m.question.unwrap().options.len(), 2);
        // a range in the middle, past the end or inside a character is ignored
        assert_eq!(without("abcdef", (1, 3)), None);
        assert_eq!(without("abc", (0, 9)), None);
        assert_eq!(without("é", (0, 1)), None);
        // a long body's preview keeps a suffix range for when the whole body is here
        let (preview, m) = incoming("Which one?", Some("question"), None, Some(&json!({"fallback": [10, 40]})));
        assert_eq!(preview, "Which one?");
        assert_eq!(shown(body, false, &Meta { fallback: Some((at, body.len())), ..m }), "Which one?");
    }

    #[test]
    fn a_reply_writes_what_older_clients_read() {
        let q = |id, body, card| Quoted { id, body, card };
        // one plain reply: the old form, no meta
        assert_eq!(outgoing("ok", &[q("a", "first\nsecond", false)], &[], true), ("> first\nok".to_string(), None));
        // answers, a skip and a multi-reply in one message
        let picks = vec![Answer { question: "c1".into(), picked: vec!["Beta".into()] }];
        let (body, meta) = outgoing("", &[q("c1", "Which channel?", true), q("c2", "Anything else?", true), q("m", "hello", false)], &picks, true);
        assert_eq!(body, "> Which channel?\nAnswer: Beta\n> Anything else?\nSkipped\n> hello\n");
        let meta = meta.unwrap();
        assert_eq!(meta["replies"], json!(["c1", "c2", "m"]));
        assert_eq!(meta["answers"], json!([{"question": "c1", "picked": ["Beta"]}]));
        assert_eq!(meta["fallback"], json!([0, body.len()]));
        // what the receiver makes of it: no text, the same replies and picks
        let (text, m) = incoming(&body, None, Some("c1"), Some(&meta));
        assert_eq!(text, "");
        assert_eq!(m.replies.len(), 3);
        assert_eq!(m.answers, picks);
        // a single skip carries meta, so "Skipped" isn't read as the answer's text
        let (body, meta) = outgoing("", &[q("c2", "Anything else?", true)], &[], true);
        assert_eq!(incoming(&body, None, Some("c2"), meta.as_ref()).0, "");
        // a hub without meta gets the plain text only
        assert_eq!(outgoing("", &[q("c2", "Anything else?", true)], &[], false), ("> Anything else?\nSkipped\n".into(), None));
        // a skip line follows only an empty reply to a card
        assert_eq!(outgoing("no", &[q("c2", "Anything else?", true)], &[], true).0, "> Anything else?\nno");
    }

    #[test]
    fn a_reply_meta_must_fit_the_hub() {
        assert!(fits(&ReplyMeta { replies: vec!["a".repeat(32); 50], answers: vec![] }));
        let big = Answer { question: "q".repeat(32), picked: (0..10).map(|i| format!("{i}{}", "x".repeat(99))).collect() };
        assert!(!fits(&ReplyMeta { replies: vec![], answers: vec![big; 5] }));
    }
}
