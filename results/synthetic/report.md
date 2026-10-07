Seed 42, 400 samples per relation, 6000 samples in total.

| Family | N | Cand. | Parser | Text to target | Query to target | Random | Nearest-anchor | Margin |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| horizontal_proximity | 800 | 2.98 | 100.0 | 100.0 | 100.0 | 36.5 | 50.0 | 0.96 |
| between | 400 | 3.02 | 100.0 | 100.0 | 100.0 | 35.7 | 79.8 | 0.95 |
| vertical_proximity | 800 | 2.71 | 100.0 | 100.0 | 100.0 | 39.5 | 99.8 | 0.92 |
| support | 800 | 2.66 | 100.0 | 100.0 | 100.0 | 40.1 | 100.0 | 1.00 |
| allocentric | 1600 | 2.80 | 100.0 | 100.0 | 100.0 | 38.7 | 39.5 | 0.62 |
| egocentric | 1600 | 2.84 | 100.0 | 100.0 | 100.0 | 38.1 | 37.7 | 0.64 |
| **all** | 6000 | 2.82 | 100.0 | 100.0 | 100.0 | 38.3 | 59.2 | 0.79 |

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
| allocentric | left | 400 | 100.0 | 100.0 | 100.0 | 38.4 | 46.2 |
| allocentric | right | 400 | 100.0 | 100.0 | 100.0 | 38.7 | 40.0 |
| allocentric | front | 400 | 100.0 | 100.0 | 100.0 | 38.7 | 33.8 |
| allocentric | behind | 400 | 100.0 | 100.0 | 100.0 | 38.9 | 38.0 |
| egocentric | left | 400 | 100.0 | 100.0 | 100.0 | 37.7 | 40.0 |
| egocentric | right | 400 | 100.0 | 100.0 | 100.0 | 38.1 | 37.0 |
| egocentric | front | 400 | 100.0 | 100.0 | 100.0 | 37.9 | 37.2 |
| egocentric | behind | 400 | 100.0 | 100.0 | 100.0 | 38.5 | 36.5 |

### Robustness to pose noise (ground-truth query, resolver only)

| Family | σ=0 cm, 0°, 0% | σ=2 cm, 2°, 5% | σ=5 cm, 5°, 10% | σ=10 cm, 10°, 20% |
| --- | ---: | ---: | ---: | ---: |
| horizontal_proximity | 100.0 | 100.0 | 99.4 | 96.4 |
| between | 100.0 | 100.0 | 100.0 | 99.5 |
| vertical_proximity | 100.0 | 100.0 | 100.0 | 99.2 |
| support | 100.0 | 99.8 | 98.6 | 93.4 |
| allocentric | 100.0 | 100.0 | 99.8 | 99.2 |
| egocentric | 100.0 | 100.0 | 100.0 | 99.9 |
| **all** | 100.0 | 100.0 | 99.7 | 98.3 |

### Held-out paraphrases (template parser)

| Family | N | Parser | Text to target |
| --- | ---: | ---: | ---: |
| horizontal_proximity | 800 | 100.0 | 100.0 |
| between | 400 | 100.0 | 100.0 |
| vertical_proximity | 800 | 86.1 | 86.1 |
| support | 800 | 75.1 | 75.1 |
| allocentric | 1600 | 81.1 | 82.3 |
| egocentric | 1600 | 79.7 | 84.4 |
| **all** | 6000 | 84.4 | 86.0 |

Typical failures:

* "the clock on the wall above the nightstand" parsed as `error: could not parse "the clock on the wall above the nightstand": unknown object "wall" (known: cabinet, clock, nightstand, sofa)`
* "the nightstand that the book is on" parsed as `error: could not parse "the nightstand that the book is on": relation "on" needs an anchor object`
* "the nightstand with the monitor on it" parsed as `error: could not parse "the nightstand with the monitor on it": relation "on" needs an anchor object`
* "the lamp on the armchair's left" parsed as `lamp | on(armchair)`
* "the trash can on the cabinet's right" parsed as `trash can | on(cabinet)`
* "the plant facing the bed" parsed as `error: could not parse "the plant facing the bed": no spatial relation found`
