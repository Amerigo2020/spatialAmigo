//! Deterministic template parser for Sr3D-style referring expressions.
//!
//! The parser needs no model and runs in WASM. It recognises sentences of the
//! form `<target> <relation> <anchor(s)> [<relation> <anchor(s)>]...`, e.g.
//!
//! * "the mug closest to the laptop"
//! * "find the chair that is farthest from the door"
//! * "the book between the lamp and the bed"
//! * "the mug on the table and to the left of the laptop from my perspective"
//!
//! Labels are matched against a vocabulary (usually the scene's labels):
//! exact, singular/plural, or the longest vocabulary label at the end of the
//! phrase ("the white mug" -> "mug").

use crate::error::{Error, Result};
use crate::query::{normalize_label, Constraint, ObjectRef, Query};
use crate::relations::{Frame, Relation};

/// Relation phrases, matched longest first.
const PHRASES: &[(&str, Relation)] = &[
    // horizontal proximity
    ("that is the closest to", Relation::Closest),
    ("closest to", Relation::Closest),
    ("nearest to", Relation::Closest),
    ("closest", Relation::Closest),
    ("nearest", Relation::Closest),
    ("next to", Relation::Closest),
    ("close to", Relation::Closest),
    ("closer to", Relation::Closest),
    ("near", Relation::Closest),
    ("beside", Relation::Closest),
    ("by", Relation::Closest),
    ("farthest away from", Relation::Farthest),
    ("furthest away from", Relation::Farthest),
    ("farthest from", Relation::Farthest),
    ("furthest from", Relation::Farthest),
    ("farthest to", Relation::Farthest),
    ("most distant from", Relation::Farthest),
    ("far away from", Relation::Farthest),
    ("far from", Relation::Farthest),
    ("farther from", Relation::Farthest),
    ("farthest", Relation::Farthest),
    ("furthest", Relation::Farthest),
    // between
    ("in between", Relation::Between),
    ("in the middle of", Relation::Between),
    ("between", Relation::Between),
    // vertical proximity
    ("hanging above", Relation::Above),
    ("hanging over", Relation::Above),
    ("higher than", Relation::Above),
    ("above", Relation::Above),
    ("over", Relation::Above),
    ("lower than", Relation::Below),
    ("below", Relation::Below),
    ("beneath", Relation::Below),
    // support
    ("on top of", Relation::On),
    ("supported by", Relation::On),
    ("resting on", Relation::On),
    ("sitting on", Relation::On),
    ("standing on", Relation::On),
    ("lying on", Relation::On),
    ("placed on", Relation::On),
    ("upon", Relation::On),
    ("atop", Relation::On),
    ("onto", Relation::On),
    ("on", Relation::On),
    ("underneath", Relation::Under),
    ("under", Relation::Under),
    ("supporting", Relation::Under),
    ("holding", Relation::Under),
    // directional
    ("to the left side of", Relation::Left),
    ("on the left side of", Relation::Left),
    ("to the left of", Relation::Left),
    ("on the left of", Relation::Left),
    ("at the left of", Relation::Left),
    ("left of", Relation::Left),
    ("to the right side of", Relation::Right),
    ("on the right side of", Relation::Right),
    ("to the right of", Relation::Right),
    ("on the right of", Relation::Right),
    ("at the right of", Relation::Right),
    ("right of", Relation::Right),
    ("in front of", Relation::Front),
    ("at the front of", Relation::Front),
    ("front of", Relation::Front),
    ("behind", Relation::Behind),
    ("in back of", Relation::Behind),
    ("at the back of", Relation::Behind),
    ("back of", Relation::Behind),
];

/// Phrases that select the egocentric frame (removed before parsing).
const EGO_MARKERS: &[&str] = &[
    "from my perspective",
    "from my point of view",
    "from my viewpoint",
    "from my view",
    "from my side",
    "from where i stand",
    "from where i am",
    "from the viewers perspective",
    "from the viewer",
    "from the cameras perspective",
    "from the camera",
    "from the viewpoint",
    "from here",
    "as i see it",
    "as seen by me",
    "as seen from here",
    "in my view",
    "egocentrically",
    "egocentric",
];

/// Phrases that select the allocentric frame (removed before parsing).
const ALLO_MARKERS: &[&str] = &[
    "from its perspective",
    "from its point of view",
    "relative to its front",
    "relative to its orientation",
    "allocentrically",
    "allocentric",
];

/// Words dropped from target and anchor phrases.
const STOP: &[&str] = &[
    "the",
    "a",
    "an",
    "that",
    "which",
    "who",
    "is",
    "are",
    "was",
    "its",
    "it",
    "located",
    "placed",
    "positioned",
    "situated",
    "sitting",
    "standing",
    "lying",
    "resting",
    "found",
    "one",
    "ones",
    "object",
    "thing",
    "to",
    "of",
    "there",
    "please",
    "find",
    "pick",
    "up",
    "grab",
    "choose",
    "select",
    "show",
    "me",
    "get",
    "give",
    "point",
    "bring",
    "where",
    "what",
    "can",
    "could",
    "you",
    "i",
    "want",
    "need",
    "also",
    "both",
    "and",
    ",",
    "thats",
    "whats",
    "hand",
    "take",
    "look",
    "at",
    "for",
    "my",
    "this",
    "those",
    "these",
    "most",
    "very",
    "just",
    "be",
];

/// Parses `text` into a [`Query`]. `vocab` are the known labels (pass the scene
/// labels; an empty slice accepts any label as written).
///
/// ```
/// use spatialamigo::{parse, Relation};
/// let q = parse("Pick the mug that is to the left of the laptop.", &["mug".into(), "laptop".into()]).unwrap();
/// assert_eq!(q.target, "mug");
/// assert_eq!(q.constraints[0].relation, Relation::Left);
/// ```
pub fn parse(text: &str, vocab: &[String]) -> Result<Query> {
    let vocab: Vec<Vec<String>> = vocab
        .iter()
        .map(|v| {
            normalize_label(v)
                .split(' ')
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .collect();
    let mut tokens = tokenize(text);
    let frame = extract_frame(&mut tokens);
    let err = |reason: String| Error::Parse {
        text: text.to_string(),
        reason,
    };

    // Scan for relation phrases, skipping over vocabulary labels so that a
    // label containing a relation word is never split.
    let phrases: Vec<(Vec<&str>, Relation)> = PHRASES
        .iter()
        .map(|(p, r)| (p.split(' ').collect::<Vec<_>>(), *r))
        .collect();
    let mut hits: Vec<(usize, usize, Relation)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if let Some(len) = longest_vocab_match(&tokens[i..], &vocab).filter(|&l| l > 1) {
            i += len;
            continue;
        }
        let best = phrases
            .iter()
            .filter(|(p, _)| tokens.len() - i >= p.len() && p.iter().zip(&tokens[i..]).all(|(a, b)| a == b))
            .max_by_key(|(p, _)| p.len());
        match best {
            Some((p, r)) => {
                hits.push((i, i + p.len(), *r));
                i += p.len();
            }
            None => i += 1,
        }
    }

    // "the mug by the window": "by" only counts as a relation if something follows.
    let target_end = hits.first().map_or(tokens.len(), |h| h.0);
    let target = resolve_label(&tokens[..target_end], &vocab)
        .ok_or_else(|| err("no target object found before the first relation".into()))?;
    let target = target.map_err(err)?;

    let mut constraints = Vec::with_capacity(hits.len());
    for (k, &(_, end, rel)) in hits.iter().enumerate() {
        let seg_end = hits.get(k + 1).map_or(tokens.len(), |h| h.0);
        let seg = &tokens[end..seg_end];
        let anchors: Vec<ObjectRef> = if rel == Relation::Between {
            let parts: Vec<&[String]> = seg
                .split(|t| t == "and" || t == ",")
                .filter(|p| has_content(p))
                .collect();
            if parts.len() != 2 {
                return Err(err(format!(
                    "\"between\" needs two anchors joined by \"and\", found {}",
                    parts.len()
                )));
            }
            parts
                .iter()
                .map(|p| resolve_label(p, &vocab).expect("non-empty").map(ObjectRef::Label))
                .collect::<std::result::Result<_, _>>()
                .map_err(err)?
        } else {
            match resolve_label(seg, &vocab) {
                Some(Ok(l)) => vec![ObjectRef::Label(l)],
                Some(Err(e)) => return Err(err(e)),
                None => return Err(err(format!("relation \"{rel}\" needs an anchor object"))),
            }
        };
        let frame = if rel.is_directional() { frame } else { Frame::Auto };
        constraints.push(Constraint {
            relation: rel,
            anchors,
            frame,
        });
    }
    if constraints.is_empty() {
        return Err(err("no spatial relation found".into()));
    }
    Ok(Query {
        target,
        constraints,
        candidates: None,
    })
}

fn tokenize(text: &str) -> Vec<String> {
    let mut s = String::with_capacity(text.len() + 8);
    for ch in text.to_lowercase().chars() {
        match ch {
            '\'' | '\u{2019}' => {}
            ',' => s.push_str(" , "),
            c if c.is_alphanumeric() => s.push(c),
            '_' | '-' => s.push(' '),
            _ => s.push(' '),
        }
    }
    s.split_whitespace().map(str::to_string).collect()
}

fn find_phrase(tokens: &[String], phrase: &str) -> Option<(usize, usize)> {
    let p: Vec<&str> = phrase.split(' ').collect();
    (0..tokens.len().saturating_sub(p.len() - 1))
        .find(|&i| p.iter().zip(&tokens[i..]).all(|(a, b)| a == b))
        .map(|i| (i, i + p.len()))
}

fn extract_frame(tokens: &mut Vec<String>) -> Frame {
    let mut frame = Frame::Auto;
    for (markers, f) in [
        (EGO_MARKERS, Frame::Egocentric),
        (ALLO_MARKERS, Frame::Allocentric),
    ] {
        for m in markers {
            while let Some((a, b)) = find_phrase(tokens, m) {
                tokens.drain(a..b);
                frame = f;
            }
        }
    }
    // "from the chairs perspective" / "from the chairs point of view" -> allocentric
    for tail in ["perspective", "point of view", "viewpoint"] {
        while let Some((a, b)) = find_phrase(tokens, tail) {
            let start = tokens[..a].iter().rposition(|t| t == "from").unwrap_or(a);
            tokens.drain(start..b);
            frame = Frame::Allocentric;
        }
    }
    frame
}

fn singular(w: &str) -> String {
    if let Some(stem) = w.strip_suffix("ies") {
        format!("{stem}y")
    } else if w.ends_with("sses") || w.ends_with("shes") || w.ends_with("ches") || w.ends_with("xes") {
        w[..w.len() - 2].to_string()
    } else if w.ends_with('s') && !w.ends_with("ss") && w.len() > 2 {
        w[..w.len() - 1].to_string()
    } else {
        w.to_string()
    }
}

fn word_eq(a: &str, b: &str) -> bool {
    a == b || singular(a) == b || a == singular(b) || singular(a) == singular(b)
}

fn longest_vocab_match(tokens: &[String], vocab: &[Vec<String>]) -> Option<usize> {
    vocab
        .iter()
        .filter(|v| v.len() <= tokens.len() && v.iter().zip(tokens).all(|(a, b)| word_eq(b, a)))
        .map(|v| v.len())
        .max()
}

fn has_content(seg: &[String]) -> bool {
    seg.iter().any(|t| !STOP.contains(&t.as_str()))
}

/// `None` if the segment has no content words, `Some(Err)` if it does not
/// match the vocabulary.
fn resolve_label(seg: &[String], vocab: &[Vec<String>]) -> Option<std::result::Result<String, String>> {
    let words: Vec<&str> = seg
        .iter()
        .map(String::as_str)
        .filter(|t| !STOP.contains(t))
        .collect();
    // Vocabulary labels are matched on the raw tokens, because labels may
    // contain stop words ("trash can"). The match that ends last wins (the head
    // noun comes last in English noun phrases), ties go to the longer label.
    let best = vocab
        .iter()
        .flat_map(|v| {
            (0..seg.len())
                .filter(move |&s| {
                    s + v.len() <= seg.len() && v.iter().zip(&seg[s..]).all(|(a, b)| word_eq(b, a))
                })
                .map(move |s| (s + v.len(), v.len(), v))
        })
        .max_by_key(|&(end, len, _)| (end, len));
    if let Some((_, _, v)) = best {
        return Some(Ok(v.join(" ")));
    }
    if words.is_empty() {
        return None;
    }
    if vocab.is_empty() {
        let mut w: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        let last = w.len() - 1;
        w[last] = singular(&w[last]);
        return Some(Ok(w.join(" ")));
    }
    let known: Vec<String> = vocab.iter().map(|v| v.join(" ")).collect();
    Some(Err(format!(
        "unknown object \"{}\" (known: {})",
        words.join(" "),
        known.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(labels: &[&str]) -> Vec<String> {
        labels.iter().map(|s| s.to_string()).collect()
    }

    fn p(text: &str) -> Query {
        parse(
            text,
            &v(&[
                "mug",
                "laptop",
                "table",
                "book",
                "bowl",
                "chair",
                "coffee table",
                "trash can",
                "door",
                "lamp",
            ]),
        )
        .unwrap()
    }

    #[test]
    fn sr3d_style_sentences() {
        let q = p("the mug that is closest to the laptop");
        assert_eq!(q.to_string(), "mug | closest(laptop)");
        assert_eq!(
            p("find the chair that is farthest from the door").to_string(),
            "chair | farthest(door)"
        );
        assert_eq!(
            p("The book between the lamp and the bowl.").to_string(),
            "book | between(lamp, bowl)"
        );
        assert_eq!(p("the mug supported by the table").to_string(), "mug | on(table)");
        assert_eq!(
            p("the table that is supporting the laptop").to_string(),
            "table | under(laptop)"
        );
        assert_eq!(p("the lamp above the table").to_string(), "lamp | above(table)");
        assert_eq!(
            p("the trash can below the table").to_string(),
            "trash can | below(table)"
        );
        assert_eq!(
            p("the chair in front of the table").to_string(),
            "chair | front(table)"
        );
        assert_eq!(
            p("the chair behind the table").to_string(),
            "chair | behind(table)"
        );
        assert_eq!(
            p("pick the mug left of the laptop").to_string(),
            "mug | left(laptop)"
        );
        assert_eq!(
            p("the mug on the right side of the laptop").to_string(),
            "mug | right(laptop)"
        );
    }

    #[test]
    fn multi_word_labels_plurals_and_adjectives() {
        assert_eq!(
            p("the mugs on the coffee table").to_string(),
            "mug | on(coffee table)"
        );
        assert_eq!(
            p("the red mug near the trash can").to_string(),
            "mug | closest(trash can)"
        );
        assert_eq!(p("the books on the tables").to_string(), "book | on(table)");
    }

    #[test]
    fn conjunctions() {
        let q = p("the mug on the table and to the left of the laptop");
        assert_eq!(q.to_string(), "mug | on(table) AND left(laptop)");
        let q = p("the mug on the table closest to the laptop");
        assert_eq!(q.to_string(), "mug | on(table) AND closest(laptop)");
        let q = p("the mug between the laptop and the book, on the table");
        assert_eq!(q.to_string(), "mug | between(laptop, book) AND on(table)");
    }

    #[test]
    fn frames() {
        let q = p("the mug to the left of the chair from my perspective");
        assert_eq!(q.constraints[0].frame, Frame::Egocentric);
        let q = p("from the chair's point of view, the mug on the left of the chair");
        assert_eq!(q.constraints[0].frame, Frame::Allocentric);
        assert_eq!(q.to_string(), "mug | left(chair)[allocentric]");
        assert_eq!(p("the mug left of the chair").constraints[0].frame, Frame::Auto);
    }

    #[test]
    fn errors() {
        let vocab = v(&["mug", "laptop"]);
        assert!(parse("the mug", &vocab).is_err());
        assert!(parse("the mug left of", &vocab).is_err());
        assert!(parse("the mug left of the sofa", &vocab).is_err());
        assert!(parse("the mug between the laptop", &vocab).is_err());
        assert!(parse("left of the laptop", &vocab).is_err());
    }

    #[test]
    fn without_vocabulary() {
        let q = parse("the office chairs closest to the doors", &[]).unwrap();
        assert_eq!(q.to_string(), "office chair | closest(door)");
    }
}
