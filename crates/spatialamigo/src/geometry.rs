//! Minimal 2D/3D geometry for oriented boxes that rotate about z.
//!
//! Everything spatialAmigo needs is a box with a yaw angle: its z interval and
//! its footprint, a convex quadrilateral in the XY plane. Footprints are
//! combined with exact convex polygon routines (clipping, hull, distance).

use crate::scene::SceneObject;

/// 2D vector in the XY plane.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    /// x component.
    pub x: f64,
    /// y component.
    pub y: f64,
}

impl Vec2 {
    /// Creates a vector.
    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }
    /// Dot product.
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    /// z component of the 3D cross product.
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    /// Euclidean length.
    pub fn norm(self) -> f64 {
        self.x.hypot(self.y)
    }
    /// Unit vector, or `None` for (near) zero vectors.
    pub fn normalized(self) -> Option<Vec2> {
        let n = self.norm();
        (n > 1e-12).then(|| Vec2::new(self.x / n, self.y / n))
    }
    /// Rotates by +90 degrees (counter-clockwise). For a heading `f` this is
    /// `z × f`, i.e. "left of f" in a right-handed, z-up frame.
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }
    /// Scales the vector.
    pub fn scale(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

/// Closed interval on the real line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    /// Lower bound.
    pub lo: f64,
    /// Upper bound.
    pub hi: f64,
}

impl Interval {
    /// Length of the interval.
    pub fn len(self) -> f64 {
        self.hi - self.lo
    }
    /// True if the interval has zero length.
    pub fn is_empty(self) -> bool {
        self.len() <= 0.0
    }
    /// Length of the overlap with `o` (0 if disjoint).
    pub fn overlap(self, o: Interval) -> f64 {
        (self.hi.min(o.hi) - self.lo.max(o.lo)).max(0.0)
    }
    /// Gap between the intervals (0 if they overlap).
    pub fn gap(self, o: Interval) -> f64 {
        (self.lo.max(o.lo) - self.hi.min(o.hi)).max(0.0)
    }
}

/// Oriented box with rotation about z only, with cached footprint.
#[derive(Debug, Clone, PartialEq)]
pub struct Obb {
    /// Center in world coordinates.
    pub center: [f64; 3],
    /// Half extents along the object's x, y, z axes.
    pub half: [f64; 3],
    /// Rotation about world z (rad).
    pub yaw: f64,
    /// Object x axis in the world XY plane.
    pub axis_x: Vec2,
    /// Object y axis in the world XY plane.
    pub axis_y: Vec2,
    /// Footprint corners, counter-clockwise.
    pub footprint: [Vec2; 4],
    /// Intrinsic front direction projected to world XY and normalized, if any.
    pub front_xy: Option<Vec2>,
}

impl Obb {
    /// Builds a box from center, full size and yaw.
    pub fn new(center: [f64; 3], size: [f64; 3], yaw: f64) -> Self {
        let half = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
        let (s, c) = yaw.sin_cos();
        let axis_x = Vec2::new(c, s);
        let axis_y = Vec2::new(-s, c);
        let o = Vec2::new(center[0], center[1]);
        let ex = axis_x.scale(half[0]);
        let ey = axis_y.scale(half[1]);
        let footprint = [o - ex - ey, o + ex - ey, o + ex + ey, o - ex + ey];
        Obb {
            center,
            half,
            yaw,
            axis_x,
            axis_y,
            footprint,
            front_xy: None,
        }
    }

    /// Builds the box of a scene object including its world front direction.
    pub fn from_object(o: &SceneObject) -> Self {
        let mut b = Obb::new(o.center, o.size, o.yaw);
        if let Some(f) = o.front {
            b.front_xy = (b.axis_x.scale(f[0]) + b.axis_y.scale(f[1])).normalized();
        }
        b
    }

    /// Center projected to XY.
    pub fn center_xy(&self) -> Vec2 {
        Vec2::new(self.center[0], self.center[1])
    }
    /// z extent.
    pub fn z(&self) -> Interval {
        Interval {
            lo: self.center[2] - self.half[2],
            hi: self.center[2] + self.half[2],
        }
    }
    /// Footprint area.
    pub fn area(&self) -> f64 {
        4.0 * self.half[0] * self.half[1]
    }
    /// Half extent of the footprint along a unit direction `d` (support function).
    pub fn extent_along(&self, d: Vec2) -> f64 {
        d.dot(self.axis_x).abs() * self.half[0] + d.dot(self.axis_y).abs() * self.half[1]
    }
}

/// Signed area of a polygon (positive for counter-clockwise order).
pub fn polygon_area(p: &[Vec2]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..p.len() {
        a += p[i].cross(p[(i + 1) % p.len()]);
    }
    0.5 * a
}

/// Convex hull (Andrew's monotone chain), counter-clockwise, no collinear points.
pub fn convex_hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let mut hull: Vec<Vec2> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Vec2>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2 {
                let a = hull[hull.len() - 2];
                let b = hull[hull.len() - 1];
                if (b - a).cross(p - a) <= 1e-15 {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Intersection of two convex, counter-clockwise polygons (Sutherland-Hodgman).
pub fn clip_convex(subject: &[Vec2], clip: &[Vec2]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = subject.to_vec();
    let n = clip.len();
    for i in 0..n {
        if out.is_empty() {
            break;
        }
        let a = clip[i];
        let b = clip[(i + 1) % n];
        let edge = b - a;
        let input = std::mem::take(&mut out);
        let inside = |p: Vec2| edge.cross(p - a) >= -1e-12;
        for j in 0..input.len() {
            let cur = input[j];
            let prev = input[(j + input.len() - 1) % input.len()];
            let (ci, pi) = (inside(cur), inside(prev));
            if ci {
                if !pi {
                    out.push(segment_line_intersection(prev, cur, a, b));
                }
                out.push(cur);
            } else if pi {
                out.push(segment_line_intersection(prev, cur, a, b));
            }
        }
    }
    out
}

fn segment_line_intersection(p: Vec2, q: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let r = q - p;
    let s = b - a;
    let denom = r.cross(s);
    if denom.abs() < 1e-15 {
        return q;
    }
    let t = (a - p).cross(s) / denom;
    p + r.scale(t)
}

/// Area of the intersection of two convex counter-clockwise polygons.
pub fn intersection_area(a: &[Vec2], b: &[Vec2]) -> f64 {
    polygon_area(&clip_convex(a, b)).max(0.0)
}

/// True if `p` lies inside (or on) a convex counter-clockwise polygon.
pub fn point_in_convex(p: Vec2, poly: &[Vec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    (0..n).all(|i| (poly[(i + 1) % n] - poly[i]).cross(p - poly[i]) >= -1e-12)
}

/// Distance from point `p` to segment `ab`.
pub fn point_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    if l2 < 1e-24 {
        return (p - a).norm();
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p - (a + ab.scale(t))).norm()
}

/// Distance from `p` to a convex polygon (0 if inside).
pub fn point_polygon_distance(p: Vec2, poly: &[Vec2]) -> f64 {
    if point_in_convex(p, poly) {
        return 0.0;
    }
    let n = poly.len();
    (0..n)
        .map(|i| point_segment_distance(p, poly[i], poly[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

/// Minimum distance between two convex polygons (0 if they touch or overlap).
pub fn polygon_distance(a: &[Vec2], b: &[Vec2]) -> f64 {
    if polygons_intersect(a, b) {
        return 0.0;
    }
    let mut d = f64::INFINITY;
    for &p in a {
        d = d.min(point_polygon_distance(p, b));
    }
    for &p in b {
        d = d.min(point_polygon_distance(p, a));
    }
    d
}

/// Separating axis test for two convex polygons.
pub fn polygons_intersect(a: &[Vec2], b: &[Vec2]) -> bool {
    for poly in [a, b] {
        let n = poly.len();
        for i in 0..n {
            let axis = (poly[(i + 1) % n] - poly[i]).perp();
            let (amin, amax) = project(a, axis);
            let (bmin, bmax) = project(b, axis);
            if amax < bmin - 1e-12 || bmax < amin - 1e-12 {
                return false;
            }
        }
    }
    true
}

fn project(poly: &[Vec2], axis: Vec2) -> (f64, f64) {
    poly.iter()
        .map(|p| p.dot(axis))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

/// Logistic function.
pub fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(cx: f64, cy: f64, s: f64) -> Vec<Vec2> {
        Obb::new([cx, cy, 0.0], [s, s, 1.0], 0.0).footprint.to_vec()
    }

    #[test]
    fn footprint_is_ccw_with_correct_area() {
        let b = Obb::new([1.0, 2.0, 0.5], [2.0, 1.0, 1.0], 0.7);
        assert!((polygon_area(&b.footprint) - 2.0).abs() < 1e-12);
        assert!((b.area() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn overlap_of_unit_squares() {
        let a = square(0.0, 0.0, 1.0);
        let b = square(0.5, 0.5, 1.0);
        assert!((intersection_area(&a, &b) - 0.25).abs() < 1e-12);
        assert_eq!(intersection_area(&a, &square(3.0, 0.0, 1.0)), 0.0);
    }

    #[test]
    fn rotated_square_inside_bigger_square() {
        let small = Obb::new([0.0, 0.0, 0.0], [0.5, 0.5, 1.0], 0.6).footprint;
        let big = square(0.0, 0.0, 2.0);
        assert!((intersection_area(&small, &big) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn distances() {
        let a = square(0.0, 0.0, 1.0);
        let b = square(3.0, 0.0, 1.0);
        assert!((polygon_distance(&a, &b) - 2.0).abs() < 1e-12);
        assert_eq!(polygon_distance(&a, &square(0.9, 0.0, 1.0)), 0.0);
        assert!((point_polygon_distance(Vec2::new(0.0, 2.5), &a) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn hull_of_two_squares() {
        let mut pts = square(0.0, 0.0, 1.0);
        pts.extend(square(4.0, 0.0, 1.0));
        let h = convex_hull(&pts);
        assert_eq!(h.len(), 4);
        assert!((polygon_area(&h) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn intervals() {
        let a = Interval { lo: 0.0, hi: 1.0 };
        let b = Interval { lo: 0.5, hi: 2.0 };
        assert_eq!(a.overlap(b), 0.5);
        assert_eq!(a.gap(Interval { lo: 1.5, hi: 2.0 }), 0.5);
    }

    #[test]
    fn front_is_rotated_into_world() {
        let o = SceneObject {
            id: "c".into(),
            label: "chair".into(),
            center: [0.0; 3],
            size: [1.0; 3],
            yaw: std::f64::consts::FRAC_PI_2,
            front: Some([1.0, 0.0, 0.0]),
        };
        let f = Obb::from_object(&o).front_xy.unwrap();
        assert!(f.x.abs() < 1e-12 && (f.y - 1.0).abs() < 1e-12);
    }
}
