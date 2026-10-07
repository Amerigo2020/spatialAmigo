| Query | N = 10 | N = 100 | N = 1000 | N = 10000 | per candidate at N = 1000 |
| --- | ---: | ---: | ---: | ---: | ---: |
| closest | 19.3 µs | 70.8 µs | 548.1 µs | 9.84 ms | 548 ns |
| on | 18.8 µs | 65.1 µs | 445.0 µs | 7.39 ms | 445 ns |
| left | 16.1 µs | 71.8 µs | 477.2 µs | 6.60 ms | 477 ns |
| between | 22.3 µs | 134.1 µs | 1.11 ms | 16.41 ms | 1109 ns |
| on AND left AND closest | 39.7 µs | 153.4 µs | 1.14 ms | 26.06 ms | 1138 ns |
| closest, no text explanation | 3.9 µs | 55.0 µs | 503.6 µs | 9.28 ms | 504 ns |

Criterion medians, full resolution including the per-candidate trace. Intel(R) Xeon(R) Processor @ 2.80GHz, Linux.
