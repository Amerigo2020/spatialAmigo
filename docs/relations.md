# Relation definitions

spatialAmigo scores every candidate of the target class with continuous predicates in `[0, 1]`. Every predicate also returns its intermediate quantities (features) so a result can be audited. All formulas below are implemented in [`crates/spatialamigo/src/relations.rs`](../crates/spatialamigo/src/relations.rs), the default constants live in `Params::default()`.

The relation families follow **Sr3D** from ReferIt3D (Achlioptas et al., ECCV 2020, main paper Sec. 3.1 and supplementary material Sec. 2). Sr3D defines crisp rules for generating unambiguous utterances. spatialAmigo keeps the geometric meaning of those rules but turns every threshold into a smooth function so that candidates can be ranked and combined. The egocentric family is not part of Sr3D and was added for robotics use with a camera viewpoint.

## Notation

| Symbol | Meaning |
| --- | --- |
| `T`, `A`, `B` | target box, anchor box, second anchor box |
| `F(X)` | footprint of box `X`: the oriented rectangle in the XY plane (exact, uses yaw) |
| `|F|` | area of a footprint |
| `F(X) ∩ F(Y)` | exact intersection polygon (Sutherland-Hodgman clipping of convex polygons) |
| `gap(P, Q)` | minimum Euclidean distance between two convex polygons, 0 if they touch or overlap |
| `c(X)` | center of `X` projected to XY |
| `Z(X) = [z_lo, z_hi]` | vertical extent |
| `G(x, σ) = exp(-(x/σ)²)` | Gaussian fall-off |
| `σ(x) = 1 / (1 + exp(-x))` | logistic function |

Soft XY overlap term, used where Sr3D requires footprints to overlap. It is continuous and still ranks objects that miss the overlap by a few centimeters:

```
xy(frac, g) = 0.5 * frac + 0.5 * G(g, σ_xy)          σ_xy = 0.15 m
```

## 1. Horizontal proximity: `closest`, `farthest`

Sr3D: for an anchor and a target class, compute the distance of every same-class object to the anchor; the closest (farthest) one is used if it beats the runner-up by a margin.

spatialAmigo measures distance in the top view, as Sr3D does ("distance on the horizontal placement of the objects"):

```
d(T, A) = gap(F(T), F(A)) + λ * |c(T) - c(A)|          λ = 0.1
```

The footprint gap makes a mug 5 cm from the edge of a large table "close" even though the table center is far away. The small center term breaks ties when several footprints overlap the anchor.

These relations are **superlatives**: the score of a candidate depends on all candidates `i` of the target class.

```
closest_i  = exp(-(d_i - min_j d_j) / τ)
farthest_i = exp(-(max_j d_j - d_i) / τ)              τ = 0.15 m
```

The best candidate always scores exactly 1. A runner-up 10 cm behind scores 0.51, 30 cm behind 0.14. The Sr3D margin (epsilonGap) shows up as the score margin between rank 1 and rank 2 in the result.

## 2. Between (two anchors)

Sr3D: compute the convex hull of the two anchors' top-view boxes. The target (a) does not intersect the anchors, (b) lies inside the hull, (c) overlaps both anchors along z.

```
H            = convex_hull(F(A) ∪ F(B))
inside       = |F(T) ∩ H| / |F(T)|
containment  = 0.5 * inside + 0.5 * G(dist(c(T), H), σ_xy)
penetration  = max over X in {A, B} of  (|F(T) ∩ F(X)| / min(|F(T)|, |F(X)|)) * (overlap_z(T, X) / min(h_T, h_X))
separation   = 1 - penetration
z_X          = G(gap_z(T, X), σ_z)                  σ_z = 0.15 m
between      = containment * separation * z_A * z_B
```

## 3. Vertical proximity: `above`, `below`

Sr3D: like support, but the objects must not touch. The footprints must overlap.

```
frac   = |F(T) ∩ F(A)| / min(|F(T)|, |F(A)|)
g_z    = z_lo(T) - z_hi(A)                     (vertical clearance)
clear  = σ((g_z - δ) / s)                      δ = 0.03 m, s = 0.01 m
above(T, A) = xy(frac, gap(F(T), F(A))) * clear
below(T, A) = above(A, T)
```

Objects that touch (`g_z ≈ 0`) score below 0.05 on `above`, so a mug standing on a table is `on` the table, not `above` it, exactly as in Sr3D.

## 4. Support: `on`, `under`

Sr3D: the target's bottom face touches the anchor's top face (small z difference) and the top-view boxes overlap by more than a threshold relative to the object's area. The converse relation ("supporting") is the target holding the anchor.

```
support   = |F(T) ∩ F(A)| / |F(T)|            (how much of the target is carried)
g_z       = z_lo(T) - z_hi(A)
contact   = G(g_z, σ_c)                        σ_c = 0.03 m
on(T, A)  = xy(support, gap(F(T), F(A))) * contact
under(T, A) = on(A, T)                         (target supports the anchor)
```

Note on wording: spatialAmigo's `under` is the support relation ("the table under the laptop"). Non-touching "below" is the vertical relation `below`. The template parser maps "under", "underneath", "supporting" and "holding" to `under` and "below" and "beneath" to `below`.

## 5. Directional relations: `left`, `right`, `front`, `behind`

Both directional families share one scoring function and differ only in the reference frame `(f, l)`, two orthogonal unit vectors in the XY plane that say where "front" and "left" are.

**Allocentric** (Sr3D): relative to the intrinsic front of the anchor. The anchor needs a `front` vector. `f` is that front rotated into the world, and `l = z × f` is the anchor's own left. A chair whose seat faces `-x` has its left at `-y`.

**Egocentric** (extension): relative to the observer in `scene.viewpoint`. With `s` the unit direction from the observer to the anchor (top view), `f = -s` (front means towards the observer) and `l = z × s` (the observer's left while looking at the anchor). Using the line of sight to each anchor instead of the global look direction keeps "left" correct for anchors at the edge of a wide field of view.

**Auto** (default in queries): allocentric if the anchor has a front, otherwise egocentric if the scene has a viewpoint. The trace records which frame was used.

Sr3D splits the space around the anchor into four oriented sections (front, back, left, right) and requires the target to lie in the right one. spatialAmigo bounds the sections by the 45° lines through the corners of the anchor footprint: a point belongs to the side whose face it lies farther outside of. Sections bounded by the box diagonals would make "front" of a thin TV cover almost the whole half plane, so "the plant to the right of the TV" would fail for any plant not exactly beside the screen.

```
r      = c(T) - c(A)
u      = r · f,   w = r · l                       (metric coordinates in the frame)
e_f    = |u| - h_f(A),   e_l = |w| - h_l(A)       (how far outside the front/back and side faces)
lat    = (e_l - e_f) / (|e_l| + |e_f| + ε)        ∈ (-1, 1): +1 beside, -1 in front or behind
left   = σ( lat / k) · σ( w / ε)
right  = σ( lat / k) · σ(-w / ε)
front  = σ(-lat / k) · σ( u / ε)
behind = σ(-lat / k) · σ(-u / ε)                  k = 0.15, ε = 0.05 m
falloff = 1 / (1 + gap(F(T), F(A)) / d₀)          d₀ = 2.0 m
directional = section * falloff
```

`h_f` and `h_l` are the half extents of the anchor footprint along `f` and `l` (support function, exact for any yaw). The fall-off prefers the nearer object when two candidates fall into the same section, which matches how people use "the mug left of the laptop". Opposite relations exclude each other: `left + right ≤ 1` and `front + behind ≤ 1` hold for every configuration (property test).

## Combining constraints

A query is a conjunction (AND) of constraints. The score of a candidate is the product of its constraint scores:

```
score(T) = Π_k  max over anchors X_k matching constraint k  of  rel_k(T, X_k)
```

If an anchor label matches several objects, the best anchor is used (existential reading, "next to a chair"). Superlatives are evaluated over the full candidate set of the target class (including candidates restricted by the query) before the product is taken.

The product keeps the result interpretable: every factor is visible in the trace, a single failing constraint drives the score to zero, and a candidate cannot compensate a violated relation by excelling at another one.

## Properties guaranteed by tests

Checked with `proptest` in [`crates/spatialamigo/tests/properties.rs`](../crates/spatialamigo/tests/properties.rs):

* all scores are finite and lie in `[0, 1]`
* invariance under translation and rotation about z of the whole scene (including the viewpoint)
* mirroring the scene (y → -y) swaps `left` and `right` in both frames and leaves all other scores unchanged
* `above(T, A) = below(A, T)` and `on(T, A) = under(A, T)`
* superlatives give 1 exactly to the extreme candidate and are monotone in distance
* opposite directions exclude each other

## Differences to Sr3D at a glance

| Aspect | Sr3D (dataset generation) | spatialAmigo |
| --- | --- | --- |
| Output | crisp yes/no, used to synthesize unambiguous sentences | continuous score per candidate with features |
| Thresholds | hard (epsilonGap, occupancy, overlap ratios) | logistic or Gaussian versions of the same thresholds |
| Geometry | point clouds and axis-aligned boxes, Scan2CAD orientation for allocentric | oriented boxes with yaw, exact footprint polygons |
| Allocentric sections | ratio of target points inside a section | target center in sections bounded by 45° lines through the footprint corners, softened |
| Egocentric | not part of Sr3D | added, relative to `scene.viewpoint` |
