//! PyO3 FFI bindings — the ONLY module that touches Python types.
//!
//! No business logic here. Translates between Rust-native types
//! and Python, maps ParseError variants to specific Python exceptions.

use pyo3::create_exception;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::errors::ParseError;
use crate::extract;

// Python exception types mapped from ParseError variants.
create_exception!(lightningparse, CorruptPdfError, pyo3::exceptions::PyException);
create_exception!(lightningparse, UnsupportedPdfError, pyo3::exceptions::PyException);
create_exception!(lightningparse, OcrEngineError, pyo3::exceptions::PyException);

impl From<ParseError> for PyErr {
    fn from(err: ParseError) -> PyErr {
        match err {
            ParseError::CorruptPdf(msg) => CorruptPdfError::new_err(msg),
            ParseError::UnsupportedPdf(msg) => UnsupportedPdfError::new_err(msg),
            ParseError::OcrEngine(msg) => OcrEngineError::new_err(msg),
            ParseError::Io(e) => PyRuntimeError::new_err(format!("IO error: {e}")),
            ParseError::Internal(msg) => PyRuntimeError::new_err(format!("Internal error: {msg}")),
        }
    }
}

/// Parse a PDF file and return structured JSON.
///
/// GIL is released during the actual parsing work.
#[pyfunction]
#[pyo3(signature = (path))]
fn parse_pdf(py: Python<'_>, path: String) -> PyResult<String> {
    let result_json = py.allow_threads(move || -> Result<String, ParseError> {
        let pdf_bytes = std::fs::read(&path).map_err(ParseError::Io)?;
        let result = extract::extract_text(&pdf_bytes)?;
        serde_json::to_string(&result)
            .map_err(|e| ParseError::Internal(format!("JSON serialization failed: {e}")))
    })?;

    Ok(result_json)
}

/// PyO3 module definition.
#[pymodule]
fn lightningparse(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_pdf, m)?)?;
    m.add("CorruptPdfError", m.py().get_type_bound::<CorruptPdfError>())?;
    m.add(
        "UnsupportedPdfError",
        m.py().get_type_bound::<UnsupportedPdfError>(),
    )?;
    m.add("OcrEngineError", m.py().get_type_bound::<OcrEngineError>())?;
    Ok(())
}
