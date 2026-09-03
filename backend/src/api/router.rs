use super::{routes, state::RouterDependencies};
use axum::Router;

pub(super) fn build(dependencies: RouterDependencies) -> Router {
    routes::public()
        .merge(routes::administration())
        .merge(routes::customer())
        .with_state(dependencies.into())
}
