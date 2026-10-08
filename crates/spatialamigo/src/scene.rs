//! Scene format v0.1, shared with motionAmigo.
//!
//! A scene is a list of labelled, oriented 3D boxes in a right-handed world
//! frame with z pointing up (meters, radians). See `schema/scene-v0.1.json`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The only format version this crate reads and writes.
pub const SCENE_FORMAT_VERSION: &str = "0.1";

/// Tolerance for checking that `front` is a unit vector.
const UNIT_TOLERANCE: f64 = 1e-3;

/// A full scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Format version, always `"0.1"`.
    pub version: String,
    /// Frame name, always `"world"` in v0.1.
    pub frame: String,
    /// Objects in the scene.
    pub objects: Vec<SceneObject>,
    /// Optional observer pose for egocentric relations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewpoint: Option<Viewpoint>,
}

/// One labelled, oriented box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneObject {
    /// Unique id within the scene.
    pub id: String,
    /// Semantic class such as `"mug"`.
    pub label: String,
    /// Box center in the world frame (m).
    pub center: [f64; 3],
    /// Full edge lengths along the object's x, y, z axes (m).
    pub size: [f64; 3],
    /// Rotation about world z (rad).
    pub yaw: f64,
    /// Optional intrinsic front direction as a unit vector in the object frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<[f64; 3]>,
}

/// Observer pose used by egocentric relations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewpoint {
    /// Observer position (m).
    pub position: [f64; 3],
    /// Point the observer looks at (m).
    pub look_at: [f64; 3],
}

impl Scene {
    /// Creates an empty v0.1 scene in the world frame.
    pub fn new() -> Self {
        Scene {
            version: SCENE_FORMAT_VERSION.to_string(),
            frame: "world".to_string(),
            objects: Vec::new(),
            viewpoint: None,
        }
    }

    /// Parses and validates a scene from JSON.
    ///
    /// ```
    /// let json = r#"{"version":"0.1","frame":"world","objects":[
    ///   {"id":"t","label":"table","center":[0,0,0.4],"size":[1,1,0.8],"yaw":0}]}"#;
    /// let scene = spatialamigo::Scene::from_json(json).unwrap();
    /// assert_eq!(scene.objects.len(), 1);
    /// ```
    pub fn from_json(json: &str) -> Result<Self> {
        let scene: Scene = serde_json::from_str(json)?;
        scene.validate()?;
        Ok(scene)
    }

    /// Serializes the scene to pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("scene serialization cannot fail")
    }

    /// Checks the format rules that JSON deserialization alone cannot express.
    pub fn validate(&self) -> Result<()> {
        if self.version != SCENE_FORMAT_VERSION {
            return Err(Error::InvalidScene(format!(
                "unsupported version \"{}\", expected \"{SCENE_FORMAT_VERSION}\"",
                self.version
            )));
        }
        if self.frame != "world" {
            return Err(Error::InvalidScene(format!(
                "unsupported frame \"{}\", v0.1 only knows \"world\"",
                self.frame
            )));
        }
        let mut ids = HashSet::new();
        for o in &self.objects {
            if o.id.is_empty() || o.label.trim().is_empty() {
                return Err(Error::InvalidScene(
                    "objects need a non-empty id and label".into(),
                ));
            }
            if !ids.insert(o.id.as_str()) {
                return Err(Error::InvalidScene(format!("duplicate object id \"{}\"", o.id)));
            }
            let finite = o.center.iter().chain(o.size.iter()).all(|v| v.is_finite()) && o.yaw.is_finite();
            if !finite {
                return Err(Error::InvalidScene(format!(
                    "object \"{}\" has non-finite values",
                    o.id
                )));
            }
            if o.size.iter().any(|&s| s <= 0.0) {
                return Err(Error::InvalidScene(format!(
                    "object \"{}\" needs strictly positive size",
                    o.id
                )));
            }
            if let Some(f) = o.front {
                let n = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
                if (n - 1.0).abs() > UNIT_TOLERANCE {
                    return Err(Error::InvalidScene(format!(
                        "object \"{}\" has a front vector of length {n:.4}, expected a unit vector",
                        o.id
                    )));
                }
            }
        }
        if let Some(vp) = &self.viewpoint {
            let all = vp.position.iter().chain(vp.look_at.iter());
            if !all.clone().all(|v| v.is_finite()) {
                return Err(Error::InvalidScene("viewpoint has non-finite values".into()));
            }
            let d = [vp.look_at[0] - vp.position[0], vp.look_at[1] - vp.position[1]];
            if d[0].hypot(d[1]) < 1e-9 {
                return Err(Error::InvalidScene(
                    "viewpoint must not look straight up or down (look_at - position needs a horizontal component)".into(),
                ));
            }
        }
        Ok(())
    }

    /// Looks up an object by id.
    pub fn get(&self, id: &str) -> Option<&SceneObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// Sorted, de-duplicated list of labels in the scene.
    pub fn labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = self.objects.iter().map(|o| o.label.clone()).collect();
        labels.sort();
        labels.dedup();
        labels
    }
}

impl Default for Scene {
    fn default() -> Self {
        Scene::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLETOP: &str = include_str!("../tests/fixtures/tabletop.json");

    #[test]
    fn loads_tabletop_example() {
        let s = Scene::from_json(TABLETOP).unwrap();
        assert_eq!(s.objects.len(), 7);
        assert!(s.viewpoint.is_some());
        assert_eq!(s.get("chair_1").unwrap().front, Some([-1.0, 0.0, 0.0]));
        assert_eq!(
            s.labels(),
            vec!["book", "bowl", "chair", "laptop", "mug", "table"]
        );
    }

    #[test]
    fn round_trips() {
        let s = Scene::from_json(TABLETOP).unwrap();
        let s2 = Scene::from_json(&s.to_json()).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn rejects_bad_version_and_duplicates() {
        let bad = TABLETOP.replace("\"0.1\"", "\"0.2\"");
        assert!(matches!(Scene::from_json(&bad), Err(Error::InvalidScene(_))));
        let dup = TABLETOP.replace("mug_2", "mug_1");
        assert!(matches!(Scene::from_json(&dup), Err(Error::InvalidScene(_))));
    }

    #[test]
    fn rejects_non_unit_front_and_zero_size() {
        let bad = TABLETOP.replace("[-1.0, 0.0, 0.0]", "[-2.0, 0.0, 0.0]");
        assert!(Scene::from_json(&bad).is_err());
        let zero = TABLETOP.replace("[0.08, 0.08, 0.1]", "[0.0, 0.08, 0.1]");
        assert!(Scene::from_json(&zero).is_err());
    }
}
