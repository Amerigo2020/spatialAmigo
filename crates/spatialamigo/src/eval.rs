//! Query evaluation: score every candidate of the target class and explain why.

use std::fmt::Write as _;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::geometry::Obb;
use crate::parser;
use crate::query::{normalize_label, Constraint, ObjectRef, Query};
use crate::relations::{
    self, allocentric_frame, egocentric_frame, DirectionalFrame, Family, Feature, Frame, Params,
    PredicateValue, Relation,
};
use crate::scene::Scene;

/// Best scores below this value are reported as a weak match in the explanation.
pub const LOW_CONFIDENCE: f64 = 0.5;
/// Number of candidates listed in the human readable explanation (the
/// structured trace always contains all of them).
pub const EXPLAIN_TOP_K: usize = 10;
/// Margins below this value are reported as ambiguous in the explanation.
pub const AMBIGUOUS_MARGIN: f64 = 0.1;

/// Evaluation trace of one constraint for one candidate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConstraintTrace {
    /// Relation that was evaluated.
    pub relation: Relation,
    /// Relation family (directional relations report the resolved frame).
    pub family: Family,
    /// Resolved frame for directional relations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
    /// Anchor object ids that gave the best score.
    pub anchors: Vec<String>,
    /// Score of this constraint in `[0, 1]`.
    pub score: f64,
    /// Intermediate quantities.
    pub features: Vec<Feature>,
    /// Remarks, e.g. why a relation was not applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A scored candidate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    /// Object id.
    pub id: String,
    /// Object label.
    pub label: String,
    /// 1-based rank.
    pub rank: usize,
    /// Product of all constraint scores.
    pub score: f64,
    /// One trace per constraint, in query order.
    pub trace: Vec<ConstraintTrace>,
}

/// Result of resolving a query.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resolution {
    /// The query that was evaluated (parsed form if it came from text).
    pub query: Query,
    /// Candidates sorted by descending score (ties by id).
    pub candidates: Vec<Candidate>,
    /// Id of the top-ranked candidate, if any scored above zero.
    pub best: Option<String>,
    /// Score difference between rank 1 and rank 2 (rank 1 score if alone).
    pub margin: f64,
    /// Human readable explanation of the decision.
    pub explanation: String,
}

impl Resolution {
    /// Pretty JSON of the full trace.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("resolution serialization cannot fail")
    }
}

/// Options for [`Resolver::resolve_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveOptions {
    /// Build the human readable explanation (default `true`).
    pub explanation: bool,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        ResolveOptions { explanation: true }
    }
}

/// Resolves queries against one scene. Construction precomputes all boxes, so
/// a resolver should be reused for many queries on the same scene.
///
/// ```
/// use spatialamigo::{Resolver, Scene};
/// let scene = Scene::from_json(include_str!("../../../examples/scenes/tabletop.json")).unwrap();
/// let r = Resolver::new(scene);
/// let res = r.resolve_text("the mug left of the laptop").unwrap();
/// assert_eq!(res.best.as_deref(), Some("mug_2"));
/// ```
#[derive(Debug, Clone)]
pub struct Resolver {
    scene: Scene,
    boxes: Vec<Obb>,
    labels: Vec<String>,
    params: Params,
}

impl Resolver {
    /// Creates a resolver with default parameters.
    pub fn new(scene: Scene) -> Self {
        Resolver::with_params(scene, Params::default())
    }

    /// Creates a resolver with custom parameters.
    pub fn with_params(scene: Scene, params: Params) -> Self {
        let boxes = scene.objects.iter().map(Obb::from_object).collect();
        let labels = scene.objects.iter().map(|o| normalize_label(&o.label)).collect();
        Resolver {
            scene,
            boxes,
            labels,
            params,
        }
    }

    /// The scene.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Parameters in use.
    pub fn params(&self) -> &Params {
        &self.params
    }

    /// Parses `text` with the template parser (using the scene labels as
    /// vocabulary) and resolves it.
    pub fn resolve_text(&self, text: &str) -> Result<Resolution> {
        let q = parser::parse(text, &self.scene.labels())?;
        self.resolve(&q)
    }

    fn lookup(&self, r: &ObjectRef) -> Vec<usize> {
        match r {
            ObjectRef::Id(id) => self
                .scene
                .objects
                .iter()
                .position(|o| &o.id == id)
                .into_iter()
                .collect(),
            ObjectRef::Label(l) => {
                let l = normalize_label(l);
                (0..self.labels.len()).filter(|&i| self.labels[i] == l).collect()
            }
        }
    }

    /// Evaluates a structured query (with human readable explanation).
    pub fn resolve(&self, q: &Query) -> Result<Resolution> {
        self.resolve_with(q, &ResolveOptions::default())
    }

    /// Evaluates a structured query with options. Skipping the text
    /// explanation saves roughly 1.5 µs per listed candidate; the structured
    /// trace is always complete.
    pub fn resolve_with(&self, q: &Query, opts: &ResolveOptions) -> Result<Resolution> {
        q.validate()?;
        let target = normalize_label(&q.target);
        let mut cands: Vec<usize> = (0..self.labels.len())
            .filter(|&i| self.labels[i] == target)
            .collect();
        if let Some(ids) = &q.candidates {
            cands.retain(|&i| ids.iter().any(|id| *id == self.scene.objects[i].id));
            for id in ids {
                if self.scene.get(id).is_none() {
                    return Err(Error::UnknownReference(format!("candidate id \"{id}\"")));
                }
            }
            if cands.is_empty() {
                // Explicit candidate ids win over the label (Sr3D adapter use case).
                cands = ids
                    .iter()
                    .filter_map(|id| self.scene.objects.iter().position(|o| &o.id == id))
                    .collect();
            }
        }
        if cands.is_empty() {
            return Err(Error::UnknownReference(format!(
                "no object with label \"{}\" (scene labels: {})",
                q.target,
                self.scene.labels().join(", ")
            )));
        }

        let mut traces: Vec<Vec<ConstraintTrace>> =
            vec![Vec::with_capacity(q.constraints.len()); cands.len()];
        for c in &q.constraints {
            let per_cand = self.eval_constraint(c, &cands)?;
            for (t, ct) in traces.iter_mut().zip(per_cand) {
                t.push(ct);
            }
        }

        let mut out: Vec<Candidate> = cands
            .iter()
            .zip(traces)
            .map(|(&i, trace)| {
                let o = &self.scene.objects[i];
                Candidate {
                    id: o.id.clone(),
                    label: o.label.clone(),
                    rank: 0,
                    score: trace.iter().map(|t| t.score).product(),
                    trace,
                }
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        for (k, c) in out.iter_mut().enumerate() {
            c.rank = k + 1;
        }
        let best = out.first().filter(|c| c.score > 0.0).map(|c| c.id.clone());
        let margin = match out.as_slice() {
            [a, b, ..] => a.score - b.score,
            [a] => a.score,
            [] => 0.0,
        };
        let explanation = if opts.explanation {
            explain(q, &out, margin)
        } else {
            String::new()
        };
        Ok(Resolution {
            query: q.clone(),
            candidates: out,
            best,
            margin,
            explanation,
        })
    }

    fn anchor_sets(&self, c: &Constraint) -> Result<Vec<Vec<usize>>> {
        c.anchors
            .iter()
            .map(|r| {
                let v = self.lookup(r);
                if v.is_empty() {
                    Err(Error::UnknownReference(format!(
                        "anchor \"{r}\" not found (scene labels: {})",
                        self.scene.labels().join(", ")
                    )))
                } else {
                    Ok(v)
                }
            })
            .collect()
    }

    fn ids(&self, idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| self.scene.objects[i].id.clone()).collect()
    }

    fn eval_constraint(&self, c: &Constraint, cands: &[usize]) -> Result<Vec<ConstraintTrace>> {
        let sets = self.anchor_sets(c)?;
        let p = &self.params;
        let rel = c.relation;
        let family = rel.family(Frame::Auto);

        if rel.is_superlative() {
            // Per anchor: distances of all candidates, soft min/max over candidates.
            // Per candidate: best anchor.
            let mut best: Vec<Option<(f64, usize, PredicateValue)>> = vec![None; cands.len()];
            for &a in &sets[0] {
                let mut dist = Vec::with_capacity(cands.len());
                let mut vals = Vec::with_capacity(cands.len());
                let mut keep = Vec::with_capacity(cands.len());
                for (k, &t) in cands.iter().enumerate() {
                    if t == a {
                        continue;
                    }
                    let v = relations::horizontal_distance(&self.boxes[t], &self.boxes[a], p);
                    dist.push(v.feature("distance_m").unwrap_or(f64::INFINITY));
                    vals.push(v);
                    keep.push(k);
                }
                let scores = relations::superlative_scores(&dist, rel == Relation::Farthest, p);
                for ((k, mut v), s) in keep.into_iter().zip(vals).zip(scores) {
                    v.score = s;
                    v.features.push(Feature {
                        name: "candidates",
                        value: dist.len() as f64,
                    });
                    if best[k].as_ref().is_none_or(|b| s > b.0) {
                        best[k] = Some((s, a, v));
                    }
                }
            }
            return Ok(best
                .into_iter()
                .map(|b| match b {
                    Some((s, a, v)) => ConstraintTrace {
                        relation: rel,
                        family,
                        frame: None,
                        anchors: self.ids(&[a]),
                        score: s,
                        features: v.features,
                        note: None,
                    },
                    None => not_applicable(rel, family, None, "the only anchor is the candidate itself"),
                })
                .collect());
        }

        // Best anchor per candidate. Traces (with id strings) are only built
        // for the winner, which keeps the hot loop allocation-light.
        struct Best {
            value: PredicateValue,
            anchors: [usize; 2],
            n_anchors: usize,
            family: Family,
            frame: Option<Frame>,
            note: Option<String>,
        }
        let mut out = Vec::with_capacity(cands.len());
        for &t in cands {
            let mut best: Option<Best> = None;
            let mut consider = |b: Best| {
                if best.as_ref().is_none_or(|cur| b.value.score > cur.value.score) {
                    best = Some(b);
                }
            };
            let tb = &self.boxes[t];
            if rel == Relation::Between {
                for &a in &sets[0] {
                    for &b in &sets[1] {
                        if a == b || a == t || b == t {
                            continue;
                        }
                        let value = relations::between(tb, &self.boxes[a], &self.boxes[b], p);
                        consider(Best {
                            value,
                            anchors: [a, b],
                            n_anchors: 2,
                            family,
                            frame: None,
                            note: None,
                        });
                    }
                }
            } else {
                for &a in &sets[0] {
                    if a == t {
                        continue;
                    }
                    let ab = &self.boxes[a];
                    let simple = |value| Best {
                        value,
                        anchors: [a, a],
                        n_anchors: 1,
                        family,
                        frame: None,
                        note: None,
                    };
                    let b = match rel {
                        Relation::Above => simple(relations::above(tb, ab, p)),
                        Relation::Below => simple(relations::below(tb, ab, p)),
                        Relation::On => simple(relations::on(tb, ab, p)),
                        Relation::Under => simple(relations::under(tb, ab, p)),
                        _ => match self.directional_frame(c, a)? {
                            Ok(fr) => Best {
                                value: relations::directional(tb, ab, rel, &fr, p),
                                anchors: [a, a],
                                n_anchors: 1,
                                family: rel.family(fr.kind),
                                frame: Some(fr.kind),
                                note: None,
                            },
                            Err(why) => Best {
                                value: PredicateValue {
                                    score: 0.0,
                                    features: Vec::new(),
                                },
                                anchors: [a, a],
                                n_anchors: 1,
                                family: rel.family(c.frame),
                                frame: Some(c.frame),
                                note: Some(why),
                            },
                        },
                    };
                    consider(b);
                }
            }
            out.push(match best {
                Some(b) => trace(
                    rel,
                    b.family,
                    b.frame,
                    self.ids(&b.anchors[..b.n_anchors]),
                    b.value,
                    b.note,
                ),
                None => not_applicable(rel, family, None, "no anchor different from the candidate itself"),
            });
        }
        Ok(out)
    }

    /// Resolves the reference frame for anchor `a`. The outer `Result` is a hard
    /// error (egocentric without viewpoint), the inner one a "not applicable" note.
    fn directional_frame(
        &self,
        c: &Constraint,
        a: usize,
    ) -> Result<std::result::Result<DirectionalFrame, String>> {
        let ab = &self.boxes[a];
        let vp = self.scene.viewpoint.as_ref();
        let anchor = &self.scene.objects[a].id;
        Ok(match c.frame {
            Frame::Allocentric => allocentric_frame(ab).ok_or_else(|| {
                format!("anchor {anchor} has no intrinsic front, allocentric frame undefined")
            }),
            Frame::Egocentric => match vp {
                Some(v) => Ok(egocentric_frame(ab, v)),
                None => return Err(Error::MissingViewpoint(c.relation.to_string())),
            },
            Frame::Auto => match (allocentric_frame(ab), vp) {
                (Some(f), _) => Ok(f),
                (None, Some(v)) => Ok(egocentric_frame(ab, v)),
                (None, None) => Err(format!(
                    "anchor {anchor} has no front and the scene has no viewpoint"
                )),
            },
        })
    }
}

fn trace(
    relation: Relation,
    family: Family,
    frame: Option<Frame>,
    anchors: Vec<String>,
    v: PredicateValue,
    note: Option<String>,
) -> ConstraintTrace {
    ConstraintTrace {
        relation,
        family,
        frame,
        anchors,
        score: v.score,
        features: v.features,
        note,
    }
}

fn not_applicable(relation: Relation, family: Family, frame: Option<Frame>, why: &str) -> ConstraintTrace {
    ConstraintTrace {
        relation,
        family,
        frame,
        anchors: Vec::new(),
        score: 0.0,
        features: Vec::new(),
        note: Some(why.to_string()),
    }
}

/// The two features that best explain a relation's score.
fn key_features(rel: Relation) -> &'static [&'static str] {
    match rel {
        Relation::Closest | Relation::Farthest => &["distance_m", "footprint_gap_m"],
        Relation::Between => &["containment", "separation"],
        Relation::Above | Relation::Below => &["vertical_gap_m", "xy_overlap_fraction"],
        Relation::On | Relation::Under => &["vertical_gap_m", "supported_fraction"],
        _ => &["sector", "distance_falloff"],
    }
}

fn describe(t: &ConstraintTrace) -> String {
    let mut s = format!("{}({})", t.relation, t.anchors.join(", "));
    if let Some(f) = t.frame {
        let _ = write!(s, "[{f}]");
    }
    let _ = write!(s, " = {:.3}", t.score);
    let feats: Vec<String> = key_features(t.relation)
        .iter()
        .filter_map(|k| t.features.iter().find(|f| f.name == *k))
        .map(|f| format!("{} {:.3}", f.name, f.value))
        .collect();
    if !feats.is_empty() {
        let _ = write!(s, " ({})", feats.join(", "));
    }
    if let Some(n) = &t.note {
        let _ = write!(s, " [{n}]");
    }
    s
}

fn explain(q: &Query, cands: &[Candidate], margin: f64) -> String {
    let mut s = format!("query: {q}\n");
    match cands.first() {
        Some(best) if best.score > 0.0 => {
            let _ = writeln!(
                s,
                "selected {} with score {:.3} (margin {:.3} over rank 2, {} candidate(s))",
                best.id,
                best.score,
                margin,
                cands.len()
            );
        }
        _ => {
            let _ = writeln!(s, "no candidate satisfies the query (all scores are 0)");
        }
    }
    match cands.first() {
        Some(best) if best.score > 0.0 && best.score < LOW_CONFIDENCE => {
            let _ = writeln!(
                s,
                "warning: the best score {:.3} is below {LOW_CONFIDENCE}, the relation barely holds for any candidate",
                best.score
            );
        }
        Some(best) if best.score >= LOW_CONFIDENCE && margin < AMBIGUOUS_MARGIN && cands.len() > 1 => {
            let _ = writeln!(
                s,
                "warning: margin {margin:.3} is small, the expression is ambiguous in this scene"
            );
        }
        _ => {}
    }
    for c in cands.iter().take(EXPLAIN_TOP_K) {
        let parts: Vec<String> = c.trace.iter().map(describe).collect();
        let _ = writeln!(
            s,
            "  #{} {} score {:.3}{}{}",
            c.rank,
            c.id,
            c.score,
            if parts.is_empty() { "" } else { ": " },
            parts.join(" AND ")
        );
    }
    if cands.len() > EXPLAIN_TOP_K {
        let _ = writeln!(
            s,
            "  ... {} more candidate(s), see the trace",
            cands.len() - EXPLAIN_TOP_K
        );
    }
    s.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Constraint;

    fn resolver() -> Resolver {
        Resolver::new(Scene::from_json(include_str!("../../../examples/scenes/tabletop.json")).unwrap())
    }

    fn best(q: Query) -> String {
        resolver().resolve(&q).unwrap().best.unwrap()
    }

    #[test]
    fn tabletop_queries() {
        assert_eq!(
            best(Query::new("mug").and(Constraint::new(Relation::Closest, "laptop"))),
            "mug_2"
        );
        assert_eq!(
            best(Query::new("mug").and(Constraint::new(Relation::Farthest, "laptop"))),
            "mug_1"
        );
        assert_eq!(
            best(Query::new("mug").and(Constraint::new(Relation::Left, "laptop"))),
            "mug_2"
        );
        assert_eq!(
            best(Query::new("mug").and(Constraint::new(Relation::Right, "laptop"))),
            "mug_1"
        );
        assert_eq!(
            best(Query::new("mug").and(Constraint::new(Relation::Left, "chair"))),
            "mug_1"
        );
        assert_eq!(
            best(
                Query::new("mug").and(Constraint::new(Relation::Left, "chair").with_frame(Frame::Egocentric))
            ),
            "mug_2"
        );
        assert_eq!(
            best(Query::new("mug").and(Constraint::new(Relation::Closest, "bowl"))),
            "mug_1"
        );
        assert_eq!(
            best(Query::new("mug").and(Constraint::new(Relation::Closest, "book"))),
            "mug_2"
        );
        assert_eq!(
            best(Query::new("book").and(Constraint::new(Relation::On, "table"))),
            "book_1"
        );
    }

    #[test]
    fn trace_contains_frames_and_features() {
        let r = resolver();
        let res = r
            .resolve(&Query::new("mug").and(Constraint::new(Relation::Left, "chair")))
            .unwrap();
        let t = &res.candidates[0].trace[0];
        assert_eq!(t.frame, Some(Frame::Allocentric));
        assert_eq!(t.family, Family::Allocentric);
        assert_eq!(t.anchors, vec!["chair_1"]);
        assert!(t.features.iter().any(|f| f.name == "sector"));
        assert!(res.explanation.contains("selected mug_1"));
        assert!(res.margin > 0.0);
        let json = res.to_json();
        assert!(json.contains("\"sector\""));
    }

    #[test]
    fn auto_frame_falls_back_to_egocentric() {
        // The bowl has no front, so "left of the bowl" uses the viewpoint.
        let r = resolver();
        let res = r
            .resolve(&Query::new("mug").and(Constraint::new(Relation::Left, "bowl")))
            .unwrap();
        assert_eq!(res.candidates[0].trace[0].frame, Some(Frame::Egocentric));
        let res = r
            .resolve(
                &Query::new("mug")
                    .and(Constraint::new(Relation::Left, "bowl").with_frame(Frame::Allocentric)),
            )
            .unwrap();
        assert_eq!(res.best, None);
        assert!(res.candidates[0].trace[0]
            .note
            .as_deref()
            .unwrap()
            .contains("no intrinsic front"));
    }

    #[test]
    fn errors() {
        let r = resolver();
        assert!(matches!(
            r.resolve(&Query::new("cat")),
            Err(Error::UnknownReference(_))
        ));
        assert!(matches!(
            r.resolve(&Query::new("mug").and(Constraint::new(Relation::On, "sofa"))),
            Err(Error::UnknownReference(_))
        ));
        let mut s = r.scene().clone();
        s.viewpoint = None;
        let r2 = Resolver::new(s);
        assert!(matches!(
            r2.resolve(
                &Query::new("mug").and(Constraint::new(Relation::Left, "bowl").with_frame(Frame::Egocentric))
            ),
            Err(Error::MissingViewpoint(_))
        ));
    }

    #[test]
    fn conjunction_multiplies_scores() {
        let r = resolver();
        let q = Query::new("mug")
            .and(Constraint::new(Relation::On, "table"))
            .and(Constraint::new(Relation::Right, "laptop"));
        let res = r.resolve(&q).unwrap();
        let c = &res.candidates[0];
        assert_eq!(c.id, "mug_1");
        let prod: f64 = c.trace.iter().map(|t| t.score).product();
        assert!((c.score - prod).abs() < 1e-12);
    }

    #[test]
    fn candidate_restriction_and_id_anchors() {
        let r = resolver();
        let q = Query {
            target: "mug".into(),
            constraints: vec![Constraint {
                relation: Relation::Closest,
                anchors: vec![ObjectRef::id("bowl_1")],
                frame: Frame::Auto,
            }],
            candidates: Some(vec!["mug_2".into()]),
        };
        let res = r.resolve(&q).unwrap();
        assert_eq!(res.candidates.len(), 1);
        assert_eq!(res.best.as_deref(), Some("mug_2"));
    }

    #[test]
    fn between_laptop_and_bowl() {
        let r = resolver();
        let res = r
            .resolve(&Query::new("mug").and(Constraint::between("book", "bowl")))
            .unwrap();
        // The book (0.38, 0.3) and the bowl (0.7, -0.3) span a diagonal band; neither mug is inside.
        assert!(
            res.candidates.iter().all(|c| c.score < 0.6),
            "{}",
            res.explanation
        );
    }
}
