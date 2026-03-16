// Adapted from sqloxide (https://github.com/wseaton/sqloxide)
// Original author: Will Eaton — MIT License

use pyo3::{exceptions::PyValueError, prelude::*};
use sqlparser::{
    dialect::{dialect_from_str, *},
    parser::Parser,
};

mod visitor;
use visitor::{extract_expressions, extract_relations, mutate_expressions, mutate_relations};

use crate::visitor::{depythonize_query, pythonize_query_output};

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

/// This utility function allows reconstituing a modified AST back into list of SQL queries.
#[pyfunction]
#[pyo3(text_signature = "(ast)")]
fn restore_ast(_py: Python, ast: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let parse_result = depythonize_query(ast)?;

    Ok(parse_result
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>())
}

#[pymodule]
fn lib_sf(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_sql, m)?)?;
    m.add_function(wrap_pyfunction!(restore_ast, m)?)?;
    // TODO: maybe refactor into seperate module
    m.add_function(wrap_pyfunction!(extract_relations, m)?)?;
    m.add_function(wrap_pyfunction!(mutate_relations, m)?)?;
    m.add_function(wrap_pyfunction!(extract_expressions, m)?)?;
    m.add_function(wrap_pyfunction!(mutate_expressions, m)?)?;
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
