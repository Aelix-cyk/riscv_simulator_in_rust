# Repository Guidelines

1. This project is test-driven and progressive.
2. Use environment variables instead of absolute paths.
3. Only 64-bit machines are supported, so use `u64` instead of `usize` for architectural values; `usize` is only for host-side indexing and allocation.
