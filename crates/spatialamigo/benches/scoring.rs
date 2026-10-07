//! Scoring many candidates. The scene layout is mirrored exactly by
//! `bench/compare_numpy.py` so the Rust, Python-binding and NumPy numbers
//! measure the same work.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use spatialamigo::{Query, ResolveOptions, Resolver, Scene, SceneObject};

/// `n` mugs on a grid on a large table, a laptop with a front and a book.
pub fn grid_scene(n: usize) -> Scene {
    let k = (n as f64).sqrt().ceil() as usize;
    let side = k as f64 * 0.15 + 1.0;
    let mut s = Scene::new();
    let obj = |id: String, label: &str, c: [f64; 3], sz: [f64; 3], front: Option<[f64; 3]>| SceneObject {
        id,
        label: label.into(),
        center: c,
        size: sz,
        yaw: 0.0,
        front,
    };
    s.objects.push(obj(
        "table".into(),
        "table",
        [0.0, 0.0, 0.35],
        [side, side, 0.7],
        None,
    ));
    s.objects.push(obj(
        "laptop".into(),
        "laptop",
        [0.07, 0.11, 0.71],
        [0.24, 0.32, 0.02],
        Some([1.0, 0.0, 0.0]),
    ));
    s.objects.push(obj(
        "book".into(),
        "book",
        [-0.4, 0.3, 0.715],
        [0.16, 0.22, 0.03],
        None,
    ));
    for i in 0..n {
        let x = (i % k) as f64 * 0.15 - side / 2.0 + 0.5;
        let y = (i / k) as f64 * 0.15 - side / 2.0 + 0.5;
        s.objects.push(obj(
            format!("mug_{i}"),
            "mug",
            [x, y, 0.75],
            [0.08, 0.08, 0.1],
            None,
        ));
    }
    s
}

fn bench(c: &mut Criterion) {
    let queries: [(&str, &str); 5] = [
        (
            "closest",
            r#"{"target":"mug","constraints":[{"relation":"closest","anchors":["laptop"]}]}"#,
        ),
        (
            "on",
            r#"{"target":"mug","constraints":[{"relation":"on","anchors":["table"]}]}"#,
        ),
        (
            "left",
            r#"{"target":"mug","constraints":[{"relation":"left","anchors":["laptop"]}]}"#,
        ),
        (
            "between",
            r#"{"target":"mug","constraints":[{"relation":"between","anchors":["laptop","book"]}]}"#,
        ),
        (
            "on+left+closest",
            r#"{"target":"mug","constraints":[{"relation":"on","anchors":["table"]},{"relation":"left","anchors":["laptop"]},{"relation":"closest","anchors":["book"]}]}"#,
        ),
    ];
    for (name, q) in queries {
        let q = Query::from_json(q).unwrap();
        let mut g = c.benchmark_group(format!("resolve/{name}"));
        for n in [10usize, 100, 1_000, 10_000] {
            let r = Resolver::new(grid_scene(n));
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
                b.iter(|| black_box(r.resolve(black_box(&q)).unwrap()))
            });
        }
        g.finish();
    }
    let quiet = ResolveOptions { explanation: false };
    let q = Query::from_json(queries[0].1).unwrap();
    let mut g = c.benchmark_group("resolve_no_text/closest");
    for n in [10usize, 100, 1_000, 10_000] {
        let r = Resolver::new(grid_scene(n));
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| black_box(r.resolve_with(black_box(&q), &quiet).unwrap()))
        });
    }
    g.finish();
    let r = Resolver::new(grid_scene(100));
    c.bench_function("parse+resolve text/100", |b| {
        b.iter(|| {
            black_box(
                r.resolve_text(black_box("the mug to the left of the laptop"))
                    .unwrap(),
            )
        })
    });
    c.bench_function("resolver construction/1000", |b| {
        b.iter(|| black_box(Resolver::new(grid_scene(1000))))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30).measurement_time(std::time::Duration::from_secs(2)).warm_up_time(std::time::Duration::from_millis(500));
    targets = bench
}
criterion_main!(benches);
