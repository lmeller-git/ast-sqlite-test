// use std::sync::Arc;

use sqlparser::ast::Statement;

// TODO maybe use Cow<Stmt> or Arc<Stmt>

pub type AST = Vec<Statement>;
