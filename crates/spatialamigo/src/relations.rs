//! Spatial relations and their continuous scores.
//!
//! Every predicate maps a target box (and one or two anchor boxes) to a score
//! in `[0, 1]` together with the intermediate quantities ("features") that
//! produced it. The formulas are documented in `docs/relations.md`; the
//! families follow Sr3D (Achlioptas et al., ECCV 2020) plus an egocentric
//! family that Sr3D does not contain.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::geometry::{convex_hull, intersection_area, point_polygon_distance, sigmoid, Obb, Vec2};
use crate::scene::Viewpoint;

/// A spatial relation between a target and its anchor(s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// Horizontally closest to the anchor (superlative over candidates).
    Closest,
    /// Horizontally farthest from the anchor (superlative over candidates).
    Farthest,
    /// Between two anchors in the top view.
    Between,
    /// Above the anchor without touching it.
    Above,
    /// Below the anchor without touching it.
    Below,
    /// Resting on (supported by) the anchor.
    On,
    /// Directly under and supporting the anchor.
    Under,
    /// Left of the anchor.
    Left,
    /// Right of the anchor.
    Right,
    /// In front of the anchor.
    Front,
    /// Behind the anchor.
    Behind,
}

/// All relations, in a stable order.
pub const ALL_RELATIONS: [Relation; 11] = [
    Relation::Closest,
    Relation::Farthest,
    Relation::Between,
    Relation::Above,
    Relation::Below,
    Relation::On,
    Relation::Under,
    Relation::Left,
    Relation::Right,
    Relation::Front,
    Relation::Behind,
];

/// Reference frame for directional relations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// Allocentric if the anchor has an intrinsic front, otherwise egocentric.
    #[default]
    Auto,
    /// Relative to the anchor's own front orientation.
    Allocentric,
    /// Relative to the observer (scene viewpoint).
    Egocentric,
}

/// Relation families as used for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// closest, farthest
    HorizontalProximity,
    /// between
    Between,
    /// above, below
    VerticalProximity,
    /// on, under
    Support,
    /// left/right/front/behind relative to the anchor's front
    Allocentric,
    /// left/right/front/behind relative to the viewpoint
    Egocentric,
}

impl Relation {
    /// Snake case name, identical to the JSON representation.
    pub fn name(self) -> &'static str {
        match self {
            Relation::Closest => "closest",
            Relation::Farthest => "farthest",
            Relation::Between => "between",
            Relation::Above => "above",
            Relation::Below => "below",
            Relation::On => "on",
            Relation::Under => "under",
            Relation::Left => "left",
            Relation::Right => "right",
            Relation::Front => "front",
            Relation::Behind => "behind",
        }
    }

    /// Number of anchors the relation needs.
    pub fn arity(self) -> usize {
        if self == Relation::Between {
            2
        } else {
            1
        }
    }

    /// True for relations whose score depends on the whole candidate set.
    pub fn is_superlative(self) -> bool {
        matches!(self, Relation::Closest | Relation::Farthest)
    }

    /// True for left/right/front/behind.
    pub fn is_directional(self) -> bool {
        matches!(
            self,
            Relation::Left | Relation::Right | Relation::Front | Relation::Behind
        )
    }

    /// Family of the relation. Directional relations need the resolved frame.
    pub fn family(self, frame: Frame) -> Family {
        match self {
            Relation::Closest | Relation::Farthest => Family::HorizontalProximity,
            Relation::Between => Family::Between,
            Relation::Above | Relation::Below => Family::VerticalProximity,
            Relation::On | Relation::Under => Family::Support,
            _ => match frame {
                Frame::Egocentric => Family::Egocentric,
                _ => Family::Allocentric,
            },
        }
    }

    /// Mirror image of a directional relation under a reflection that flips
    /// handedness: left and right swap, everything else stays.
    pub fn mirrored(self) -> Relation {
        match self {
            Relation::Left => Relation::Right,
            Relation::Right => Relation::Left,
            r => r,
        }
    }

    /// Parses the snake case name.
    pub fn from_name(s: &str) -> Option<Relation> {
        ALL_RELATIONS.iter().copied().find(|r| r.name() == s)
    }
}

impl fmt::Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Family {
    /// Snake case name.
    pub fn name(self) -> &'static str {
        match self {
            Family::HorizontalProximity => "horizontal_proximity",
            Family::Between => "between",
            Family::VerticalProximity => "vertical_proximity",
            Family::Support => "support",
            Family::Allocentric => "allocentric",
            Family::Egocentric => "egocentric",
        }
    }
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Frame::Auto => "auto",
            Frame::Allocentric => "allocentric",
            Frame::Egocentric => "egocentric",
        })
    }
}

/// Tunable constants of all predicates. Defaults are documented in `docs/relations.md`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    /// Temperature (m) of the soft min/max for closest and farthest.
    pub proximity_tau: f64,
    /// Weight of the center distance added to the footprint gap (tie breaker).
    pub center_weight: f64,
    /// Width (m) of the soft fall-off outside footprints and hulls.
    pub xy_sigma: f64,
    /// Width (m) of the soft fall-off for vertical misalignment in `between`.
    pub z_sigma: f64,
    /// Vertical gap (m) below which two boxes count as touching.
    pub touch_tolerance: f64,
    /// Softness (m) of the touching threshold for above/below.
    pub touch_softness: f64,
    /// Width (m) of the contact term for on/under.
    pub contact_sigma: f64,
    /// Softness of the sector boundaries for directional relations.
    pub sector_softness: f64,
    /// Length scale (m) that smooths sector corners and the sign of a side.
    pub sector_smoothing: f64,
    /// Distance (m) at which the directional score halves.
    pub direction_falloff: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            proximity_tau: 0.15,
            center_weight: 0.1,
            xy_sigma: 0.15,
            z_sigma: 0.15,
            touch_tolerance: 0.03,
            touch_softness: 0.01,
            contact_sigma: 0.03,
            sector_softness: 0.15,
            sector_smoothing: 0.05,
            direction_falloff: 2.0,
        }
    }
}

/// A named intermediate quantity of a predicate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Feature {
    /// Name, e.g. `"footprint_gap_m"`.
    pub name: &'static str,
    /// Value.
    pub value: f64,
}

/// Score of a predicate plus the features that explain it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PredicateValue {
    /// Score in `[0, 1]`.
    pub score: f64,
    /// Explanation features in evaluation order.
    pub features: Vec<Feature>,
}

impl PredicateValue {
    fn new(score: f64, features: &[(&'static str, f64)]) -> Self {
        PredicateValue {
            score: score.clamp(0.0, 1.0),
            features: features
                .iter()
                .map(|&(name, value)| Feature { name, value })
                .collect(),
        }
    }

    /// Looks up a feature by name.
    pub fn feature(&self, name: &str) -> Option<f64> {
        self.features.iter().find(|f| f.name == name).map(|f| f.value)
    }
}

fn gauss(x: f64, sigma: f64) -> f64 {
    (-(x / sigma).powi(2)).exp()
}

/// Soft XY overlap term: half from the overlap fraction, half from a Gaussian of the gap.
fn xy_term(frac: f64, gap: f64, p: &Params) -> f64 {
    0.5 * frac + 0.5 * gauss(gap, p.xy_sigma)
}

/// Raw horizontal distance used by closest/farthest:
/// `d = gap(footprints) + center_weight * |c_t - c_a|`.
pub fn horizontal_distance(t: &Obb, a: &Obb, p: &Params) -> PredicateValue {
    let gap = t.footprint_distance(a);
    let center = (t.center_xy() - a.center_xy()).norm();
    let d = gap + p.center_weight * center;
    PredicateValue {
        score: 0.0,
        features: vec![
            Feature {
                name: "footprint_gap_m",
                value: gap,
            },
            Feature {
                name: "center_distance_m",
                value: center,
            },
            Feature {
                name: "distance_m",
                value: d,
            },
        ],
    }
}

/// Soft arg-min (closest) or arg-max (farthest) over the candidates' distances:
/// `s_i = exp(-(d_i - d_min) / tau)` resp. `exp(-(d_max - d_i) / tau)`.
pub fn superlative_scores(distances: &[f64], farthest: bool, p: &Params) -> Vec<f64> {
    if distances.is_empty() {
        return Vec::new();
    }
    let dmin = distances.iter().copied().fold(f64::INFINITY, f64::min);
    let dmax = distances.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    distances
        .iter()
        .map(|&d| {
            let delta = if farthest { dmax - d } else { d - dmin };
            (-delta / p.proximity_tau).exp()
        })
        .collect()
}

/// Target between anchors `a` and `b` (Sr3D definition, made continuous).
pub fn between(t: &Obb, a: &Obb, b: &Obb, p: &Params) -> PredicateValue {
    let mut pts = a.footprint.to_vec();
    pts.extend_from_slice(&b.footprint);
    let hull = convex_hull(&pts);
    let inside_frac = (intersection_area(&t.footprint, &hull) / t.area()).clamp(0.0, 1.0);
    let center_dist = point_polygon_distance(t.center_xy(), &hull);
    let containment = 0.5 * inside_frac + 0.5 * gauss(center_dist, p.xy_sigma);
    let tz = t.z();
    let penetration = [a, b]
        .iter()
        .map(|anc| {
            let xy = t.footprint_overlap_area(anc) / t.area().min(anc.area());
            let z = tz.overlap(anc.z()) / tz.len().min(anc.z().len());
            (xy * z).clamp(0.0, 1.0)
        })
        .fold(0.0, f64::max);
    let separation = 1.0 - penetration;
    let za = gauss(tz.gap(a.z()), p.z_sigma);
    let zb = gauss(tz.gap(b.z()), p.z_sigma);
    let score = containment * separation * za * zb;
    PredicateValue::new(
        score,
        &[
            ("inside_hull_fraction", inside_frac),
            ("center_to_hull_m", center_dist),
            ("containment", containment),
            ("anchor_penetration", penetration),
            ("separation", separation),
            ("z_alignment_a", za),
            ("z_alignment_b", zb),
        ],
    )
}

/// Target above anchor without touching it.
pub fn above(t: &Obb, a: &Obb, p: &Params) -> PredicateValue {
    let overlap = t.footprint_overlap_area(a);
    let frac = (overlap / t.area().min(a.area())).clamp(0.0, 1.0);
    let gap_xy = t.footprint_distance(a);
    let xy = xy_term(frac, gap_xy, p);
    let gap_z = t.z().lo - a.z().hi;
    let clear = sigmoid((gap_z - p.touch_tolerance) / p.touch_softness);
    PredicateValue::new(
        xy * clear,
        &[
            ("xy_overlap_fraction", frac),
            ("xy_gap_m", gap_xy),
            ("xy_term", xy),
            ("vertical_gap_m", gap_z),
            ("clearance", clear),
        ],
    )
}

/// Target below anchor without touching it. Identical to `above(anchor, target)`.
pub fn below(t: &Obb, a: &Obb, p: &Params) -> PredicateValue {
    above(a, t, p)
}

/// Target resting on the anchor: footprint supported by the anchor and bottom
/// face touching the anchor's top face.
pub fn on(t: &Obb, a: &Obb, p: &Params) -> PredicateValue {
    let overlap = t.footprint_overlap_area(a);
    let support = (overlap / t.area()).clamp(0.0, 1.0);
    let gap_xy = t.footprint_distance(a);
    let xy = xy_term(support, gap_xy, p);
    let gap_z = t.z().lo - a.z().hi;
    let contact = gauss(gap_z, p.contact_sigma);
    PredicateValue::new(
        xy * contact,
        &[
            ("supported_fraction", support),
            ("xy_gap_m", gap_xy),
            ("xy_term", xy),
            ("vertical_gap_m", gap_z),
            ("contact", contact),
        ],
    )
}

/// Target directly under the anchor and supporting it. Identical to `on(anchor, target)`.
pub fn under(t: &Obb, a: &Obb, p: &Params) -> PredicateValue {
    on(a, t, p)
}

/// Axes of a directional reference frame in the XY plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionalFrame {
    /// Direction that counts as "front".
    pub front: Vec2,
    /// Direction that counts as "left".
    pub left: Vec2,
    /// Which frame this is (never `Auto`).
    pub kind: Frame,
}

/// Allocentric frame of an anchor with an intrinsic front: `left = z × front`.
pub fn allocentric_frame(anchor: &Obb) -> Option<DirectionalFrame> {
    let f = anchor.front_xy?;
    Some(DirectionalFrame {
        front: f,
        left: f.perp(),
        kind: Frame::Allocentric,
    })
}

/// Egocentric frame: "front" points from the anchor towards the observer and
/// "left" is the observer's left while looking at the anchor.
pub fn egocentric_frame(anchor: &Obb, vp: &Viewpoint) -> DirectionalFrame {
    let eye = Vec2::new(vp.position[0], vp.position[1]);
    let look = Vec2::new(vp.look_at[0] - vp.position[0], vp.look_at[1] - vp.position[1]);
    let sight = (anchor.center_xy() - eye)
        .normalized()
        .or_else(|| look.normalized())
        .unwrap_or(Vec2::new(1.0, 0.0));
    DirectionalFrame {
        front: sight.scale(-1.0),
        left: sight.perp(),
        kind: Frame::Egocentric,
    }
}

/// Directional relation (left/right/front/behind) of target w.r.t. anchor in `frame`.
///
/// The plane around the anchor footprint is split into four sections by the
/// 45° lines through its corners: a point belongs to the side whose face it
/// is farther outside of. Unlike sections bounded by the box diagonals, this
/// keeps "left of the TV" meaningful for thin or elongated anchors. The
/// section membership is softened with logistics and multiplied by a distance
/// fall-off `1 / (1 + gap / direction_falloff)`.
pub fn directional(t: &Obb, a: &Obb, rel: Relation, frame: &DirectionalFrame, p: &Params) -> PredicateValue {
    let r = t.center_xy() - a.center_xy();
    let hf = a.extent_along(frame.front);
    let hl = a.extent_along(frame.left);
    let u = r.dot(frame.front);
    let w = r.dot(frame.left);
    // How far the target center lies outside the anchor's front/back faces and side faces.
    let excess_front = u.abs() - hf;
    let excess_left = w.abs() - hl;
    // +1: clearly beside the anchor (left/right), -1: clearly in front or behind.
    let lateral =
        (excess_left - excess_front) / (excess_left.abs() + excess_front.abs() + p.sector_smoothing);
    let (axis, coord) = match rel {
        Relation::Front => (-lateral, u),
        Relation::Behind => (-lateral, -u),
        Relation::Left => (lateral, w),
        Relation::Right => (lateral, -w),
        _ => (0.0, 0.0),
    };
    let axis_term = sigmoid(axis / p.sector_softness);
    let side = sigmoid(coord / p.sector_smoothing);
    let sector = axis_term * side;
    let gap = t.footprint_distance(a);
    let falloff = 1.0 / (1.0 + gap / p.direction_falloff);
    PredicateValue::new(
        sector * falloff,
        &[
            ("front_coord_m", u),
            ("left_coord_m", w),
            ("lateral_dominance", lateral),
            ("axis", axis_term),
            ("side", side),
            ("sector", sector),
            ("footprint_gap_m", gap),
            ("distance_falloff", falloff),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    const TABLETOP: &str = include_str!("../tests/fixtures/tabletop.json");

    fn boxes() -> (Scene, impl Fn(&str) -> Obb) {
        let s = Scene::from_json(TABLETOP).unwrap();
        let s2 = s.clone();
        (s, move |id: &str| Obb::from_object(s2.get(id).unwrap()))
    }

    #[test]
    fn mugs_and_book_are_on_the_table() {
        let (_, b) = boxes();
        let p = Params::default();
        for id in ["mug_1", "mug_2", "book_1", "laptop_1", "bowl_1"] {
            let v = on(&b(id), &b("table_1"), &p);
            assert!(v.score > 0.95, "{id} on table: {v:?}");
            assert!(
                above(&b(id), &b("table_1"), &p).score < 0.1,
                "{id} touches, so it is not above"
            );
        }
        assert!(under(&b("table_1"), &b("mug_1"), &p).score > 0.4);
        assert!(on(&b("chair_1"), &b("table_1"), &p).score < 0.01);
    }

    #[test]
    fn allocentric_left_of_laptop_and_chair() {
        let (s, b) = boxes();
        let p = Params::default();
        let laptop = allocentric_frame(&b("laptop_1")).unwrap();
        let l2 = directional(&b("mug_2"), &b("laptop_1"), Relation::Left, &laptop, &p).score;
        let l1 = directional(&b("mug_1"), &b("laptop_1"), Relation::Left, &laptop, &p).score;
        assert!(l2 > 0.7 && l1 < 0.2, "laptop left: mug_2={l2} mug_1={l1}");
        // The chair faces -x, so its left is -y: mug_1.
        let chair = allocentric_frame(&b("chair_1")).unwrap();
        let c1 = directional(&b("mug_1"), &b("chair_1"), Relation::Left, &chair, &p).score;
        let c2 = directional(&b("mug_2"), &b("chair_1"), Relation::Left, &chair, &p).score;
        assert!(c1 > c2, "chair left: mug_1={c1} mug_2={c2}");
        // Egocentrically the viewer at x=-0.3 looks along +x, left is +y: mug_2.
        let ego = egocentric_frame(&b("chair_1"), s.viewpoint.as_ref().unwrap());
        let e1 = directional(&b("mug_1"), &b("chair_1"), Relation::Left, &ego, &p).score;
        let e2 = directional(&b("mug_2"), &b("chair_1"), Relation::Left, &ego, &p).score;
        assert!(e2 > e1, "egocentric left of chair: mug_1={e1} mug_2={e2}");
        assert!(allocentric_frame(&b("mug_1")).is_none());
    }

    #[test]
    fn closest_and_farthest_mug_to_laptop() {
        let (_, b) = boxes();
        let p = Params::default();
        let d: Vec<f64> = ["mug_1", "mug_2"]
            .iter()
            .map(|id| {
                horizontal_distance(&b(id), &b("laptop_1"), &p)
                    .feature("distance_m")
                    .unwrap()
            })
            .collect();
        let closest = superlative_scores(&d, false, &p);
        let farthest = superlative_scores(&d, true, &p);
        // mug_2 footprint is 8 cm from the laptop, mug_1 about 11 cm.
        assert_eq!(closest[1], 1.0);
        assert!(closest[0] < 1.0);
        assert_eq!(farthest[0], 1.0);
        assert!(farthest[1] < 1.0);
        assert!(closest
            .iter()
            .zip(&farthest)
            .all(|(c, f)| (0.0..=1.0).contains(c) && (0.0..=1.0).contains(f)));
    }

    #[test]
    fn between_two_boxes() {
        let p = Params::default();
        let a = Obb::new([0.0, 0.0, 0.5], [0.5, 0.5, 1.0], 0.0);
        let b = Obb::new([3.0, 0.0, 0.5], [0.5, 0.5, 1.0], 0.0);
        let mid = Obb::new([1.5, 0.0, 0.3], [0.3, 0.3, 0.6], 0.0);
        let off = Obb::new([1.5, 2.0, 0.3], [0.3, 0.3, 0.6], 0.0);
        let high = Obb::new([1.5, 0.0, 2.5], [0.3, 0.3, 0.6], 0.0);
        assert!(between(&mid, &a, &b, &p).score > 0.95);
        assert!(between(&off, &a, &b, &p).score < 0.05);
        assert!(between(&high, &a, &b, &p).score < 0.05);
    }

    #[test]
    fn above_and_below_are_symmetric() {
        let p = Params::default();
        let lamp = Obb::new([0.0, 0.0, 1.8], [0.3, 0.3, 0.2], 0.0);
        let table = Obb::new([0.0, 0.0, 0.4], [1.0, 1.0, 0.8], 0.0);
        assert!(above(&lamp, &table, &p).score > 0.95);
        assert!(below(&table, &lamp, &p).score > 0.95);
        assert!(above(&table, &lamp, &p).score < 0.01);
    }

    #[test]
    fn relation_names_round_trip() {
        for r in ALL_RELATIONS {
            assert_eq!(Relation::from_name(r.name()), Some(r));
            let json = serde_json::to_string(&r).unwrap();
            assert_eq!(json, format!("\"{}\"", r.name()));
        }
    }
}
