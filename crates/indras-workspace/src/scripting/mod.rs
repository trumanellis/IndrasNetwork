//! Lua scripting support for GUI test automation.
//!
//! Feature-gated behind `lua-scripting`. Only available in debug builds.

pub mod action;
pub mod event;
pub mod query;
pub mod channels;
#[cfg(feature = "lua-scripting")]
pub mod lua_runtime;
#[cfg(feature = "lua-scripting")]
pub mod dispatcher;
