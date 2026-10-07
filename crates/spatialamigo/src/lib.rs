//! # spatialAmigo
//!
//! Deterministic, explainable resolution of spatial referring expressions
//! ("the mug left of the laptop") in 3D scenes made of labelled, oriented boxes.
//!
//! The design follows one rule: a language model may translate free text into a
//! structured query, but the decision itself is made by fast, deterministic
//! geometry that reports *why* it ranked a candidate first.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod geometry;
pub mod relations;
pub mod scene;

pub use error::{Error, Result};
pub use geometry::Obb;
pub use relations::{Family, Frame, Params, Relation};
pub use scene::{Scene, SceneObject, Viewpoint, SCENE_FORMAT_VERSION};
