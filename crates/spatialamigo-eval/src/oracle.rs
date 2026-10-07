//! Crisp, threshold-based relation checks in the spirit of the Sr3D generation
//! rules (ReferIt3D supplementary, Sec. 2). They are deliberately implemented
//! separately from spatialAmigo's soft scores and only decide yes/no, so the
//! benchmark ground truth does not reuse the scoring code under test.

use spatialamigo::geometry::{convex_hull, intersection_area, point_in_convex, polygon_distance, Obb};
use spatialamigo::relations::DirectionalFrame;

/// Minimum distance difference (m) between the referred object and the
/// runner-up for closest/farthest (Sr3D's epsilonGap).
pub const EPSILON_GAP: f64 = 0.15;
/// Minimum footprint overlap ratio for vertical and support relations.
pub const OVERLAP_RATIO: f64 = 0.5;
/// Maximum |z gap| (m) for two boxes to touch.
pub const TOUCH: f64 = 0.02;
/// Minimum z gap (m) for above/below (no contact).
pub const CLEARANCE: f64 = 0.05;

/// Top-view distance between the two footprints.
pub fn distance(t: &Obb, a: &Obb) -> f64 {
    polygon_distance(&t.footprint, &a.footprint)
}

/// Target center inside the convex hull of the two anchor footprints.
pub fn in_hull(t: &Obb, a: &Obb, b: &Obb) -> bool {
    let mut pts = a.footprint.to_vec();
    pts.extend_from_slice(&b.footprint);
    point_in_convex(t.center_xy(), &convex_hull(&pts))
}

/// Sr3D between: inside the hull, no intersection with the anchors, z overlap with both.
pub fn between(t: &Obb, a: &Obb, b: &Obb) -> bool {
    in_hull(t, a, b)
        && intersection_area(&t.footprint, &a.footprint) <= 1e-9
        && intersection_area(&t.footprint, &b.footprint) <= 1e-9
        && t.z().overlap(a.z()) > 0.0
        && t.z().overlap(b.z()) > 0.0
}

fn overlap_ratio(t: &Obb, a: &Obb) -> f64 {
    intersection_area(&t.footprint, &a.footprint) / t.area().min(a.area())
}

/// Target above anchor: footprints overlap enough and there is clearance.
pub fn above(t: &Obb, a: &Obb) -> bool {
    overlap_ratio(t, a) >= OVERLAP_RATIO && t.z().lo - a.z().hi >= CLEARANCE
}

/// Target on anchor: most of the target's footprint is carried and the faces touch.
pub fn on(t: &Obb, a: &Obb) -> bool {
    intersection_area(&t.footprint, &a.footprint) / t.area() >= OVERLAP_RATIO
        && (t.z().lo - a.z().hi).abs() <= TOUCH
}

/// Section of the anchor (bounded by 45° lines through its footprint corners) that contains the target center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sector {
    /// +front
    Front,
    /// -front
    Behind,
    /// +left
    Left,
    /// -left
    Right,
}

/// Sector of the target w.r.t. the anchor in `frame` (sections bounded by the
/// 45° lines through the footprint corners), `None` if the target center lies
/// inside the anchor footprint or exactly on a boundary.
pub fn sector(t: &Obb, a: &Obb, frame: &DirectionalFrame) -> Option<Sector> {
    if point_in_convex(t.center_xy(), &a.footprint) {
        return None;
    }
    let r = t.center_xy() - a.center_xy();
    let u = r.dot(frame.front);
    let w = r.dot(frame.left);
    let eu = u.abs() - a.extent_along(frame.front);
    let ew = w.abs() - a.extent_along(frame.left);
    if (eu - ew).abs() < 1e-9 {
        None
    } else if eu > ew {
        Some(if u > 0.0 { Sector::Front } else { Sector::Behind })
    } else {
        Some(if w > 0.0 { Sector::Left } else { Sector::Right })
    }
}
