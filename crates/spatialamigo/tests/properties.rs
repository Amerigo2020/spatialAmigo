//! Property-based tests of the relation predicates.

use proptest::prelude::*;
use spatialamigo::geometry::Obb;
use spatialamigo::relations::{self, allocentric_frame, egocentric_frame, Params, Relation};
use spatialamigo::{SceneObject, Viewpoint};

const TOL: f64 = 1e-6;
const DIRECTIONS: [Relation; 4] = [Relation::Left, Relation::Right, Relation::Front, Relation::Behind];

#[derive(Debug, Clone)]
struct Obj {
    center: [f64; 3],
    size: [f64; 3],
    yaw: f64,
    front: [f64; 2],
}

fn obj() -> impl Strategy<Value = Obj> {
    (
        (-3.0..3.0f64, -3.0..3.0f64, 0.0..2.0f64),
        (0.05..1.5f64, 0.05..1.5f64, 0.05..1.5f64),
        -3.2..3.2f64,
        0.0..std::f64::consts::TAU,
    )
        .prop_map(|((x, y, z), (sx, sy, sz), yaw, fa)| Obj {
            center: [x, y, z],
            size: [sx, sy, sz],
            yaw,
            front: [fa.cos(), fa.sin()],
        })
}

fn viewpoint() -> impl Strategy<Value = Viewpoint> {
    (
        (-6.0..6.0f64, -6.0..6.0f64, 0.5..2.5f64),
        (-1.0..1.0f64, -1.0..1.0f64),
    )
        .prop_map(|((x, y, z), (lx, ly))| Viewpoint {
            position: [x, y, z],
            look_at: [lx, ly, 0.5],
        })
}

fn to_obb(o: &Obj) -> Obb {
    Obb::from_object(&SceneObject {
        id: "x".into(),
        label: "x".into(),
        center: o.center,
        size: o.size,
        yaw: o.yaw,
        front: Some([o.front[0], o.front[1], 0.0]),
    })
}

/// Rigid motion: rotate by `theta` about z, then translate by `t`.
fn moved(o: &Obj, theta: f64, t: [f64; 3]) -> Obj {
    let (s, c) = theta.sin_cos();
    let [x, y, z] = o.center;
    Obj {
        center: [c * x - s * y + t[0], s * x + c * y + t[1], z + t[2]],
        size: o.size,
        yaw: o.yaw + theta,
        front: o.front,
    }
}

fn moved_vp(v: &Viewpoint, theta: f64, t: [f64; 3]) -> Viewpoint {
    let f = |p: [f64; 3]| {
        let (s, c) = theta.sin_cos();
        [
            c * p[0] - s * p[1] + t[0],
            s * p[0] + c * p[1] + t[1],
            p[2] + t[2],
        ]
    };
    Viewpoint {
        position: f(v.position),
        look_at: f(v.look_at),
    }
}

/// Reflection y -> -y. The object frame's y axis flips, so the front's y component flips too.
fn mirrored(o: &Obj) -> Obj {
    Obj {
        center: [o.center[0], -o.center[1], o.center[2]],
        size: o.size,
        yaw: -o.yaw,
        front: [o.front[0], -o.front[1]],
    }
}

fn mirrored_vp(v: &Viewpoint) -> Viewpoint {
    Viewpoint {
        position: [v.position[0], -v.position[1], v.position[2]],
        look_at: [v.look_at[0], -v.look_at[1], v.look_at[2]],
    }
}

/// All scores of the target `t` w.r.t. anchors `a` (and `b` for between).
fn all_scores(t: &Obj, a: &Obj, b: &Obj, vp: &Viewpoint) -> Vec<(String, f64)> {
    let p = Params::default();
    let (t, a, b) = (to_obb(t), to_obb(a), to_obb(b));
    let mut out = vec![
        ("between".to_string(), relations::between(&t, &a, &b, &p).score),
        ("above".to_string(), relations::above(&t, &a, &p).score),
        ("below".to_string(), relations::below(&t, &a, &p).score),
        ("on".to_string(), relations::on(&t, &a, &p).score),
        ("under".to_string(), relations::under(&t, &a, &p).score),
        (
            "distance".to_string(),
            relations::horizontal_distance(&t, &a, &p)
                .feature("distance_m")
                .unwrap(),
        ),
    ];
    let allo = allocentric_frame(&a).unwrap();
    let ego = egocentric_frame(&a, vp);
    for r in DIRECTIONS {
        out.push((
            format!("allo_{r}"),
            relations::directional(&t, &a, r, &allo, &p).score,
        ));
        out.push((
            format!("ego_{r}"),
            relations::directional(&t, &a, r, &ego, &p).score,
        ));
    }
    out
}

fn assert_close(x: &[(String, f64)], y: &[(String, f64)]) -> Result<(), TestCaseError> {
    for ((n, a), (_, b)) in x.iter().zip(y) {
        prop_assert!((a - b).abs() < TOL, "{n}: {a} vs {b}");
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn scores_are_finite_and_in_unit_interval(t in obj(), a in obj(), b in obj(), vp in viewpoint()) {
        for (n, s) in all_scores(&t, &a, &b, &vp) {
            prop_assert!(s.is_finite(), "{n} not finite");
            if n != "distance" {
                prop_assert!((0.0..=1.0).contains(&s), "{n} = {s}");
            }
        }
    }

    #[test]
    fn translation_invariance(t in obj(), a in obj(), b in obj(), vp in viewpoint(),
                              d in (-50.0..50.0f64, -50.0..50.0f64, -5.0..5.0f64)) {
        let d = [d.0, d.1, d.2];
        let base = all_scores(&t, &a, &b, &vp);
        let shifted = all_scores(&moved(&t, 0.0, d), &moved(&a, 0.0, d), &moved(&b, 0.0, d), &moved_vp(&vp, 0.0, d));
        assert_close(&base, &shifted)?;
    }

    #[test]
    fn rotation_about_z_invariance(t in obj(), a in obj(), b in obj(), vp in viewpoint(), theta in -3.2..3.2f64) {
        let z = [0.0; 3];
        let base = all_scores(&t, &a, &b, &vp);
        let rot = all_scores(&moved(&t, theta, z), &moved(&a, theta, z), &moved(&b, theta, z), &moved_vp(&vp, theta, z));
        assert_close(&base, &rot)?;
    }

    #[test]
    fn mirroring_swaps_left_and_right(t in obj(), a in obj(), b in obj(), vp in viewpoint()) {
        let base = all_scores(&t, &a, &b, &vp);
        let mir = all_scores(&mirrored(&t), &mirrored(&a), &mirrored(&b), &mirrored_vp(&vp));
        let swap = |n: &str| n.replace("left", "TMP").replace("right", "left").replace("TMP", "right");
        for (n, s) in &base {
            let target = swap(n);
            let m = mir.iter().find(|(k, _)| *k == target).unwrap().1;
            prop_assert!((s - m).abs() < TOL, "{n} = {s} but mirrored {target} = {m}");
        }
    }

    #[test]
    fn converse_pairs(t in obj(), a in obj()) {
        let p = Params::default();
        let (t, a) = (to_obb(&t), to_obb(&a));
        prop_assert!((relations::above(&t, &a, &p).score - relations::below(&a, &t, &p).score).abs() < TOL);
        prop_assert!((relations::on(&t, &a, &p).score - relations::under(&a, &t, &p).score).abs() < TOL);
    }

    #[test]
    fn superlative_picks_extremes(ds in proptest::collection::vec(0.0..10.0f64, 1..30)) {
        let p = Params::default();
        let c = relations::superlative_scores(&ds, false, &p);
        let f = relations::superlative_scores(&ds, true, &p);
        let imin = ds.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let imax = ds.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        prop_assert_eq!(c[imin], 1.0);
        prop_assert_eq!(f[imax], 1.0);
        for i in 0..ds.len() {
            for j in 0..ds.len() {
                if ds[i] < ds[j] {
                    prop_assert!(c[i] >= c[j] && f[i] <= f[j]);
                }
            }
        }
    }

    #[test]
    fn opposite_directions_do_not_both_hold(t in obj(), a in obj()) {
        let p = Params::default();
        let (t, a) = (to_obb(&t), to_obb(&a));
        let fr = allocentric_frame(&a).unwrap();
        let l = relations::directional(&t, &a, Relation::Left, &fr, &p).score;
        let r = relations::directional(&t, &a, Relation::Right, &fr, &p).score;
        let f = relations::directional(&t, &a, Relation::Front, &fr, &p).score;
        let b = relations::directional(&t, &a, Relation::Behind, &fr, &p).score;
        prop_assert!(l + r <= 1.0 + TOL, "left {l} + right {r}");
        prop_assert!(f + b <= 1.0 + TOL, "front {f} + behind {b}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The O(1) rectangle routines agree with the generic convex polygon ones.
    #[test]
    fn fast_footprint_routines_match_generic(a in obj(), b in obj(), snap in 0usize..3) {
        let (mut a, b) = (a, b);
        // also exercise the aligned fast path (same yaw or rotated by 90 degrees)
        if snap == 1 { a.yaw = b.yaw; }
        if snap == 2 { a.yaw = b.yaw + std::f64::consts::FRAC_PI_2; }
        let (oa, ob) = (to_obb(&a), to_obb(&b));
        let d_fast = oa.footprint_distance(&ob);
        let d_gen = spatialamigo::geometry::polygon_distance(&oa.footprint, &ob.footprint);
        prop_assert!((d_fast - d_gen).abs() < 1e-9, "distance {d_fast} vs {d_gen}");
        let i_fast = oa.footprint_overlap_area(&ob);
        let i_gen = spatialamigo::geometry::intersection_area(&oa.footprint, &ob.footprint);
        prop_assert!((i_fast - i_gen).abs() < 1e-9, "area {i_fast} vs {i_gen}");
    }
}
