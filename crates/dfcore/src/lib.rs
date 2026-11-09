//! # dfcore — equation-oriented thermodynamic solver
//!
//! Pipeline: [`parser`] → [`compile`] (unrolling, units, AD tapes) →
//! [`structure`] (matching, Dulmage–Mendelsohn, BLT) → [`solver`]
//! (explicit evaluation / Newton per block).
//!
//! ```no_run
//! let model = dfcore::compile("x + y = 3\nx - y = 1", &Default::default());
//! let (_plan, sol) = dfcore::solve(&model, &Default::default());
//! assert!(sol.converged);
//! ```

pub mod ast;
pub mod combustion;
pub mod compile;
pub mod dffile;
pub mod diagrams;
pub mod dims;
pub mod ees;
pub mod ees_export;
pub mod funcs;
pub mod lexer;
pub mod liquids;
pub mod locale;
pub mod lookup;
pub mod parser;
pub mod props;
pub mod solver;
pub mod structure;
pub mod studies;
pub mod tape;
pub mod units;

pub use compile::{compile, CompileOptions, Model, VarInfo};
pub use parser::{Diagnostic, Severity};
pub use solver::{solve, solve_with_plan, Plan, Solution, SolveOptions};
pub use structure::{EqStatus, Structure, VarStatus};
pub use units::{Dim, Unit, UnitSystem};
