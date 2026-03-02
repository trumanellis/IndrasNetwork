//! Business logic services extracted from the monolithic app component.
//!
//! Each module contains standalone async functions that perform a single
//! responsibility, taking their dependencies as explicit parameters rather
//! than relying on Dioxus signals or component state.

pub mod boot;
pub mod polling;
pub mod intention_data;
pub mod event_subscription;
