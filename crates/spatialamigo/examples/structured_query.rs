//! Build a structured query in code (no parser involved) and inspect the trace,
//! the way an LLM-produced JSON query would be evaluated.
//!
//! ```text
//! cargo run -p spatialamigo --example structured_query
//! ```

use spatialamigo::{Constraint, Frame, Query, Relation, Resolver, Scene};

fn main() -> Result<(), spatialamigo::Error> {
    let scene = Scene::from_json(include_str!("../tests/fixtures/tabletop.json"))?;
    let resolver = Resolver::new(scene);

    // "the mug that is on the table and, seen from the chair, on its left"
    let query = Query::new("mug")
        .and(Constraint::new(Relation::On, "table"))
        .and(Constraint::new(Relation::Left, "chair").with_frame(Frame::Allocentric));
    println!("query JSON: {}", query.to_json());

    // The same query as it would arrive from an LLM via structured output.
    let from_llm = Query::from_json(&query.to_json())?;
    assert_eq!(from_llm, query);

    let res = resolver.resolve(&from_llm)?;
    for c in &res.candidates {
        println!("#{} {} score={:.3}", c.rank, c.id, c.score);
        for t in &c.trace {
            let feats: Vec<String> = t
                .features
                .iter()
                .map(|f| format!("{}={:.3}", f.name, f.value))
                .collect();
            println!(
                "    {} {:?} -> {:.3} [{}]",
                t.relation,
                t.anchors,
                t.score,
                feats.join(", ")
            );
        }
    }
    println!("best: {:?}", res.best);
    Ok(())
}
