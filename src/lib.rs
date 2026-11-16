#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(any(target_arch = "wasm32", test))]
mod logic;

#[cfg(target_arch = "wasm32")]
mod runtime;

#[cfg(test)]
mod tests;
