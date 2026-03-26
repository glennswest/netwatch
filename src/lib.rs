pub mod models;
pub mod config;
pub mod db;
pub mod snmp;
pub mod discovery;
pub mod monitor;
pub mod alert;
pub mod topo;
pub mod web;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
