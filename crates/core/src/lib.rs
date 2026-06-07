//! mac-cleaner engine.
//!
//! Pipeline: [`scanner`] discovers candidates in allowlisted locations and
//! produces a [`plan::Plan`] (pure data, no side effects). [`executor`] turns a
//! plan into actions, but **only** when given explicit [`executor::Consent`];
//! the default is a dry run that mutates nothing. Every planned and executed
//! action is written to an append-only [`audit`] log.

pub mod audit;
pub mod categories;
pub mod executor;
pub mod loginitems;
pub mod plan;
pub mod report;
pub mod scanner;
