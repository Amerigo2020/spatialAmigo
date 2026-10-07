# Synthetic benchmark

Reproduce with

```bash
cargo run --release -p spatialamigo-eval --bin synth-bench -- --seed 42 --per-task 400 --out results/synthetic
```

The run takes well under a second (6000 samples generated in about 55 ms and evaluated in about 0.26 s including parsing and resolver construction per sample, on a 2 vCPU cloud VM).

## How samples are built

* A 5 m x 5 m room is filled with objects from a catalog of 23 indoor classes (furniture, small items on surfaces, wall-mounted objects) with size jitter of ±15 % and random yaw. Collisions are rejected.
* For every relation the target is **placed constructively** so that the relation holds (a mug on a desk, a box in the left section of a sofa, a picture above a bed). One to three **distractors of the same class** are placed so that the relation does not hold, including hard negatives: a mug on a wall shelf hanging over the same desk, the same item on the floor right next to it, a picture above a different piece of furniture.
* A **crisp oracle** ([`oracle.rs`](../crates/spatialamigo-eval/src/oracle.rs)) re-implements the Sr3D generation rules with hard thresholds (epsilonGap 0.15 m for closest/farthest, overlap ratio 0.5, contact within 2 cm, clearance at least 5 cm, hull membership, corner-bounded sections). A sample is kept only if the oracle confirms that the target satisfies the relation and no distractor does. The oracle shares the polygon routines with the library but none of the scoring code.
* Ids within a label are shuffled so the referred object is not systematically `_1` (ids are the resolver's last tie breaker).
* The sentence comes from 2 to 5 Sr3D-like templates per relation. Egocentric samples get a viewpoint on a circle around the room and, in 60 % of cases, a phrase like "from my perspective".

## Metrics

| Column | Meaning |
| --- | --- |
| Cand. | mean number of objects of the target class (target plus distractors) |
| Parser | template parser output equals the ground-truth query |
| Text to target | end to end: sentence, template parser, resolver, top-1 equals target |
| Query to target | resolver only, fed the ground-truth query |
| Random | expected accuracy of picking a random candidate of the target class |
| Nearest-anchor | baseline that ignores the relation and picks the candidate whose footprint is closest to the anchor |
| Margin | mean score difference between rank 1 and rank 2 |

## Results

Seed 42, 400 samples per relation, 6000 samples in total.

| Family | N | Cand. | Parser | Text to target | Query to target | Random | Nearest-anchor | Margin |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| horizontal_proximity | 800 | 2.98 | 100.0 | 100.0 | 100.0 | 36.5 | 50.0 | 0.96 |
| between | 400 | 3.02 | 100.0 | 100.0 | 100.0 | 35.7 | 79.8 | 0.95 |
| vertical_proximity | 800 | 2.71 | 100.0 | 100.0 | 100.0 | 39.5 | 99.8 | 0.92 |
| support | 800 | 2.66 | 100.0 | 100.0 | 100.0 | 40.1 | 100.0 | 1.00 |
| allocentric | 1600 | 2.82 | 100.0 | 100.0 | 100.0 | 38.4 | 38.7 | 0.77 |
| egocentric | 1600 | 2.83 | 100.0 | 100.0 | 100.0 | 38.2 | 37.6 | 0.78 |
| **all** | 6000 | 2.82 | 100.0 | 100.0 | 100.0 | 38.3 | 59.0 | 0.86 |

Per relation:

| Family | Relation | N | Parser | Text to target | Query to target | Random | Nearest-anchor |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| horizontal_proximity | closest | 400 | 100.0 | 100.0 | 100.0 | 36.1 | 100.0 |
| horizontal_proximity | farthest | 400 | 100.0 | 100.0 | 100.0 | 36.8 | 0.0 |
| between | between | 400 | 100.0 | 100.0 | 100.0 | 35.7 | 79.8 |
| vertical_proximity | above | 400 | 100.0 | 100.0 | 100.0 | 42.6 | 99.5 |
| vertical_proximity | below | 400 | 100.0 | 100.0 | 100.0 | 36.5 | 100.0 |
| support | on | 400 | 100.0 | 100.0 | 100.0 | 37.9 | 100.0 |
| support | under | 400 | 100.0 | 100.0 | 100.0 | 42.3 | 100.0 |
| allocentric | left | 400 | 100.0 | 100.0 | 100.0 | 37.7 | 42.0 |
| allocentric | right | 400 | 100.0 | 100.0 | 100.0 | 38.3 | 43.8 |
| allocentric | front | 400 | 100.0 | 100.0 | 100.0 | 38.6 | 33.2 |
| allocentric | behind | 400 | 100.0 | 100.0 | 100.0 | 38.9 | 35.8 |
| egocentric | left | 400 | 100.0 | 100.0 | 100.0 | 38.2 | 38.5 |
| egocentric | right | 400 | 100.0 | 100.0 | 100.0 | 37.9 | 37.2 |
| egocentric | front | 400 | 100.0 | 100.0 | 100.0 | 37.8 | 37.5 |
| egocentric | behind | 400 | 100.0 | 100.0 | 100.0 | 38.7 | 37.0 |

### Robustness to pose noise (ground-truth query, resolver only)

| Family | σ=0 cm, 0°, 0% | σ=2 cm, 2°, 5% | σ=5 cm, 5°, 10% | σ=10 cm, 10°, 20% |
| --- | ---: | ---: | ---: | ---: |
| horizontal_proximity | 100.0 | 100.0 | 99.4 | 96.4 |
| between | 100.0 | 100.0 | 100.0 | 99.5 |
| vertical_proximity | 100.0 | 100.0 | 100.0 | 99.2 |
| support | 100.0 | 99.8 | 98.6 | 93.4 |
| allocentric | 100.0 | 100.0 | 99.9 | 99.4 |
| egocentric | 100.0 | 100.0 | 100.0 | 99.9 |
| **all** | 100.0 | 100.0 | 99.7 | 98.3 |

### Held-out paraphrases (template parser)

| Family | N | Parser | Text to target |
| --- | ---: | ---: | ---: |
| horizontal_proximity | 800 | 100.0 | 100.0 |
| between | 400 | 100.0 | 100.0 |
| vertical_proximity | 800 | 86.1 | 86.1 |
| support | 800 | 75.1 | 75.1 |
| allocentric | 1600 | 81.1 | 81.7 |
| egocentric | 1600 | 79.7 | 85.0 |
| **all** | 6000 | 84.4 | 86.0 |

Typical failures:

* "the clock on the wall above the nightstand" parsed as `error: could not parse "the clock on the wall above the nightstand": unknown object "wall" (known: cabinet, clock, nightstand, sofa)`
* "the nightstand that the book is on" parsed as `error: could not parse "the nightstand that the book is on": relation "on" needs an anchor object`
* "the nightstand with the monitor on it" parsed as `error: could not parse "the nightstand with the monitor on it": relation "on" needs an anchor object`
* "the lamp on the armchair's left" parsed as `lamp | on(armchair)`
* "the chair on the cabinet's right" parsed as `chair | on(cabinet)`
* "the box facing the chair" parsed as `error: could not parse "the box facing the chair": no spatial relation found`

## Reading the numbers

* **Clean scenes are a consistency check, not a challenge.** 100 % means that the continuous scores rank the objects exactly like the crisp Sr3D rules. Because the scenes are generated to be unambiguous under those rules, a correct implementation must reach 100 %. The value of this table is that it would catch regressions in any relation and that it separates the parser from the geometry.
* **Baselines show which families need relational reasoning.** "Nearest to the anchor" solves closest, support and vertical relations in these scenes, but fails on farthest (0 %), between (80 %) and on all directional relations (33 to 44 %, close to random). Left/right/front/behind are where an explicit frame model matters.
* **Pose noise** emulates perception errors (Gaussian noise on position, yaw and size of every object, including anchors). Up to 5 cm and 5° the resolver stays above 98.6 % in every family. At 10 cm, 10° and 20 % size noise, support drops to 93 % because the contact term (σ_c = 3 cm) starts to reject objects whose bottom face no longer touches the support, and closest/farthest to 96 % where the 15 cm margin is eaten by noise. A larger `contact_sigma` trades robustness for discrimination between "on" and "above".
* **Held-out paraphrases** were written after the parser and never used to tune it. The template parser keeps 84 % exact parses and 86 % end-to-end accuracy. Typical failures are constructions outside its grammar (relative clauses like "the table that the book is on", possessives like "on the chair's left", unknown landmarks like "the wall", verbs like "facing"). This is the gap the optional LLM parser closes without giving up deterministic resolution.
