use pyo3::{
    Bound,
    PyResult,
    pyfunction,
    pymodule,
    types::{PyModule, PyModuleMethods},
    wrap_pyfunction,
};

#[pyfunction]
pub fn add(left: u64, right: u64) -> PyResult<u64> {
    Ok(add_internal(left, right))
}

fn add_internal(left: u64, right: u64) -> u64 {
    left + right
}

#[pymodule]
fn lib_sql_test(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(add, m)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add_internal(2, 2);
        assert_eq!(result, 4);
    }
}
