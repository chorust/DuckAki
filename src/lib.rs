// Derived in part from duckdb/extension-template-rs (commit abb7b2f3f954a47f90fc37a99cda3ce0618e6094)

pub mod bindings;
pub mod core;
pub mod eastmoney;

use duckdb::{duckdb_entrypoint_c_api, Connection, Result};
use std::error::Error;

#[duckdb_entrypoint_c_api]
pub unsafe fn extension_entrypoint(con: Connection) -> Result<(), Box<dyn Error>> {
    bindings::register(&con)?;
    Ok(())
}
