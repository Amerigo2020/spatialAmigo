//! Resolve referring expressions in a scene and print the explanation trace.
//!
//! ```text
//! cargo run -p spatialamigo --example resolve
//! cargo run -p spatialamigo --example resolve -- examples/scenes/tabletop.json "the mug left of the chair"
//! ```

use spatialamigo::{Resolver, Scene};

const DEFAULT_SCENE: &str = include_str!("../tests/fixtures/tabletop.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (scene_json, sentences) = match args.as_slice() {
        [] => (
            DEFAULT_SCENE.to_string(),
            vec![
                "the mug closest to the laptop".to_string(),
                "the mug left of the laptop".to_string(),
                "the mug to the left of the chair".to_string(),
                "the mug to the left of the chair from my perspective".to_string(),
                "the book on the table".to_string(),
                "the mug on the table and to the right of the laptop".to_string(),
            ],
        ),
        [path, rest @ ..] => (std::fs::read_to_string(path)?, rest.to_vec()),
    };
    let resolver = Resolver::new(Scene::from_json(&scene_json)?);
    for s in sentences {
        println!("> {s}");
        match resolver.resolve_text(&s) {
            Ok(res) => println!("{}\n", res.explanation),
            Err(e) => println!("error: {e}\n"),
        }
    }
    Ok(())
}
