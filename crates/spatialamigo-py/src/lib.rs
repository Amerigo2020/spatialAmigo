//! Python bindings for spatialAmigo (milestone M4).

use pyo3::prelude::*;

/// Native module `spatialamigo._core`.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
