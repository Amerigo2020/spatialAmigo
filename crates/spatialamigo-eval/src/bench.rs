//! Runs the synthetic benchmark and aggregates accuracy tables.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;
use spatialamigo::geometry::{polygon_distance, Obb};
use spatialamigo::relations::{Family, Frame, Relation};
use spatialamigo::{normalize_label, parse, ObjectRef, Query, Resolver};

use crate::synth::Sample;

/// Outcome for one sample.
#[derive(Debug, Clone, Serialize)]
pub struct SampleResult {
    /// Sample index.
    pub index: usize,
    /// Relation family.
    pub family: Family,
    /// Relation.
    pub relation: Relation,
    /// Sentence.
    pub sentence: String,
    /// Number of candidates of the target class.
    pub n_candidates: usize,
    /// Template parser produced the ground-truth query.
    pub parse_correct: bool,
    /// Top-1 from text (parser + resolver) is the target.
    pub text_correct: bool,
    /// Top-1 from the ground-truth query (resolver only) is the target.
    pub query_correct: bool,
    /// Score margin of the text pipeline (0 if it failed).
    pub margin: f64,
    /// Baseline: candidate nearest to the (first) anchor is the target.
    pub nearest_baseline_correct: bool,
    /// Error message if parsing or resolution failed.
    pub error: Option<String>,
}

/// Aggregate over a group of samples.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Aggregate {
    /// Number of samples.
    pub n: usize,
    /// Mean number of candidates.
    pub mean_candidates: f64,
    /// Parser accuracy (exact query match).
    pub parse_acc: f64,
    /// End-to-end top-1 accuracy from text.
    pub text_acc: f64,
    /// Top-1 accuracy with the ground-truth query.
    pub query_acc: f64,
    /// Expected accuracy of picking a random candidate.
    pub random_acc: f64,
    /// Accuracy of the "nearest to the anchor" heuristic.
    pub nearest_acc: f64,
    /// Mean margin (text pipeline).
    pub mean_margin: f64,
}

fn aggregate<'a>(rs: impl Iterator<Item = &'a SampleResult>) -> Aggregate {
    let rs: Vec<&SampleResult> = rs.collect();
    let n = rs.len();
    if n == 0 {
        return Aggregate::default();
    }
    let mean = |f: &dyn Fn(&SampleResult) -> f64| rs.iter().map(|r| f(r)).sum::<f64>() / n as f64;
    Aggregate {
        n,
        mean_candidates: mean(&|r| r.n_candidates as f64),
        parse_acc: mean(&|r| r.parse_correct as u8 as f64),
        text_acc: mean(&|r| r.text_correct as u8 as f64),
        query_acc: mean(&|r| r.query_correct as u8 as f64),
        random_acc: mean(&|r| 1.0 / r.n_candidates as f64),
        nearest_acc: mean(&|r| r.nearest_baseline_correct as u8 as f64),
        mean_margin: mean(&|r| r.margin),
    }
}

/// Full benchmark report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Seed used.
    pub seed: u64,
    /// Samples per (family, relation).
    pub per_task: usize,
    /// Per (family, relation).
    pub by_relation: Vec<(String, String, Aggregate)>,
    /// Per family.
    pub by_family: Vec<(String, Aggregate)>,
    /// Over all samples.
    pub overall: Aggregate,
    /// Failure cases (first 30) for inspection.
    pub failures: Vec<SampleResult>,
}

fn same_query(parsed: &Query, gt: &Query) -> bool {
    if normalize_label(&parsed.target) != normalize_label(&gt.target)
        || parsed.constraints.len() != gt.constraints.len()
    {
        return false;
    }
    parsed.constraints.iter().zip(&gt.constraints).all(|(p, g)| {
        let anchors = |c: &spatialamigo::Constraint| -> Vec<String> {
            c.anchors
                .iter()
                .map(|a| match a {
                    ObjectRef::Label(l) => normalize_label(l),
                    ObjectRef::Id(i) => format!("#{i}"),
                })
                .collect()
        };
        p.relation == g.relation && anchors(p) == anchors(g) && (p.frame == Frame::Auto || p.frame == g.frame)
    })
}

fn nearest_baseline(s: &Sample) -> bool {
    let anchor_label = match &s.query.constraints[0].anchors[0] {
        ObjectRef::Label(l) => l.clone(),
        ObjectRef::Id(i) => i.clone(),
    };
    let boxes: Vec<(String, String, Obb)> = s
        .scene
        .objects
        .iter()
        .map(|o| (o.id.clone(), o.label.clone(), Obb::from_object(o)))
        .collect();
    let anchors: Vec<&Obb> = boxes
        .iter()
        .filter(|b| b.1 == anchor_label)
        .map(|b| &b.2)
        .collect();
    let best = boxes
        .iter()
        .filter(|b| b.1 == s.query.target)
        .map(|b| {
            // Footprint gap first, 3D center distance as tie breaker.
            let d = anchors
                .iter()
                .map(|a| {
                    let c: f64 = (0..3)
                        .map(|k| (b.2.center[k] - a.center[k]).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    (polygon_distance(&b.2.footprint, &a.footprint), c)
                })
                .fold((f64::INFINITY, f64::INFINITY), |m, x| if x < m { x } else { m });
            (d, &b.0)
        })
        .min_by(|x, y| x.0 .0.total_cmp(&y.0 .0).then(x.0 .1.total_cmp(&y.0 .1)));
    best.is_some_and(|(_, id)| *id == s.target_id)
}

/// Evaluates one sample.
pub fn evaluate(s: &Sample) -> SampleResult {
    let resolver = Resolver::new(s.scene.clone());
    let mut error = None;
    let parsed = parse(&s.sentence, &s.scene.labels());
    let parse_correct = parsed.as_ref().is_ok_and(|q| same_query(q, &s.query));
    let (text_correct, margin) = match parsed.and_then(|q| resolver.resolve(&q)) {
        Ok(r) => (r.best.as_deref() == Some(s.target_id.as_str()), r.margin),
        Err(e) => {
            error = Some(e.to_string());
            (false, 0.0)
        }
    };
    let query_correct = match resolver.resolve(&s.query) {
        Ok(r) => r.best.as_deref() == Some(s.target_id.as_str()),
        Err(e) => {
            error.get_or_insert(e.to_string());
            false
        }
    };
    SampleResult {
        index: s.index,
        family: s.family,
        relation: s.relation,
        sentence: s.sentence.clone(),
        n_candidates: s.n_candidates,
        parse_correct,
        text_correct,
        query_correct,
        margin,
        nearest_baseline_correct: nearest_baseline(s),
        error,
    }
}

/// Evaluates all samples and aggregates.
pub fn run(samples: &[Sample], seed: u64, per_task: usize) -> (Report, Vec<SampleResult>) {
    let results: Vec<SampleResult> = samples.iter().map(evaluate).collect();
    let mut rel_groups: BTreeMap<(Family, Relation), Vec<&SampleResult>> = BTreeMap::new();
    let mut fam_groups: BTreeMap<Family, Vec<&SampleResult>> = BTreeMap::new();
    for r in &results {
        rel_groups.entry((r.family, r.relation)).or_default().push(r);
        fam_groups.entry(r.family).or_default().push(r);
    }
    let report = Report {
        seed,
        per_task,
        by_relation: rel_groups
            .into_iter()
            .map(|((f, r), v)| (f.to_string(), r.to_string(), aggregate(v.into_iter())))
            .collect(),
        by_family: fam_groups
            .into_iter()
            .map(|(f, v)| (f.to_string(), aggregate(v.into_iter())))
            .collect(),
        overall: aggregate(results.iter()),
        failures: results
            .iter()
            .filter(|r| !r.text_correct)
            .take(30)
            .cloned()
            .collect(),
    };
    (report, results)
}

fn pct(x: f64) -> String {
    format!("{:.1}", 100.0 * x)
}

/// Markdown tables of a report.
pub fn markdown(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "Seed {}, {} samples per relation, {} samples in total.\n",
        r.seed, r.per_task, r.overall.n
    );
    let _ = writeln!(s, "| Family | N | Cand. | Parser | Text to target | Query to target | Random | Nearest-anchor | Margin |");
    let _ = writeln!(
        s,
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
    );
    let row = |s: &mut String, name: &str, a: &Aggregate| {
        let _ = writeln!(
            s,
            "| {name} | {} | {:.2} | {} | {} | {} | {} | {} | {:.2} |",
            a.n,
            a.mean_candidates,
            pct(a.parse_acc),
            pct(a.text_acc),
            pct(a.query_acc),
            pct(a.random_acc),
            pct(a.nearest_acc),
            a.mean_margin
        );
    };
    for (f, a) in &r.by_family {
        row(&mut s, f, a);
    }
    row(&mut s, "**all**", &r.overall);
    let _ = writeln!(s, "\nPer relation:\n");
    let _ = writeln!(
        s,
        "| Family | Relation | N | Parser | Text to target | Query to target | Random | Nearest-anchor |"
    );
    let _ = writeln!(s, "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |");
    for (f, rel, a) in &r.by_relation {
        let _ = writeln!(
            s,
            "| {f} | {rel} | {} | {} | {} | {} | {} | {} |",
            a.n,
            pct(a.parse_acc),
            pct(a.text_acc),
            pct(a.query_acc),
            pct(a.random_acc),
            pct(a.nearest_acc)
        );
    }
    s
}

/// Pose noise level for the robustness study.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct NoiseLevel {
    /// Position standard deviation per axis (m).
    pub pos_m: f64,
    /// Yaw standard deviation (degrees).
    pub yaw_deg: f64,
    /// Relative size standard deviation.
    pub size_rel: f64,
}

/// Noise levels reported in the README.
pub const NOISE_LEVELS: [NoiseLevel; 4] = [
    NoiseLevel {
        pos_m: 0.0,
        yaw_deg: 0.0,
        size_rel: 0.0,
    },
    NoiseLevel {
        pos_m: 0.02,
        yaw_deg: 2.0,
        size_rel: 0.05,
    },
    NoiseLevel {
        pos_m: 0.05,
        yaw_deg: 5.0,
        size_rel: 0.10,
    },
    NoiseLevel {
        pos_m: 0.10,
        yaw_deg: 10.0,
        size_rel: 0.20,
    },
];

/// Accuracy of the ground-truth query on perturbed scenes, per family and noise level.
#[derive(Debug, Clone, Serialize)]
pub struct NoiseStudy {
    /// Noise levels (columns).
    pub levels: Vec<NoiseLevel>,
    /// (family, accuracy per level).
    pub rows: Vec<(String, Vec<f64>)>,
    /// Overall accuracy per level.
    pub overall: Vec<f64>,
}

/// Runs the pose noise study. The resolver sees the noisy scene, the ground
/// truth comes from the clean one.
pub fn noise_study(samples: &[Sample], seed: u64) -> NoiseStudy {
    let mut per_family: BTreeMap<Family, Vec<Vec<bool>>> = BTreeMap::new();
    let mut overall = vec![Vec::new(); NOISE_LEVELS.len()];
    for (li, lvl) in NOISE_LEVELS.iter().enumerate() {
        let mut rng = crate::rng::Rng::new(seed ^ (0xA5A5 + li as u64));
        for s in samples {
            let noisy = crate::synth::perturb(
                &mut rng,
                &s.scene,
                lvl.pos_m,
                lvl.yaw_deg.to_radians(),
                lvl.size_rel,
            );
            let ok = Resolver::new(noisy)
                .resolve(&s.query)
                .map(|r| r.best.as_deref() == Some(s.target_id.as_str()))
                .unwrap_or(false);
            let e = per_family
                .entry(s.family)
                .or_insert_with(|| vec![Vec::new(); NOISE_LEVELS.len()]);
            e[li].push(ok);
            overall[li].push(ok);
        }
    }
    let acc = |v: &Vec<bool>| v.iter().filter(|&&b| b).count() as f64 / v.len().max(1) as f64;
    NoiseStudy {
        levels: NOISE_LEVELS.to_vec(),
        rows: per_family
            .into_iter()
            .map(|(f, v)| (f.to_string(), v.iter().map(acc).collect()))
            .collect(),
        overall: overall.iter().map(acc).collect(),
    }
}

/// Markdown table of a noise study.
pub fn noise_markdown(n: &NoiseStudy) -> String {
    let mut s = String::from("| Family |");
    for l in &n.levels {
        let _ = write!(
            s,
            " σ={:.0} cm, {:.0}°, {:.0}% |",
            l.pos_m * 100.0,
            l.yaw_deg,
            l.size_rel * 100.0
        );
    }
    s.push_str("\n| --- |");
    for _ in &n.levels {
        s.push_str(" ---: |");
    }
    s.push('\n');
    for (f, v) in n
        .rows
        .iter()
        .map(|(f, v)| (f.as_str(), v))
        .chain(std::iter::once(("**all**", &n.overall)))
    {
        let _ = write!(s, "| {f} |");
        for a in v {
            let _ = write!(s, " {} |", pct(*a));
        }
        s.push('\n');
    }
    s
}

/// Parser robustness on held-out paraphrases.
#[derive(Debug, Clone, Serialize)]
pub struct ParaphraseStudy {
    /// (family, n, parser accuracy, text-to-target accuracy).
    pub rows: Vec<(String, usize, f64, f64)>,
    /// Overall (n, parser accuracy, text-to-target accuracy).
    pub overall: (usize, f64, f64),
    /// Example failures (sentence, parsed query or error).
    pub failures: Vec<(String, String)>,
}

/// Re-phrases every sample with a held-out paraphrase and runs the text pipeline.
pub fn paraphrase_study(samples: &[Sample], seed: u64) -> ParaphraseStudy {
    let mut rng = crate::rng::Rng::new(seed ^ 0x5EED);
    let mut groups: BTreeMap<Family, (usize, usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in samples {
        let (template, sentence) = crate::synth::paraphrase(&mut rng, s);
        let parsed = parse(&sentence, &s.scene.labels());
        let p_ok = parsed.as_ref().is_ok_and(|q| same_query(q, &s.query));
        let t_ok = parsed
            .as_ref()
            .ok()
            .and_then(|q| Resolver::new(s.scene.clone()).resolve(q).ok())
            .is_some_and(|r| r.best.as_deref() == Some(s.target_id.as_str()));
        let g = groups.entry(s.family).or_default();
        g.0 += 1;
        g.1 += p_ok as usize;
        g.2 += t_ok as usize;
        if !p_ok && failures.len() < 12 && seen.insert(template) {
            let got = match parsed {
                Ok(q) => q.to_string(),
                Err(e) => format!("error: {e}"),
            };
            failures.push((sentence, got));
        }
    }
    let total = groups
        .values()
        .fold((0, 0, 0), |a, g| (a.0 + g.0, a.1 + g.1, a.2 + g.2));
    let f = |n: usize, k: usize| k as f64 / n.max(1) as f64;
    ParaphraseStudy {
        rows: groups
            .into_iter()
            .map(|(fam, (n, p, t))| (fam.to_string(), n, f(n, p), f(n, t)))
            .collect(),
        overall: (total.0, f(total.0, total.1), f(total.0, total.2)),
        failures,
    }
}

/// Markdown table of a paraphrase study.
pub fn paraphrase_markdown(p: &ParaphraseStudy) -> String {
    let mut s = String::from("| Family | N | Parser | Text to target |\n| --- | ---: | ---: | ---: |\n");
    for (f, n, pa, ta) in &p.rows {
        let _ = writeln!(s, "| {f} | {n} | {} | {} |", pct(*pa), pct(*ta));
    }
    let _ = writeln!(
        s,
        "| **all** | {} | {} | {} |",
        p.overall.0,
        pct(p.overall.1),
        pct(p.overall.2)
    );
    let _ = writeln!(s, "\nTypical failures:\n");
    for (sent, got) in &p.failures {
        let _ = writeln!(s, "* \"{sent}\" parsed as `{got}`");
    }
    s
}
