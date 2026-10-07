//! Structured queries: a target class plus a conjunction of spatial constraints.
//!
//! The JSON form is what the template parser, the optional LLM parser and the
//! Sr3D adapter produce, see `schema/query-v0.1.json`:
//!
//! ```json
//! {"target": "mug",
//!  "constraints": [{"relation": "left", "anchors": ["laptop"], "frame": "allocentric"}]}
//! ```

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};
use crate::relations::{Frame, Relation};

/// Reference to anchor objects, either by class label or by object id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ObjectRef {
    /// All objects with this label (normalized comparison).
    Label(String),
    /// Exactly the object with this id.
    Id(String),
}

impl ObjectRef {
    /// Reference by label.
    pub fn label(s: impl Into<String>) -> Self {
        ObjectRef::Label(s.into())
    }
    /// Reference by id.
    pub fn id(s: impl Into<String>) -> Self {
        ObjectRef::Id(s.into())
    }
}

impl fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectRef::Label(l) => f.write_str(l),
            ObjectRef::Id(i) => write!(f, "#{i}"),
        }
    }
}

impl Serialize for ObjectRef {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            ObjectRef::Label(l) => s.serialize_str(l),
            ObjectRef::Id(i) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("id", i)?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ObjectRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Str(String),
            Id { id: String },
            Label { label: String },
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Str(s) | Raw::Label { label: s } => ObjectRef::Label(s),
            Raw::Id { id } => ObjectRef::Id(id),
        })
    }
}

/// One spatial constraint on the target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    /// The relation.
    pub relation: Relation,
    /// One anchor, or two for `between`.
    pub anchors: Vec<ObjectRef>,
    /// Reference frame for directional relations (ignored otherwise).
    #[serde(default, skip_serializing_if = "is_auto")]
    pub frame: Frame,
}

fn is_auto(f: &Frame) -> bool {
    *f == Frame::Auto
}

impl Constraint {
    /// Constraint with a single anchor label and automatic frame.
    pub fn new(relation: Relation, anchor: impl Into<String>) -> Self {
        Constraint {
            relation,
            anchors: vec![ObjectRef::label(anchor)],
            frame: Frame::Auto,
        }
    }

    /// `between` constraint with two anchor labels.
    pub fn between(a: impl Into<String>, b: impl Into<String>) -> Self {
        Constraint {
            relation: Relation::Between,
            anchors: vec![ObjectRef::label(a), ObjectRef::label(b)],
            frame: Frame::Auto,
        }
    }

    /// Sets the frame.
    pub fn with_frame(mut self, frame: Frame) -> Self {
        self.frame = frame;
        self
    }
}

/// A structured spatial query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    /// Class label of the object we are looking for.
    pub target: String,
    /// Conjunction of constraints. Empty means "any object of the target class".
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    /// Optional restriction of the candidate set to these object ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

impl Query {
    /// Query for a target class without constraints.
    pub fn new(target: impl Into<String>) -> Self {
        Query {
            target: target.into(),
            constraints: Vec::new(),
            candidates: None,
        }
    }

    /// Adds a constraint (builder style).
    ///
    /// ```
    /// use spatialamigo::{Query, Constraint, Relation};
    /// let q = Query::new("mug").and(Constraint::new(Relation::Left, "laptop"));
    /// assert_eq!(q.to_json(), r#"{"target":"mug","constraints":[{"relation":"left","anchors":["laptop"]}]}"#);
    /// ```
    pub fn and(mut self, c: Constraint) -> Self {
        self.constraints.push(c);
        self
    }

    /// Parses and validates a query from JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        let q: Query = serde_json::from_str(json)?;
        q.validate()?;
        Ok(q)
    }

    /// Compact JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("query serialization cannot fail")
    }

    /// Structural checks (non-empty target, correct number of anchors).
    pub fn validate(&self) -> Result<()> {
        if normalize_label(&self.target).is_empty() {
            return Err(Error::InvalidQuery("target label is empty".into()));
        }
        for c in &self.constraints {
            if c.anchors.len() != c.relation.arity() {
                return Err(Error::InvalidQuery(format!(
                    "relation \"{}\" needs {} anchor(s), got {}",
                    c.relation,
                    c.relation.arity(),
                    c.anchors.len()
                )));
            }
        }
        Ok(())
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.target)?;
        for (i, c) in self.constraints.iter().enumerate() {
            f.write_str(if i == 0 { " | " } else { " AND " })?;
            let anchors: Vec<String> = c.anchors.iter().map(|a| a.to_string()).collect();
            write!(f, "{}({})", c.relation, anchors.join(", "))?;
            if c.relation.is_directional() && c.frame != Frame::Auto {
                write!(f, "[{}]", c.frame)?;
            }
        }
        Ok(())
    }
}

/// Canonical form of a label: lower case, `_`/`-` as spaces, single spaces.
pub fn normalize_label(s: &str) -> String {
    s.to_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_with_ids_and_frames() {
        let json = r#"{"target":"mug","constraints":[
            {"relation":"between","anchors":["laptop",{"id":"book_1"}]},
            {"relation":"left","anchors":[{"label":"chair"}],"frame":"egocentric"}],
            "candidates":["mug_1","mug_2"]}"#;
        let q = Query::from_json(json).unwrap();
        assert_eq!(q.constraints[0].anchors[1], ObjectRef::id("book_1"));
        assert_eq!(q.constraints[1].anchors[0], ObjectRef::label("chair"));
        assert_eq!(q.constraints[1].frame, Frame::Egocentric);
        assert_eq!(Query::from_json(&q.to_json()).unwrap(), q);
        assert_eq!(
            q.to_string(),
            "mug | between(laptop, #book_1) AND left(chair)[egocentric]"
        );
    }

    #[test]
    fn rejects_wrong_arity_and_unknown_fields() {
        assert!(Query::from_json(
            r#"{"target":"mug","constraints":[{"relation":"between","anchors":["a"]}]}"#
        )
        .is_err());
        assert!(Query::from_json(r#"{"target":"mug","constraint":[]}"#).is_err());
        assert!(Query::from_json(r#"{"target":"  "}"#).is_err());
        assert!(
            Query::from_json(r#"{"target":"mug","constraints":[{"relation":"near","anchors":["a"]}]}"#)
                .is_err()
        );
    }

    #[test]
    fn normalizes_labels() {
        assert_eq!(normalize_label("  Coffee_Table "), "coffee table");
        assert_eq!(normalize_label("trash-can"), "trash can");
    }
}
