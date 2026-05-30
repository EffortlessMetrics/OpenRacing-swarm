use crate::types::ConfigWriter;

mod constructors;
mod entries;

pub use entries::config_writer_factories;

/// Factory for constructing config writer instances.
pub type ConfigWriterFactory = fn() -> Box<dyn ConfigWriter + Send + Sync>;
