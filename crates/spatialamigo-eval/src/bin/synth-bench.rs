//! Synthetic benchmark runner.
//!
//! ```text
//! cargo run --release -p spatialamigo-eval --bin synth-bench -- --seed 42 --per-task 400 --out results/synthetic
//! ```

use std::path::PathBuf;

use spatialamigo_eval::{bench, synth};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = 42u64;
    let mut per_task = 400usize;
    let mut out: Option<PathBuf> = None;
    let mut dump_scenes = 0usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seed" => seed = args.next().ok_or("--seed needs a value")?.parse()?,
            "--per-task" => per_task = args.next().ok_or("--per-task needs a value")?.parse()?,
            "--out" => out = Some(args.next().ok_or("--out needs a value")?.into()),
            "--dump-scenes" => dump_scenes = args.next().ok_or("--dump-scenes needs a value")?.parse()?,
            "-h" | "--help" => {
                println!("synth-bench [--seed N] [--per-task N] [--out DIR] [--dump-scenes N]");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let t0 = std::time::Instant::now();
    let samples = synth::generate(seed, per_task);
    let t_gen = t0.elapsed();
    let t1 = std::time::Instant::now();
    let (report, results) = bench::run(&samples, seed, per_task);
    let t_eval = t1.elapsed();
    let noise = bench::noise_study(&samples, seed);
    let para = bench::paraphrase_study(&samples, seed);
    let md = format!(
        "{}\n### Robustness to pose noise (ground-truth query, resolver only)\n\n{}\n### Held-out paraphrases (template parser)\n\n{}",
        bench::markdown(&report),
        bench::noise_markdown(&noise),
        bench::paraphrase_markdown(&para)
    );
    println!("{md}");
    println!(
        "generated {} samples in {:.2?}, evaluated in {:.2?} ({:.1} us per sample incl. parsing and resolver setup)",
        samples.len(),
        t_gen,
        t_eval,
        t_eval.as_secs_f64() * 1e6 / samples.len().max(1) as f64
    );
    if let Some(dir) = out {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("report.md"), &md)?;
        let json = serde_json::json!({"report": report, "noise": noise, "paraphrases": para});
        std::fs::write(dir.join("report.json"), serde_json::to_string_pretty(&json)?)?;
        let lines: Vec<String> = results
            .iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect();
        std::fs::write(dir.join("samples.jsonl"), lines.join("\n") + "\n")?;
        if let Some(step) = samples.len().checked_div(dump_scenes) {
            let sd = dir.join("scenes");
            std::fs::create_dir_all(&sd)?;
            for s in samples.iter().step_by(step.max(1)).take(dump_scenes) {
                let name = format!("{:04}_{}_{}.json", s.index, s.family, s.relation);
                let payload = serde_json::json!({"sentence": s.sentence, "target_id": s.target_id, "query": s.query, "scene": s.scene});
                std::fs::write(sd.join(name), serde_json::to_string_pretty(&payload)?)?;
            }
        }
        println!("wrote {}", dir.display());
    }
    Ok(())
}
