// Download engine moved to launcher-net (phase 2, no behaviour change);
// re-export keeps every `crate::download::…` call site unchanged.
pub use launcher_net::download::*;
