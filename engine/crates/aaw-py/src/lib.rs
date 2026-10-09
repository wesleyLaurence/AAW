//! The song model for Python: `agent_daw.aaw_py`.
//!
//! The sample library and the perception tools read, validate and save songs
//! through these functions, so there is one implementation of the document.
//! A song crosses as plain data: the full dump, every field present, as dicts,
//! lists, numbers and strings. Anything that takes a song validates it first.
//! A document the model refuses raises `ValueError` with the model's message.

use aaw_model::value::{Key, Value};
use aaw_model::{Beat, ModelError, Project};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::ToPrimitive;
use pyo3::exceptions::{PyOSError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use std::collections::HashMap;
use std::path::PathBuf;

/// A Python object as the tree validation reads. Tuples read as lists.
fn value(x: &Bound<'_, PyAny>) -> PyResult<Value> {
    if x.is_none() {
        return Ok(Value::None);
    }
    if let Ok(b) = x.cast::<PyBool>() {
        return Ok(Value::Bool(b.is_true()));
    }
    if x.is_instance_of::<PyInt>() {
        return Ok(Value::Int(x.extract::<BigInt>()?));
    }
    if let Ok(f) = x.cast::<PyFloat>() {
        return Ok(Value::Float(f.value()));
    }
    if let Ok(s) = x.cast::<PyString>() {
        return Ok(Value::Str(s.to_str()?.to_string()));
    }
    if let Ok(items) = x.cast::<PyList>() {
        return items.iter().map(|item| value(&item)).collect::<PyResult<_>>().map(Value::List);
    }
    if let Ok(items) = x.cast::<PyTuple>() {
        return items.iter().map(|item| value(&item)).collect::<PyResult<_>>().map(Value::List);
    }
    if let Ok(d) = x.cast::<PyDict>() {
        let mut out = aaw_model::value::Dict::new();
        for (k, v) in d.iter() {
            out.insert(Key(value(&k)?), value(&v)?);
        }
        return Ok(Value::Dict(out));
    }
    if let Ok(b) = x.cast::<PyBytes>() {
        return Ok(Value::Bytes(b.as_bytes().to_vec()));
    }
    // What YAML can also hold, which no field accepts.
    let name = x.get_type().name()?.to_string();
    for known in ["date", "datetime", "set"] {
        if name == known {
            return Ok(Value::Other(known));
        }
    }
    Err(PyTypeError::new_err(format!("A song cannot hold a value of type {name}")))
}

fn object<'py>(py: Python<'py>, v: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        Value::None => py.None().into_bound(py),
        Value::Bool(b) => PyBool::new(py, *b).to_owned().into_any(),
        Value::Int(n) => n.into_pyobject(py)?.into_any(),
        Value::Float(f) => PyFloat::new(py, *f).into_any(),
        Value::Str(s) => PyString::new(py, s).into_any(),
        Value::List(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(object(py, item)?)?;
            }
            list.into_any()
        }
        Value::Dict(d) => {
            let dict = PyDict::new(py);
            for (k, item) in d {
                dict.set_item(object(py, &k.0)?, object(py, item)?)?;
            }
            dict.into_any()
        }
        Value::Bytes(b) => PyBytes::new(py, b).into_any(),
        Value::Other(name) => return Err(PyTypeError::new_err(format!("A {name} is not part of a song"))),
    })
}

fn error(e: ModelError) -> PyErr {
    match e {
        ModelError::Io(e) => PyOSError::new_err(e.to_string()),
        other => PyValueError::new_err(other.to_string()),
    }
}

fn project(data: &Bound<'_, PyAny>) -> PyResult<Project> {
    Project::validate(&value(data)?).map_err(|e| PyValueError::new_err(e.to_string()))
}

fn full<'py>(py: Python<'py>, p: &Project) -> PyResult<Bound<'py, PyAny>> {
    object(py, &p.dump(false))
}

/// The song in YAML text, validated, as its full dump.
#[pyfunction]
fn parse<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    full(py, &aaw_model::parse(text).map_err(error)?)
}

/// The song in a file, validated, as its full dump. With `verify_assets`, each
/// sample must exist and match its hash.
#[pyfunction]
#[pyo3(signature = (path, verify_assets = true))]
fn load<'py>(py: Python<'py>, path: PathBuf, verify_assets: bool) -> PyResult<Bound<'py, PyAny>> {
    let loaded = aaw_model::load(&path, verify_assets).map_err(|e| match e {
        ModelError::Io(e) => PyOSError::new_err(format!("{}: {e}", path.display())),
        other => error(other),
    })?;
    full(py, &loaded)
}

/// A song given as data, validated, as its full dump.
#[pyfunction]
fn validate<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    full(py, &project(data)?)
}

/// The canonical YAML a save writes.
#[pyfunction]
fn to_yaml(data: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(aaw_model::to_yaml(&project(data)?))
}

/// Writes the song's canonical YAML to a file, through a temporary file.
#[pyfunction]
fn save(data: &Bound<'_, PyAny>, path: PathBuf) -> PyResult<()> {
    aaw_model::save(&project(data)?, &path).map_err(|e| PyOSError::new_err(format!("{}: {e}", path.display())))
}

/// The song's fingerprints, newest first: `project_sha256`, then the forms
/// earlier engines wrote in their render reports.
#[pyfunction]
fn fingerprints(data: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    Ok(aaw_model::fingerprints(&project(data)?))
}

/// Every hit as (start frame, track, pad, release frame or None), in time order.
#[pyfunction]
fn schedule(data: &Bound<'_, PyAny>) -> PyResult<Vec<(i64, String, String, Option<i64>)>> {
    let triggers = aaw_model::schedule::schedule(&project(data)?);
    Ok(triggers.into_iter().map(|t| (t.start, t.track_id, t.pad, t.cutoff)).collect())
}

/// What each track plays, as (track, start beat, end beat, pitch, hit): the
/// pitch a note sounds at as a MIDI number, or None with the pad and transpose
/// of a hit that has no note. Audio clips are not listed.
#[pyfunction]
fn sounded(data: &Bound<'_, PyAny>) -> PyResult<Vec<(String, f64, f64, Option<f64>, String)>> {
    let played = aaw_model::schedule::sounded(&project(data)?);
    Ok(played.into_iter().map(|s| (s.track_id, s.at, s.until, s.pitch, s.hit)).collect())
}

/// What `daw check` warns about in a valid song, as a JSON list of objects
/// with a code, a level, a message, the paths of what each is about and its
/// beat. `seconds` is each sample's file length by sample ID, for audio clips
/// that play to the end of their file.
#[pyfunction]
#[pyo3(signature = (data, seconds=None))]
fn warnings(data: &Bound<'_, PyAny>, seconds: Option<HashMap<String, f64>>) -> PyResult<String> {
    let p = project(data)?;
    let found = aaw_model::check::check(&p, &seconds.unwrap_or_default());
    Ok(serde_json::Value::Array(found.iter().map(|w| w.to_json()).collect()).to_string())
}

/// A beat written as an integer, a decimal or a fraction, as an exact
/// (numerator, denominator).
#[pyfunction]
fn beat(written: &Bound<'_, PyAny>) -> PyResult<(BigInt, BigInt)> {
    let written = match value(written)? {
        Value::Int(n) => Beat::Int(n),
        Value::Float(f) => Beat::Float(f),
        Value::Str(s) => Beat::Str(s),
        other => return Err(PyValueError::new_err(format!(
            "Invalid beat value {}; use a number or fraction like '1/3'",
            aaw_model::value::py_repr(&other)
        ))),
    };
    let exact = aaw_model::beat(&written).map_err(PyValueError::new_err)?;
    Ok((exact.numer().clone(), exact.denom().clone()))
}

/// The audio frame of an exact beat position, rounded half up from absolute time.
#[pyfunction]
fn frame(numerator: BigInt, denominator: BigInt, tempo: f64, rate: i64) -> PyResult<i64> {
    if denominator == BigInt::ZERO || !(tempo.is_finite() && tempo > 0.0) {
        return Err(PyValueError::new_err("A frame needs a nonzero denominator and a positive tempo"));
    }
    let position = BigRational::new(numerator, denominator);
    let estimate = position.to_f64().unwrap_or(f64::INFINITY).abs() * 60.0 * rate as f64 / tempo;
    if !(estimate < 4e18) {
        return Err(PyValueError::new_err("The position is beyond any timeline"));
    }
    Ok(aaw_model::frame(&position, tempo, rate))
}

/// The MIDI number of a note with its octave, such as C2 or F#1.
#[pyfunction]
fn midi(note: &str) -> PyResult<i64> {
    aaw_model::rules::midi(note).map_err(PyValueError::new_err)
}

/// The Agent DAW song model: validation, exact beats, canonical YAML,
/// fingerprints and the schedule, from the Rust core.
#[pymodule]
mod aaw_py {
    #[pymodule_export]
    use super::{beat, fingerprints, frame, load, midi, parse, save, schedule, sounded, to_yaml, validate, warnings};
}
