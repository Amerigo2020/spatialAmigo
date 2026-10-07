//! Python bindings for spatialAmigo (`spatialamigo._core`).
//!
//! Data crosses the boundary as JSON-compatible Python objects (dicts, lists,
//! strings). The Rust types stay the single source of truth for validation.

use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use spatialamigo::{Error, Params, Query, Resolution, ResolveOptions, Resolver, Scene};

create_exception!(
    _core,
    SpatialAmigoError,
    PyValueError,
    "Base class of all spatialAmigo errors."
);
create_exception!(
    _core,
    SceneError,
    SpatialAmigoError,
    "The scene is not valid scene format v0.1."
);
create_exception!(
    _core,
    QueryError,
    SpatialAmigoError,
    "The structured query is invalid."
);
create_exception!(
    _core,
    ParseError,
    SpatialAmigoError,
    "The template parser could not understand the text."
);
create_exception!(
    _core,
    ReferenceError,
    SpatialAmigoError,
    "A label or id is not present in the scene."
);

fn to_py_err(e: Error) -> PyErr {
    let msg = e.to_string();
    match e {
        Error::Json(_) | Error::InvalidScene(_) => SceneError::new_err(msg),
        Error::InvalidQuery(_) => QueryError::new_err(msg),
        Error::Parse { .. } => ParseError::new_err(msg),
        Error::UnknownReference(_) | Error::MissingViewpoint(_) => ReferenceError::new_err(msg),
    }
}

/// Accepts a JSON string or a JSON-compatible Python object and returns a JSON string.
fn as_json(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = obj.cast::<PyString>() {
        return Ok(s.to_string());
    }
    let json = py.import("json")?;
    json.call_method1("dumps", (obj,))?.extract()
}

fn from_json(py: Python<'_>, s: &str) -> PyResult<Py<PyAny>> {
    let json = py.import("json")?;
    Ok(json.call_method1("loads", (s,))?.unbind())
}

/// A validated scene (format v0.1).
#[pyclass(name = "Scene", module = "spatialamigo._core", frozen, from_py_object)]
#[derive(Clone)]
struct PyScene {
    inner: Scene,
}

#[pymethods]
impl PyScene {
    /// Builds a scene from a dict or a JSON string.
    #[new]
    fn new(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        let json = as_json(py, data)?;
        let inner = Scene::from_json(&json).map_err(|e| match e {
            Error::Json(m) => SceneError::new_err(format!("invalid scene JSON: {m}")),
            other => to_py_err(other),
        })?;
        Ok(PyScene { inner })
    }

    /// Loads a scene from a JSON file.
    #[staticmethod]
    fn from_file(py: Python<'_>, path: std::path::PathBuf) -> PyResult<Self> {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| SceneError::new_err(format!("cannot read {}: {e}", path.display())))?;
        PyScene::new(py, PyString::new(py, &text).as_any())
    }

    /// Sorted list of distinct labels.
    #[getter]
    fn labels(&self) -> Vec<String> {
        self.inner.labels()
    }

    /// Object ids in scene order.
    #[getter]
    fn ids(&self) -> Vec<String> {
        self.inner.objects.iter().map(|o| o.id.clone()).collect()
    }

    /// True if the scene has a viewpoint.
    #[getter]
    fn has_viewpoint(&self) -> bool {
        self.inner.viewpoint.is_some()
    }

    /// The scene as a dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        from_json(py, &self.inner.to_json())
    }

    /// The scene as pretty JSON.
    fn to_json(&self) -> String {
        self.inner.to_json()
    }

    fn __len__(&self) -> usize {
        self.inner.objects.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "Scene({} objects, labels={:?})",
            self.inner.objects.len(),
            self.inner.labels()
        )
    }
}

/// Result of resolving a query: ranked candidates with trace.
#[pyclass(name = "Resolution", module = "spatialamigo._core", frozen)]
struct PyResolution {
    inner: Resolution,
}

#[pymethods]
impl PyResolution {
    /// Id of the best candidate, or None if nothing matched.
    #[getter]
    fn best(&self) -> Option<String> {
        self.inner.best.clone()
    }

    /// Score difference between rank 1 and rank 2.
    #[getter]
    fn margin(&self) -> f64 {
        self.inner.margin
    }

    /// Human readable explanation.
    #[getter]
    fn explanation(&self) -> String {
        self.inner.explanation.clone()
    }

    /// The evaluated query as a dict.
    #[getter]
    fn query(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        from_json(py, &self.inner.query.to_json())
    }

    /// Ranked candidates as a list of dicts (id, label, rank, score, trace).
    #[getter]
    fn candidates(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let s = serde_json::to_string(&self.inner.candidates)
            .map_err(|e| SpatialAmigoError::new_err(e.to_string()))?;
        from_json(py, &s)
    }

    /// `(id, score)` pairs in rank order.
    #[getter]
    fn scores(&self) -> Vec<(String, f64)> {
        self.inner
            .candidates
            .iter()
            .map(|c| (c.id.clone(), c.score))
            .collect()
    }

    /// Everything as a dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        from_json(py, &self.inner.to_json())
    }

    /// Everything as pretty JSON.
    fn to_json(&self) -> String {
        self.inner.to_json()
    }

    fn __repr__(&self) -> String {
        let best = self
            .inner
            .best
            .as_ref()
            .map_or("None".to_string(), |b| format!("'{b}'"));
        format!(
            "Resolution(best={best}, margin={:.3}, candidates={})",
            self.inner.margin,
            self.inner.candidates.len()
        )
    }

    fn __str__(&self) -> String {
        self.inner.explanation.clone()
    }
}

/// Resolves referring expressions or structured queries in one scene.
#[pyclass(name = "Resolver", module = "spatialamigo._core", frozen)]
struct PyResolver {
    inner: Resolver,
}

#[pymethods]
impl PyResolver {
    /// Creates a resolver. `scene` is a `Scene`, dict or JSON string; `params`
    /// optionally overrides relation constants (see `default_params()`).
    #[new]
    #[pyo3(signature = (scene, params=None))]
    fn new(py: Python<'_>, scene: &Bound<'_, PyAny>, params: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let scene = match scene.extract::<PyScene>() {
            Ok(s) => s.inner,
            Err(_) => PyScene::new(py, scene)?.inner,
        };
        let params: Params = match params {
            Some(p) => serde_json::from_str(&as_json(py, p)?)
                .map_err(|e| QueryError::new_err(format!("invalid params: {e}")))?,
            None => Params::default(),
        };
        Ok(PyResolver {
            inner: Resolver::with_params(scene, params),
        })
    }

    /// Resolves a sentence (str, parsed by the template parser) or a
    /// structured query (dict). `explain=False` skips the text explanation
    /// (the structured trace stays complete).
    #[pyo3(signature = (query, explain=true))]
    fn resolve(&self, py: Python<'_>, query: &Bound<'_, PyAny>, explain: bool) -> PyResult<PyResolution> {
        let opts = ResolveOptions { explanation: explain };
        let res = if let Ok(text) = query.cast::<PyString>() {
            let text = text.to_string();
            py.detach(|| {
                spatialamigo::parse(&text, &self.inner.scene().labels())
                    .and_then(|q| self.inner.resolve_with(&q, &opts))
            })
        } else {
            let q = Query::from_json(&as_json(py, query)?).map_err(|e| match e {
                Error::Json(m) => QueryError::new_err(format!("invalid query: {m}")),
                other => to_py_err(other),
            })?;
            py.detach(|| self.inner.resolve_with(&q, &opts))
        };
        res.map(|inner| PyResolution { inner }).map_err(to_py_err)
    }

    /// Resolves many sentences or queries; errors are returned in place as
    /// exceptions instances instead of being raised.
    fn resolve_many(&self, py: Python<'_>, queries: &Bound<'_, PyList>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::with_capacity(queries.len());
        for q in queries.iter() {
            match self.resolve(py, &q, true) {
                Ok(r) => out.push(Py::new(py, r)?.into_any()),
                Err(e) => out.push(e.into_value(py).into_any()),
            }
        }
        Ok(out)
    }

    /// The scene this resolver works on.
    #[getter]
    fn scene(&self) -> PyScene {
        PyScene {
            inner: self.inner.scene().clone(),
        }
    }

    /// Relation parameters in use, as a dict.
    #[getter]
    fn params(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        from_json(
            py,
            &serde_json::to_string(self.inner.params()).expect("params serialize"),
        )
    }
}

/// Parses a sentence with the deterministic template parser and returns the
/// structured query as a dict. `labels` is the vocabulary (scene labels).
#[pyfunction]
#[pyo3(signature = (text, labels=None))]
fn parse(py: Python<'_>, text: &str, labels: Option<Vec<String>>) -> PyResult<Py<PyAny>> {
    let q = spatialamigo::parse(text, &labels.unwrap_or_default()).map_err(to_py_err)?;
    from_json(py, &q.to_json())
}

/// Validates a structured query (dict or JSON string) and returns its
/// canonical dict form. Raises `QueryError` if it is invalid.
#[pyfunction]
fn validate_query(py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let q = Query::from_json(&as_json(py, query)?).map_err(|e| match e {
        Error::Json(m) => QueryError::new_err(format!("invalid query: {m}")),
        other => to_py_err(other),
    })?;
    from_json(py, &q.to_json())
}

/// Default relation parameters as a dict.
#[pyfunction]
fn default_params(py: Python<'_>) -> PyResult<Py<PyAny>> {
    from_json(
        py,
        &serde_json::to_string(&Params::default()).expect("params serialize"),
    )
}

/// Native module `spatialamigo._core`.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("SCENE_SCHEMA", from_json(py, spatialamigo::SCENE_SCHEMA)?)?;
    m.add("QUERY_SCHEMA", from_json(py, spatialamigo::QUERY_SCHEMA)?)?;
    m.add_class::<PyScene>()?;
    m.add_class::<PyResolver>()?;
    m.add_class::<PyResolution>()?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(validate_query, m)?)?;
    m.add_function(wrap_pyfunction!(default_params, m)?)?;
    m.add("SpatialAmigoError", py.get_type::<SpatialAmigoError>())?;
    m.add("SceneError", py.get_type::<SceneError>())?;
    m.add("QueryError", py.get_type::<QueryError>())?;
    m.add("ParseError", py.get_type::<ParseError>())?;
    m.add("ReferenceError", py.get_type::<ReferenceError>())?;
    Ok(())
}
