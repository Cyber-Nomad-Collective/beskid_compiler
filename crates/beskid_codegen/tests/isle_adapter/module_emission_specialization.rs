//! Whole-module syntax emission and generic specialization through the ISLE adapter.

#[path = "module_emission_specialization/call_boundaries.rs"]
mod call_boundaries;
#[path = "module_emission_specialization/generic_literals.rs"]
mod generic_literals;
#[path = "module_emission_specialization/generic_specialization.rs"]
mod generic_specialization;
#[path = "module_emission_specialization/imported_generics.rs"]
mod imported_generics;
#[path = "module_emission_specialization/imported_receivers.rs"]
mod imported_receivers;
#[path = "module_emission_specialization/typed_arrays.rs"]
mod typed_arrays;

use imported_receivers::{StringComparison, assert_string_content_comparison};
