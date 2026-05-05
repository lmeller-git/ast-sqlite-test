use sqlparser::ast::Statement;

// TODO maybe use Cow<Stmt>

pub type AST = Vec<Statement>;
