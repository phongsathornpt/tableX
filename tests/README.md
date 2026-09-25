# Tests

`unit/` holds unit-test source files included by their owning modules with
`#[cfg(test)]` and `#[path]`. They stay child modules of the code they test so
they can exercise private behavior without widening the application API.

Root-level `*.rs` files are reserved for integration tests against the public
library API.
