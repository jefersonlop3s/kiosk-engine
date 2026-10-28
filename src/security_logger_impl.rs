//! Log forense. O tipo `SecurityEvent` vive em `database_schema` (fonte única);
//! a gravação transacional entra na F3 do E-001.
pub use crate::database_schema::SecurityEvent;
