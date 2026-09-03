//! Warstwa HTTP CASP.
//!
//! Moduł scala tylko publiczną fabrykę routera. Implementacja tras, handlerów,
//! walidacji i mapowania błędów jest rozdzielona do podmodułów, aby transport
//! HTTP nie mieszał się z przypadkami użycia usług CASP.

mod handlers;
mod requests;
mod responses;
mod router;
mod routes;
mod state;
mod validators;

use axum::Router;

pub use state::RouterDependencies;

/// Buduje kompletny router HTTP CASP z jawnymi zależnościami aplikacji.
pub fn router(dependencies: RouterDependencies) -> Router {
    router::build(dependencies)
}
