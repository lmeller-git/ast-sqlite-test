// Adapted from sqloxide (https://github.com/wseaton/sqloxide)
// Original author: Will Eaton — MIT License

use engine::{Engine, SchedulerBuilder, StrategyBuilder};
use lsf_core::entry::{CorpusEntry as RawCorpusEntry, ID as RawID, Meta, RawEntry as RawestEntry};
use pyo3::{exceptions::PyValueError, prelude::*, pymodule};
use sqlparser::{
    dialect::{dialect_from_str, *},
    parser::Parser,
};
use visitor::{
    depythonize_query,
    extract_expressions,
    extract_relations,
    mutate_expressions,
    mutate_relations,
    pythonize_query_output,
};

use crate::engine::{SeedGeneratorBuilder, SelectedGeneration};

mod engine;
mod visitor;

/// Function to parse SQL statements from a string. Returns a list with
/// one item per query statement.
///
/// Available `dialects`: https://github.com/sqlparser-rs/sqlparser-rs/blob/main/src/dialect/mod.rs#L189-L206
#[pyfunction]
#[pyo3(text_signature = "(sql, dialect)")]
fn parse_sql(py: Python, sql: String, dialect: String) -> PyResult<Py<PyAny>> {
    let chosen_dialect = dialect_from_str(dialect).unwrap_or_else(|| {
        println!("The dialect you chose was not recognized, falling back to 'generic'");
        Box::new(GenericDialect {})
    });
    let parse_result = Parser::parse_sql(&*chosen_dialect, &sql);

    let output = match parse_result {
        Ok(statements) => pythonize_query_output(py, statements)?,
        Err(e) => {
            let msg = e.to_string();
            return Err(PyValueError::new_err(format!(
                "Query parsing failed.\n\t{msg}"
            )));
        }
    };

    Ok(output)
}

/// This utility function allows reconstituing a modified AST back into a SQL query.
#[pyfunction]
#[pyo3(text_signature = "(ast)")]
fn restore_ast(_py: Python, ast: &Bound<'_, PyAny>) -> PyResult<String> {
    let parse_result = depythonize_query(ast)?;

    Ok(parse_result
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>()
        .join(";"))
}

#[pyclass(from_py_object)]
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct ID(RawID);

#[pymethods]
impl ID {
    #[new]
    pub fn new() -> Self {
        Self(RawID::next())
    }
}

impl From<RawID> for ID {
    fn from(value: RawID) -> Self {
        ID(value)
    }
}

#[pyclass(from_py_object)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CorpusEntry(RawCorpusEntry);

#[pymethods]
impl CorpusEntry {
    #[getter]
    pub fn id(&self) -> ID {
        self.0.id().into()
    }

    pub fn as_ast(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        pythonize_query_output(py, self.0.ast().clone())
    }
}

#[pyclass]
#[derive(Debug, PartialEq, Eq)]
pub struct RawEntry(Option<RawestEntry>);

#[pymethods]
impl RawEntry {
    #[getter]
    pub fn id(&self) -> ID {
        self.0.as_ref().unwrap().id().into()
    }

    pub fn as_ast(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        pythonize_query_output(py, self.0.as_ref().unwrap().ast().clone())
    }

    pub fn into_corpus_entry(&mut self, meta: TestMeta) -> CorpusEntry {
        CorpusEntry(self.0.take().unwrap().into_corpus_entry(meta.0))
    }
}

#[pyclass(from_py_object)]
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct TestMeta(Meta);

#[pymethods]
impl TestMeta {
    #[new]
    #[pyo3(signature = (exec_time, new_cov_nodes = 0, triggers_bug = false, is_valid_syntax = true))]
    pub fn new(
        exec_time: u32,
        new_cov_nodes: usize,
        triggers_bug: bool,
        is_valid_syntax: bool,
    ) -> Self {
        Self(Meta {
            triggers_bug,
            is_valid_syntax,
            new_cov_nodes,
            exec_time,
        })
    }

    #[getter]
    pub fn triggers_bug(&self) -> bool {
        self.0.triggers_bug
    }

    #[getter]
    pub fn is_valid_syntax(&self) -> bool {
        self.0.is_valid_syntax
    }

    #[getter]
    pub fn exec_time(&self) -> u32 {
        self.0.exec_time
    }

    #[getter]
    pub fn new_cov_nodes(&self) -> usize {
        self.0.new_cov_nodes
    }

    #[setter]
    pub fn set_triggers_bug(&mut self, triggers_bug: bool) {
        self.0.triggers_bug = triggers_bug
    }

    #[setter]
    pub fn set_valid_syntax(&mut self, is_valid_syntax: bool) {
        self.0.is_valid_syntax = is_valid_syntax
    }

    #[setter]
    pub fn set_exec_time(&mut self, exec_time: u32) {
        self.0.exec_time = exec_time
    }

    #[setter]
    pub fn set_new_cov_nodes(&mut self, new_cov_nodes: usize) {
        self.0.new_cov_nodes = new_cov_nodes
    }
}

#[pymodule]
fn lib_sf(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_sql, m)?)?;
    m.add_function(wrap_pyfunction!(restore_ast, m)?)?;

    m.add_class::<ID>()?;
    m.add_class::<RawEntry>()?;
    m.add_class::<CorpusEntry>()?;
    m.add_class::<TestMeta>()?;

    let engine = PyModule::new(py, "engine")?;
    engine.add_class::<Engine>()?;
    engine.add_class::<StrategyBuilder>()?;
    engine.add_class::<SchedulerBuilder>()?;
    engine.add_class::<SeedGeneratorBuilder>()?;
    engine.add_class::<SelectedGeneration>()?;
    m.add_submodule(&engine)?;

    let visitor = PyModule::new(py, "visitor")?;
    visitor.add_function(wrap_pyfunction!(extract_relations, m)?)?;
    visitor.add_function(wrap_pyfunction!(mutate_relations, m)?)?;
    visitor.add_function(wrap_pyfunction!(extract_expressions, m)?)?;
    visitor.add_function(wrap_pyfunction!(mutate_expressions, m)?)?;
    m.add_submodule(&visitor)?;

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Once;

    use pyo3::{PyResult, Python, types::PyAnyMethods};

    use crate::lib_sf;

    static INIT: Once = Once::new();

    pub(crate) fn python_setup() {
        INIT.call_once(|| {
            pyo3::append_to_inittab!(lib_sf);
            Python::initialize();
        });
    }

    #[test]
    fn test_parse_roundtrip() {
        python_setup();
        let query = "SELECT A FROM B";
        Python::attach(|py| -> PyResult<()> {
            let lib_sf_ = py.import("lib_sf").unwrap();
            let parse = lib_sf_.getattr("parse_sql").unwrap();
            let restore = lib_sf_.getattr("restore_ast").unwrap();

            let sql = parse.call1((query, "SQLite")).unwrap();

            let restored: Vec<String> = restore.call1((sql,)).unwrap().extract().unwrap();

            assert_eq!(restored[0], query);

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_parse_fail() {
        python_setup();
        Python::attach(|py| -> PyResult<()> {
            let lib_sf_ = py.import("lib_sf").unwrap();
            let parse = lib_sf_.getattr("parse_sql").unwrap();
            assert!(parse.call1(("malformed query", "SQLite")).is_err());
            Ok(())
        })
        .unwrap()
    }

    #[test]
    fn test_restore_fail() {
        python_setup();
        Python::attach(|py| -> PyResult<()> {
            let lib_sf_ = py.import("lib_sf").unwrap();
            let restore = lib_sf_.getattr("restore_ast").unwrap();
            assert!(restore.call1(("malformed ast",)).is_err());
            Ok(())
        })
        .unwrap()
    }
}
