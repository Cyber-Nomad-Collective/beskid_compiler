//! Per-unit walk that builds a [`super::model::UnitTypeSurface`] without checking function bodies.

mod contracts;
mod items_lookup;
mod methods;
mod register;
mod state;
mod types_lookup;
mod walk;

pub(super) use state::TypeSurfaceBuilder;
