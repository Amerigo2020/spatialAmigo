//! Seeded generator of indoor scenes with Sr3D-style referring expressions.
//!
//! Every sample is built constructively: the target is *placed* so that the
//! relation holds, distractors of the same class are placed so that it does
//! not, and a crisp, threshold-based oracle (independent of spatialAmigo's
//! soft scores, see [`crate::oracle`]) must confirm both before the sample is
//! accepted.

use std::f64::consts::PI;

use serde::Serialize;
use spatialamigo::geometry::{polygons_intersect, Obb, Vec2};
use spatialamigo::relations::{
    allocentric_frame, egocentric_frame, DirectionalFrame, Family, Frame, Relation,
};
use spatialamigo::{Constraint, Query, Scene, SceneObject, Viewpoint};

use crate::oracle::{self, Sector};
use crate::rng::Rng;

/// Half side length of the square room (m).
pub const ROOM: f64 = 2.5;

/// Where an object class lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Stands on the floor.
    Floor,
    /// Small item that goes on surfaces.
    Surface,
    /// Mounted on a wall or hanging (elevated).
    Elevated,
}

/// Object class with nominal size `[depth (x), width (y), height]`.
#[derive(Debug, Clone, Copy)]
pub struct Kind {
    /// Label.
    pub label: &'static str,
    /// Nominal size.
    pub size: [f64; 3],
    /// Has an intrinsic front along +x of its own frame.
    pub front: bool,
    /// Placement type.
    pub place: Place,
}

const fn k(label: &'static str, size: [f64; 3], front: bool, place: Place) -> Kind {
    Kind {
        label,
        size,
        front,
        place,
    }
}

/// The object catalog.
pub const CATALOG: &[Kind] = &[
    k("table", [0.8, 1.2, 0.75], false, Place::Floor),
    k("desk", [0.7, 1.4, 0.75], false, Place::Floor),
    k("bed", [2.0, 1.6, 0.5], true, Place::Floor),
    k("sofa", [0.9, 2.0, 0.85], true, Place::Floor),
    k("chair", [0.5, 0.5, 0.9], true, Place::Floor),
    k("armchair", [0.8, 0.8, 0.9], true, Place::Floor),
    k("cabinet", [0.5, 0.8, 1.0], true, Place::Floor),
    k("tv", [0.3, 1.2, 1.2], true, Place::Floor),
    k("nightstand", [0.4, 0.5, 0.55], false, Place::Floor),
    k("door", [0.1, 0.9, 2.0], true, Place::Floor),
    k("trash can", [0.3, 0.3, 0.4], false, Place::Floor),
    k("box", [0.4, 0.3, 0.3], false, Place::Floor),
    k("plant", [0.4, 0.4, 0.8], false, Place::Floor),
    k("lamp", [0.3, 0.3, 1.5], false, Place::Floor),
    k("mug", [0.08, 0.08, 0.1], false, Place::Surface),
    k("book", [0.16, 0.22, 0.03], false, Place::Surface),
    k("bowl", [0.16, 0.16, 0.06], false, Place::Surface),
    k("bottle", [0.07, 0.07, 0.25], false, Place::Surface),
    k("laptop", [0.24, 0.32, 0.02], true, Place::Surface),
    k("monitor", [0.2, 0.55, 0.4], true, Place::Surface),
    k("picture", [0.05, 0.8, 0.6], false, Place::Elevated),
    k("shelf", [0.3, 1.0, 0.3], false, Place::Elevated),
    k("clock", [0.05, 0.3, 0.3], false, Place::Elevated),
];

/// Looks up a catalog entry.
pub fn kind(label: &str) -> Kind {
    *CATALOG
        .iter()
        .find(|k| k.label == label)
        .unwrap_or_else(|| panic!("unknown kind {label}"))
}

/// One benchmark sample.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    /// Running index.
    pub index: usize,
    /// Relation family.
    pub family: Family,
    /// Relation.
    pub relation: Relation,
    /// Generated scene.
    pub scene: Scene,
    /// Referring expression.
    pub sentence: String,
    /// Ground-truth structured query.
    pub query: Query,
    /// Id of the referred object.
    pub target_id: String,
    /// Number of objects of the target class.
    pub n_candidates: usize,
}

/// Incremental scene builder with collision checks.
#[derive(Debug, Default)]
pub struct Builder {
    objects: Vec<SceneObject>,
    boxes: Vec<Obb>,
    counts: std::collections::HashMap<&'static str, usize>,
    viewpoint: Option<Viewpoint>,
}

impl Builder {
    fn id_for(&mut self, label: &'static str) -> String {
        let c = self.counts.entry(label).or_insert(0);
        *c += 1;
        format!("{}_{}", label.replace(' ', "_"), c)
    }

    /// Adds an object if it fits in the room and does not collide. Returns its index.
    fn add(&mut self, kind: Kind, center: [f64; 3], size: [f64; 3], yaw: f64) -> Option<usize> {
        let front = kind.front.then_some([1.0, 0.0, 0.0]);
        let probe = SceneObject {
            id: String::new(),
            label: kind.label.into(),
            center,
            size,
            yaw,
            front,
        };
        let b = Obb::from_object(&probe);
        if b.footprint.iter().any(|p| p.x.abs() > ROOM || p.y.abs() > ROOM) {
            return None;
        }
        let z = b.z();
        for o in &self.boxes {
            if z.overlap(o.z()) > 1e-6 && polygons_intersect(&b.footprint, &o.footprint) {
                return None;
            }
        }
        let id = self.id_for(kind.label);
        self.objects.push(SceneObject { id, ..probe });
        self.boxes.push(b);
        Some(self.objects.len() - 1)
    }

    fn jitter(rng: &mut Rng, k: Kind) -> [f64; 3] {
        let s = rng.range(0.85, 1.15);
        [k.size[0] * s, k.size[1] * s, k.size[2] * rng.range(0.9, 1.1)]
    }

    /// Places `kind` on the floor at a random free spot.
    fn floor(&mut self, rng: &mut Rng, kind: Kind) -> Option<usize> {
        for _ in 0..60 {
            let size = Self::jitter(rng, kind);
            let c = [rng.range(-ROOM, ROOM), rng.range(-ROOM, ROOM), size[2] / 2.0];
            if let Some(i) = self.add(kind, c, size, rng.range(-PI, PI)) {
                return Some(i);
            }
        }
        None
    }

    /// Places `kind` on the floor at `xy` (exact).
    fn floor_at(&mut self, rng: &mut Rng, kind: Kind, xy: Vec2) -> Option<usize> {
        let size = Self::jitter(rng, kind);
        self.add(kind, [xy.x, xy.y, size[2] / 2.0], size, rng.range(-PI, PI))
    }

    /// Places a surface item on top of object `on`.
    fn on_top(&mut self, rng: &mut Rng, kind: Kind, on: usize) -> Option<usize> {
        let base = self.boxes[on].clone();
        for _ in 0..40 {
            let size = Self::jitter(rng, kind);
            let (u, v) = (rng.range(-0.6, 0.6), rng.range(-0.6, 0.6));
            let xy = base.center_xy()
                + base.axis_x.scale(u * (base.half[0] - size[0] / 2.0).max(0.0))
                + base.axis_y.scale(v * (base.half[1] - size[1] / 2.0).max(0.0));
            let c = [xy.x, xy.y, base.z().hi + size[2] / 2.0];
            if let Some(i) = self.add(kind, c, size, base.yaw + rng.range(-0.4, 0.4)) {
                return Some(i);
            }
        }
        None
    }

    /// Places an elevated object with its bottom at `z_bottom` above `xy`.
    fn elevated_at(&mut self, rng: &mut Rng, kind: Kind, xy: Vec2, z_bottom: f64, yaw: f64) -> Option<usize> {
        let size = Self::jitter(rng, kind);
        self.add(kind, [xy.x, xy.y, z_bottom + size[2] / 2.0], size, yaw)
    }

    /// Adds `n` random clutter objects whose labels are not in `exclude`.
    fn clutter(&mut self, rng: &mut Rng, n: usize, exclude: &[&str]) {
        let pool: Vec<Kind> = CATALOG
            .iter()
            .copied()
            .filter(|k| k.place == Place::Floor && !exclude.contains(&k.label))
            .collect();
        for _ in 0..n {
            let k = *rng.choose(&pool);
            let _ = self.floor(rng, k);
        }
    }

    fn finish(self) -> Scene {
        let mut s = Scene::new();
        s.objects = self.objects;
        s.viewpoint = self.viewpoint;
        s
    }

    fn id(&self, i: usize) -> String {
        self.objects[i].id.clone()
    }
}

fn pick_kinds(rng: &mut Rng, labels: &[&'static str], n: usize) -> Vec<Kind> {
    let mut l: Vec<&'static str> = labels.to_vec();
    rng.shuffle(&mut l);
    l.into_iter().take(n).map(kind).collect()
}

fn fill(template: &str, t: &str, a: &str, b: Option<&str>) -> String {
    let s = template.replace("{t}", t).replace("{a}", a);
    match b {
        Some(b) => s.replace("{b}", b),
        None => s,
    }
}

/// Sentence templates per relation (Sr3D-like phrasing with some variation).
pub fn templates(rel: Relation) -> &'static [&'static str] {
    match rel {
        Relation::Closest => &[
            "the {t} closest to the {a}",
            "the {t} that is closest to the {a}",
            "find the {t} nearest to the {a}",
            "the {t} next to the {a}",
            "pick the {t} that is the closest to the {a}",
        ],
        Relation::Farthest => &[
            "the {t} farthest from the {a}",
            "the {t} that is farthest from the {a}",
            "find the {t} that is furthest away from the {a}",
            "the {t} far from the {a}",
        ],
        Relation::Between => &[
            "the {t} between the {a} and the {b}",
            "the {t} that is between the {a} and the {b}",
            "find the {t} in between the {a} and the {b}",
            "the {t} in the middle of the {a} and the {b}",
        ],
        Relation::Above => &[
            "the {t} above the {a}",
            "the {t} that is above the {a}",
            "the {t} hanging over the {a}",
            "find the {t} over the {a}",
        ],
        Relation::Below => &[
            "the {t} below the {a}",
            "the {t} that is below the {a}",
            "the {t} beneath the {a}",
        ],
        Relation::On => &[
            "the {t} on the {a}",
            "the {t} on top of the {a}",
            "the {t} that is supported by the {a}",
            "pick up the {t} sitting on the {a}",
            "the {t} lying on the {a}",
        ],
        Relation::Under => &[
            "the {t} under the {a}",
            "the {t} that is supporting the {a}",
            "the {t} underneath the {a}",
            "the {t} holding the {a}",
        ],
        Relation::Left => &[
            "the {t} to the left of the {a}",
            "the {t} on the left side of the {a}",
            "the {t} that is left of the {a}",
        ],
        Relation::Right => &[
            "the {t} to the right of the {a}",
            "the {t} on the right side of the {a}",
            "the {t} that is right of the {a}",
        ],
        Relation::Front => &[
            "the {t} in front of the {a}",
            "the {t} that is in front of the {a}",
        ],
        Relation::Behind => &["the {t} behind the {a}", "the {t} at the back of the {a}"],
    }
}

/// Held-out paraphrases. They were written after the parser and are not used
/// to tune it; they measure how the template parser copes with unseen wording.
pub fn paraphrases(rel: Relation) -> &'static [&'static str] {
    match rel {
        Relation::Closest => &[
            "the {t} that is nearest the {a}",
            "which {t} is closest to the {a}?",
            "the {t} by the {a}",
            "the {t} located closest to the {a}",
            "the {t} that's nearest to the {a}",
        ],
        Relation::Farthest => &[
            "the {t} that is the most distant from the {a}",
            "which {t} is farthest away from the {a}?",
            "the {t} located furthest from the {a}",
            "the {t} that's far away from the {a}",
        ],
        Relation::Between => &[
            "the {t} located between the {a} and the {b}",
            "which {t} is in between the {a} and the {b}?",
            "the {t} that sits between the {a} and the {b}",
            "the {t} positioned in the middle of the {a} and the {b}",
        ],
        Relation::Above => &[
            "the {t} mounted above the {a}",
            "the {t} that hangs over the {a}",
            "which {t} is higher than the {a}?",
            "the {t} on the wall above the {a}",
        ],
        Relation::Below => &[
            "the {t} located below the {a}",
            "the {t} that sits beneath the {a}",
            "which {t} is lower than the {a}?",
            "the {t} right below the {a}",
        ],
        Relation::On => &[
            "the {t} resting on the {a}",
            "the {t} placed on top of the {a}",
            "which {t} is sitting on the {a}?",
            "the {t} that lies upon the {a}",
            "the {t} standing on the {a}",
        ],
        Relation::Under => &[
            "the {t} with the {a} on it",
            "the {t} that the {a} is on",
            "the {t} supporting the {a}",
            "which {t} is underneath the {a}?",
        ],
        Relation::Left => &[
            "the {t} on the {a}'s left",
            "the {t} left of the {a}",
            "the {t} positioned to the left side of the {a}",
            "which {t} is on the left of the {a}?",
        ],
        Relation::Right => &[
            "the {t} on the {a}'s right",
            "the {t} right of the {a}",
            "the {t} positioned to the right side of the {a}",
            "which {t} is on the right of the {a}?",
        ],
        Relation::Front => &[
            "the {t} located in front of the {a}",
            "the {t} right in front of the {a}",
            "which {t} is at the front of the {a}?",
            "the {t} facing the {a}",
        ],
        Relation::Behind => &[
            "the {t} located behind the {a}",
            "the {t} right behind the {a}",
            "the {t} in back of the {a}",
            "which {t} is at the back of the {a}?",
        ],
    }
}

/// Re-phrases a sample's sentence with a held-out paraphrase. Returns the
/// template and the sentence.
pub fn paraphrase(rng: &mut Rng, s: &Sample) -> (&'static str, String) {
    let anchors: Vec<String> = s.query.constraints[0]
        .anchors
        .iter()
        .map(|a| a.to_string())
        .collect();
    let template = *rng.choose(paraphrases(s.relation));
    let mut out = fill(
        template,
        &s.query.target,
        &anchors[0],
        anchors.get(1).map(String::as_str),
    );
    if s.family == Family::Egocentric {
        out.push_str(rng.choose(&[
            "",
            " from my point of view",
            " from the camera",
            " as seen from here",
        ]));
    }
    (template, out)
}

/// Copy of the scene with Gaussian pose noise on every object: position
/// (`sigma_pos` m per axis), yaw (`sigma_yaw` rad) and relative size (`sigma_size`).
pub fn perturb(rng: &mut Rng, scene: &Scene, sigma_pos: f64, sigma_yaw: f64, sigma_size: f64) -> Scene {
    let mut s = scene.clone();
    for o in &mut s.objects {
        for c in &mut o.center {
            *c += sigma_pos * rng.normal();
        }
        for d in &mut o.size {
            *d = (*d * (1.0 + sigma_size * rng.normal())).max(0.01);
        }
        o.yaw += sigma_yaw * rng.normal();
    }
    s
}

/// Generates one sample (with rejection sampling). `None` if 200 attempts fail.
pub fn sample(rng: &mut Rng, family: Family, relation: Relation, index: usize) -> Option<Sample> {
    for _ in 0..200 {
        let s = match family {
            Family::HorizontalProximity => proximity(rng, relation),
            Family::Between => between(rng),
            Family::VerticalProximity => vertical(rng, relation),
            Family::Support => support(rng, relation),
            Family::Allocentric | Family::Egocentric => directional(rng, relation, family),
        };
        if let Some(mut s) = s {
            s.index = index;
            return Some(s);
        }
    }
    None
}

struct Partial {
    b: Builder,
    rel: Relation,
    family: Family,
    target: usize,
    target_label: &'static str,
    anchors: Vec<&'static str>,
    frame: Frame,
    sentence: String,
}

fn finish(mut p: Partial, rng: &mut Rng) -> Option<Sample> {
    // Shuffle the numbering within each label so that the referred object is
    // not systematically "_1" (ids are the resolver's final tie breaker).
    let mut by_label: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, o) in p.b.objects.iter().enumerate() {
        by_label.entry(o.label.clone()).or_default().push(i);
    }
    for (label, idx) in by_label {
        let mut nums: Vec<usize> = (1..=idx.len()).collect();
        rng.shuffle(&mut nums);
        for (i, n) in idx.into_iter().zip(nums) {
            p.b.objects[i].id = format!("{}_{}", label.replace(' ', "_"), n);
        }
    }
    let target_id = p.b.id(p.target);
    let n_candidates = p.b.objects.iter().filter(|o| o.label == p.target_label).count();
    if n_candidates < 2 {
        return None;
    }
    let mut c = if p.rel == Relation::Between {
        Constraint::between(p.anchors[0], p.anchors[1])
    } else {
        Constraint::new(p.rel, p.anchors[0])
    };
    c.frame = p.frame;
    Some(Sample {
        index: 0,
        family: p.family,
        relation: p.rel,
        scene: p.b.finish(),
        sentence: p.sentence,
        query: Query::new(p.target_label).and(c),
        target_id,
        n_candidates,
    })
}

const FLOOR_TARGETS: &[&str] = &[
    "chair",
    "box",
    "plant",
    "trash can",
    "lamp",
    "armchair",
    "nightstand",
    "cabinet",
];
const FLOOR_ANCHORS: &[&str] = &["table", "desk", "bed", "sofa", "tv", "cabinet", "door"];

fn proximity(rng: &mut Rng, rel: Relation) -> Option<Sample> {
    let mut b = Builder::default();
    let anchors: Vec<&str> = FLOOR_ANCHORS.iter().copied().filter(|l| *l != "door").collect();
    let ak = kind(rng.choose(&anchors));
    let tk = kind(
        rng.choose(
            &FLOOR_TARGETS
                .iter()
                .copied()
                .filter(|l| *l != ak.label)
                .collect::<Vec<_>>(),
        ),
    );
    let a = b.floor(rng, ak)?;
    let n = rng.int(2, 4);
    let mut ts = Vec::new();
    for _ in 0..n {
        ts.push(b.floor(rng, tk)?);
    }
    let d: Vec<f64> = ts
        .iter()
        .map(|&t| oracle::distance(&b.boxes[t], &b.boxes[a]))
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| d[i].total_cmp(&d[j]));
    let (best, runner) = if rel == Relation::Farthest {
        (order[n - 1], order[n - 2])
    } else {
        (order[0], order[1])
    };
    if (d[best] - d[runner]).abs() < oracle::EPSILON_GAP {
        return None;
    }
    let k = rng.int(0, 3);
    b.clutter(rng, k, &[ak.label, tk.label]);
    let sentence = fill(rng.choose(templates(rel)), tk.label, ak.label, None);
    finish(
        Partial {
            b,
            rel,
            family: Family::HorizontalProximity,
            target: ts[best],
            target_label: tk.label,
            anchors: vec![ak.label],
            frame: Frame::Auto,
            sentence,
        },
        rng,
    )
}

fn between(rng: &mut Rng) -> Option<Sample> {
    let mut b = Builder::default();
    let ks = pick_kinds(
        rng,
        &[
            "table", "desk", "bed", "sofa", "tv", "cabinet", "armchair", "door",
        ],
        2,
    );
    let tk = kind(rng.choose(&["box", "plant", "trash can", "lamp", "chair", "nightstand"]));
    let a = b.floor(rng, ks[0])?;
    let ang = rng.range(-PI, PI);
    let dist = rng.range(1.6, 3.0);
    let ca = b.boxes[a].center_xy();
    let cb = ca + Vec2::new(ang.cos(), ang.sin()).scale(dist);
    let bb = b.floor_at(rng, ks[1], cb)?;
    let t = rng.range(0.35, 0.65);
    let perp = Vec2::new(-ang.sin(), ang.cos()).scale(rng.range(-0.12, 0.12));
    let tc = ca + (cb - ca).scale(t) + perp;
    let target = b.floor_at(rng, tk, tc)?;
    if !oracle::between(&b.boxes[target], &b.boxes[a], &b.boxes[bb]) {
        return None;
    }
    for _ in 0..rng.int(1, 3) {
        let d = b.floor(rng, tk)?;
        if oracle::between(&b.boxes[d], &b.boxes[a], &b.boxes[bb])
            || oracle::in_hull(&b.boxes[d], &b.boxes[a], &b.boxes[bb])
        {
            return None;
        }
    }
    let k = rng.int(0, 2);
    b.clutter(rng, k, &[ks[0].label, ks[1].label, tk.label]);
    let sentence = fill(
        rng.choose(templates(Relation::Between)),
        tk.label,
        ks[0].label,
        Some(ks[1].label),
    );
    finish(
        Partial {
            b,
            rel: Relation::Between,
            family: Family::Between,
            target,
            target_label: tk.label,
            anchors: vec![ks[0].label, ks[1].label],
            frame: Frame::Auto,
            sentence,
        },
        rng,
    )
}

fn vertical(rng: &mut Rng, rel: Relation) -> Option<Sample> {
    let mut b = Builder::default();
    let (target, tk, ak) = if rel == Relation::Above {
        // Elevated target above floor furniture; distractors hang over other furniture or bare floor.
        let ak = kind(rng.choose(&["bed", "sofa", "desk", "table", "cabinet", "nightstand"]));
        let tk = kind(rng.choose(&["picture", "shelf", "clock"]));
        let a = b.floor(rng, ak)?;
        let ab = b.boxes[a].clone();
        let z = ab.z().hi + rng.range(0.3, 0.9);
        let t = b.elevated_at(rng, tk, ab.center_xy(), z, ab.yaw)?;
        for _ in 0..rng.int(1, 2) {
            let other = kind(
                rng.choose(
                    &["table", "desk", "bed", "sofa", "cabinet", "nightstand"]
                        .iter()
                        .copied()
                        .filter(|l| *l != ak.label)
                        .collect::<Vec<_>>(),
                ),
            );
            let xy = match b.floor(rng, other) {
                Some(o) if rng.chance(0.7) => b.boxes[o].center_xy(),
                _ => Vec2::new(rng.range(-ROOM, ROOM), rng.range(-ROOM, ROOM)),
            };
            let (z, yaw) = (rng.range(1.2, 1.8), rng.range(-PI, PI));
            let d = b.elevated_at(rng, tk, xy, z, yaw)?;
            if oracle::above(&b.boxes[d], &ab) {
                return None;
            }
        }
        (t, tk, ak)
    } else {
        // Floor target below an elevated anchor; distractors stand elsewhere.
        let ak = kind(rng.choose(&["shelf", "picture", "clock"]));
        let tk = kind(rng.choose(&["box", "trash can", "plant", "chair", "nightstand"]));
        let t = b.floor(rng, tk)?;
        let tb = b.boxes[t].clone();
        let z = tb.z().hi + rng.range(0.3, 0.8);
        let a = b.elevated_at(rng, ak, tb.center_xy(), z, tb.yaw)?;
        for _ in 0..rng.int(1, 3) {
            let d = b.floor(rng, tk)?;
            if oracle::above(&b.boxes[a], &b.boxes[d]) {
                return None;
            }
        }
        // A second elevated object of another class over one distractor makes it harder.
        (t, tk, ak)
    };
    let a_idx = b.objects.iter().position(|o| o.label == ak.label)?;
    let ab = b.boxes[a_idx].clone();
    let ok = match rel {
        Relation::Above => oracle::above(&b.boxes[target], &ab),
        _ => oracle::above(&ab, &b.boxes[target]),
    };
    if !ok {
        return None;
    }
    let k = rng.int(0, 2);
    b.clutter(rng, k, &[ak.label, tk.label]);
    let sentence = fill(rng.choose(templates(rel)), tk.label, ak.label, None);
    finish(
        Partial {
            b,
            rel,
            family: Family::VerticalProximity,
            target,
            target_label: tk.label,
            anchors: vec![ak.label],
            frame: Frame::Auto,
            sentence,
        },
        rng,
    )
}

fn support(rng: &mut Rng, rel: Relation) -> Option<Sample> {
    let mut b = Builder::default();
    let surfaces = ["table", "desk", "cabinet", "nightstand"];
    let (target, tk, ak) = if rel == Relation::On {
        let ak = kind(rng.choose(&surfaces));
        let tk = kind(rng.choose(&["mug", "book", "bowl", "bottle", "laptop"]));
        let a = b.floor(rng, ak)?;
        let t = b.on_top(rng, tk, a)?;
        for _ in 0..rng.int(1, 3) {
            let other: Vec<&str> = surfaces.iter().copied().filter(|l| *l != ak.label).collect();
            let r = rng.f64();
            let d = if r < 0.4 {
                let ok = kind(rng.choose(&other));
                let o = b.floor(rng, ok)?;
                b.on_top(rng, tk, o)?
            } else if r < 0.7 {
                // Hard negative: on a wall shelf hanging right above the anchor
                // (overlapping footprint, so "nearest to the anchor" cannot tell them apart).
                let ab = b.boxes[a].clone();
                let z = ab.z().hi + rng.range(0.4, 0.7);
                let shelf = b.elevated_at(rng, kind("shelf"), ab.center_xy(), z, ab.yaw)?;
                b.on_top(rng, tk, shelf)?
            } else {
                // Hard negative: on the floor right next to the anchor.
                let ab = b.boxes[a].clone();
                let dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)).normalized()?;
                let xy = ab.center_xy() + dir.scale(ab.half[0].max(ab.half[1]) + rng.range(0.15, 0.5));
                b.floor_at(rng, tk, xy)?
            };
            if oracle::on(&b.boxes[d], &b.boxes[a]) {
                return None;
            }
        }
        (t, tk, ak)
    } else {
        // "the table under the laptop": several tables, only one carries the anchor.
        let tk = kind(rng.choose(&surfaces));
        let ak = kind(rng.choose(&["laptop", "monitor", "mug", "book", "bowl", "bottle"]));
        let t = b.floor(rng, tk)?;
        b.on_top(rng, ak, t)?;
        let others: Vec<&str> = ["mug", "book", "bowl", "bottle", "laptop", "monitor"]
            .iter()
            .copied()
            .filter(|l| *l != ak.label)
            .collect();
        for _ in 0..rng.int(1, 2) {
            let d = b.floor(rng, tk)?;
            if rng.chance(0.6) {
                let ok = kind(rng.choose(&others));
                b.on_top(rng, ok, d)?;
            }
        }
        (t, tk, ak)
    };
    let k = rng.int(0, 2);
    b.clutter(rng, k, &[ak.label, tk.label]);
    let sentence = fill(rng.choose(templates(rel)), tk.label, ak.label, None);
    finish(
        Partial {
            b,
            rel,
            family: Family::Support,
            target,
            target_label: tk.label,
            anchors: vec![ak.label],
            frame: Frame::Auto,
            sentence,
        },
        rng,
    )
}

fn sector_of(rel: Relation) -> Sector {
    match rel {
        Relation::Left => Sector::Left,
        Relation::Right => Sector::Right,
        Relation::Front => Sector::Front,
        _ => Sector::Behind,
    }
}

fn axis_of(fr: &DirectionalFrame, s: Sector) -> Vec2 {
    match s {
        Sector::Front => fr.front,
        Sector::Behind => fr.front.scale(-1.0),
        Sector::Left => fr.left,
        Sector::Right => fr.left.scale(-1.0),
    }
}

fn directional(rng: &mut Rng, rel: Relation, family: Family) -> Option<Sample> {
    let mut b = Builder::default();
    let ego = family == Family::Egocentric;
    let ak = if ego {
        kind(rng.choose(&["table", "desk", "nightstand", "box", "plant", "trash can"]))
    } else {
        kind(rng.choose(&["chair", "sofa", "bed", "armchair", "tv", "cabinet"]))
    };
    let tpool: Vec<&str> = ["box", "plant", "trash can", "lamp", "nightstand", "chair"]
        .iter()
        .copied()
        .filter(|l| *l != ak.label)
        .collect();
    let tk = kind(rng.choose(&tpool));
    let xy = Vec2::new(rng.range(-0.8, 0.8), rng.range(-0.8, 0.8));
    let a = b.floor_at(rng, ak, xy)?;
    let ab = b.boxes[a].clone();
    if ego {
        let ang = rng.range(-PI, PI);
        let pos = Vec2::new(ang.cos(), ang.sin()).scale(rng.range(3.2, 4.0));
        b.viewpoint = Some(Viewpoint {
            position: [pos.x, pos.y, 1.5],
            look_at: [0.0, 0.0, 0.5],
        });
    }
    let fr = if ego {
        egocentric_frame(&ab, b.viewpoint.as_ref()?)
    } else {
        allocentric_frame(&ab)?
    };
    let want = sector_of(rel);
    let place_in = |b: &mut Builder, rng: &mut Rng, s: Sector| -> Option<usize> {
        let axis = axis_of(&fr, s);
        let ang = rng.range(-0.45, 0.45);
        let (sn, cs) = ang.sin_cos();
        let dir = Vec2::new(axis.x * cs - axis.y * sn, axis.x * sn + axis.y * cs);
        let r = ab.extent_along(dir) + rng.range(0.35, 1.2);
        b.floor_at(rng, tk, ab.center_xy() + dir.scale(r))
    };
    let target = place_in(&mut b, rng, want)?;
    if oracle::sector(&b.boxes[target], &ab, &fr) != Some(want) {
        return None;
    }
    let mut others = vec![Sector::Front, Sector::Behind, Sector::Left, Sector::Right];
    others.retain(|s| *s != want);
    for _ in 0..rng.int(1, 3) {
        let s = *rng.choose(&others);
        let d = place_in(&mut b, rng, s)?;
        if oracle::sector(&b.boxes[d], &ab, &fr) == Some(want) {
            return None;
        }
    }
    let k = rng.int(0, 2);
    b.clutter(rng, k, &[ak.label, tk.label]);
    let mut sentence = fill(rng.choose(templates(rel)), tk.label, ak.label, None);
    let frame = if ego {
        let suffix = *rng.choose(&[
            "",
            "",
            " from my perspective",
            " from where i stand",
            " as i see it",
        ]);
        sentence.push_str(suffix);
        Frame::Egocentric
    } else {
        if rng.chance(0.25) {
            sentence = format!("from the {}'s point of view, {sentence}", ak.label);
        }
        Frame::Allocentric
    };
    finish(
        Partial {
            b,
            rel,
            family,
            target,
            target_label: tk.label,
            anchors: vec![ak.label],
            frame,
            sentence,
        },
        rng,
    )
}

/// The (family, relation) pairs of the benchmark.
pub fn tasks() -> Vec<(Family, Relation)> {
    use Relation::*;
    vec![
        (Family::HorizontalProximity, Closest),
        (Family::HorizontalProximity, Farthest),
        (Family::Between, Between),
        (Family::VerticalProximity, Above),
        (Family::VerticalProximity, Below),
        (Family::Support, On),
        (Family::Support, Under),
        (Family::Allocentric, Left),
        (Family::Allocentric, Right),
        (Family::Allocentric, Front),
        (Family::Allocentric, Behind),
        (Family::Egocentric, Left),
        (Family::Egocentric, Right),
        (Family::Egocentric, Front),
        (Family::Egocentric, Behind),
    ]
}

/// Generates `per_task` samples for every (family, relation) pair.
pub fn generate(seed: u64, per_task: usize) -> Vec<Sample> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for (family, rel) in tasks() {
        for _ in 0..per_task {
            let idx = out.len();
            if let Some(s) = sample(&mut rng, family, rel, idx) {
                out.push(s);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_scenes_for_every_task() {
        let samples = generate(1, 5);
        assert_eq!(samples.len(), tasks().len() * 5);
        for s in &samples {
            s.scene.validate().unwrap();
            assert!(s.n_candidates >= 2, "{}", s.sentence);
            assert!(s.scene.get(&s.target_id).is_some());
        }
    }

    #[test]
    fn is_deterministic() {
        let a = generate(42, 3);
        let b = generate(42, 3);
        let sa: Vec<_> = a
            .iter()
            .map(|s| (s.sentence.clone(), s.scene.to_json()))
            .collect();
        let sb: Vec<_> = b
            .iter()
            .map(|s| (s.sentence.clone(), s.scene.to_json()))
            .collect();
        assert_eq!(sa, sb);
    }
}
