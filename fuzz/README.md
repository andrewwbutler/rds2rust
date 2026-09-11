# Bounded parser round trips

Run `cargo +nightly fuzz run bounded_roundtrip -- -max_len=1024` from this
directory after installing cargo-fuzz. The target creates small lists and
integer vectors, writes them, and checks that the default parser accepts the
result. It also parses each result with small allocation and nesting limits.

The target covers writer-generated streams. It does not measure arbitrary
stream coverage. Add regression tests with small configured limits when a
resource boundary changes.
