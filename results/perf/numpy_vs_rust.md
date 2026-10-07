| Relation | N | spatialAmigo via Python (µs) | same, no text explanation (µs) | NumPy reference (µs) | max abs. score diff |
| --- | ---: | ---: | ---: | ---: | ---: |
| closest | 10 | 27 | 11 | 19 | 2.2e-16 |
| closest | 100 | 94 | 78 | 36 | 2.2e-16 |
| closest | 1000 | 779 | 772 | 114 | 2.2e-16 |
| closest | 10000 | 17708 | 18327 | 917 | 2.2e-16 |
| on | 10 | 27 | 10 | 44 | 5.6e-16 |
| on | 100 | 90 | 70 | 65 | 5.6e-16 |
| on | 1000 | 749 | 685 | 195 | 5.6e-16 |
| on | 10000 | 12912 | 13018 | 1500 | 5.6e-16 |
| left | 10 | 24 | 11 | 31 | 8.7e-19 |
| left | 100 | 92 | 73 | 42 | 1.1e-16 |
| left | 1000 | 755 | 782 | 114 | 2.5e-16 |
| left | 10000 | 12945 | 12375 | 827 | 2.2e-16 |

Median wall time per call. Python 3.13.16, NumPy 2.5.3, spatialAmigo 0.1.0, x86_64 Linux.
