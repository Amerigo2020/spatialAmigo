//! # spatialAmigo
//!
//! Deterministic, explainable resolution of spatial referring expressions
//! ("the mug left of the laptop") in 3D scenes made of labelled, oriented boxes.
//!
//! The design follows one rule: a language model may translate free text into a
//! structured [`Query`], but the decision itself is made by fast, deterministic
//! geometry that reports *why* it ranked a candidate first.
//!
//! ```
//! use spatialamigo::{Resolver, Scene};
//!
//! let scene = Scene::from_json(include_str!("../tests/fixtures/tabletop.json"))?;
//! let resolver = Resolver::new(scene);
//! let res = resolver.resolve_text("the mug closest to the laptop")?;
//! assert_eq!(res.best.as_deref(), Some("mug_2"));
//! println!("{}", res.explanation);
//! # Ok::<(), spatialamigo::Error>(())
//! ```
//!
//! Modules:
//! * [`scene`]: scene format v0.1 (shared with motionAmigo)
//! * [`geometry`]: oriented boxes and convex polygon routines
//! * [`relations`]: the relation predicates and their parameters
//! * [`query`]: structured queries
//! * [`parser`]: deterministic template parser
//! * [`eval`]: query evaluation with trace

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod eval;
pub mod geometry;
pub mod parser;
pub mod query;
pub mod relations;
pub mod scene;

pub use error::{Error, Result};
pub use eval::{Candidate, ConstraintTrace, Resolution, ResolveOptions, Resolver};
pub use geometry::Obb;
pub use parser::parse;
pub use query::{normalize_label, Constraint, ObjectRef, Query};
pub use relations::{Family, Frame, Params, Relation};
pub use scene::{Scene, SceneObject, Viewpoint, SCENE_FORMAT_VERSION};

/// Schema of the scene format v0.1 (JSON Schema draft 2020-12).
pub const SCENE_SCHEMA: &str = include_str!("../schema/scene-v0.1.json");
/// Schema of the query format v0.1 (JSON Schema draft 2020-12).
pub const QUERY_SCHEMA: &str = include_str!("../schema/query-v0.1.json");
