#![no_main]
use libfuzzer_sys::fuzz_target;
use rds2rust::{read_rds_with_config, write_rds, ParseConfig, ParseMode, RObject};

fuzz_target!(|data: &[u8]| {
    if data.len() > 1024 {
        return;
    }
    let values: Vec<RObject> = data
        .chunks(16)
        .take(16)
        .map(|chunk| {
            RObject::Integer(
                chunk
                    .iter()
                    .map(|byte| i32::from(*byte))
                    .collect::<Vec<_>>()
                    .into(),
            )
        })
        .collect();
    // Nest deep enough to straddle both the default (64) and hard-cap (128)
    // nesting limits, not just a handful of levels, so the fuzz corpus can
    // find off-by-one/boundary bugs in enter_nesting() itself.
    let extra_wraps = usize::from(data.first().copied().unwrap_or(0)) % 140;
    let mut object = RObject::List(values);
    for _ in 0..extra_wraps {
        object = RObject::List(vec![object]);
    }
    let bytes = write_rds(&object).expect("bounded object should serialize");

    // A config matching the actual nesting depth built above must always
    // round-trip, regardless of how deep that happens to be this run.
    let matching_depth_config =
        ParseConfig::default().with_max_nesting_depth((extra_wraps + 4).min(128));
    if extra_wraps <= 124 {
        // Only assert success when the object's real depth is guaranteed to
        // fit under the 128 hard cap even after the +4 margin above.
        read_rds_with_config(&bytes, matching_depth_config)
            .expect("writer output should parse under a config sized to its own depth");
    }

    for max_nesting_depth in [2usize, 63, 64, 65, 127, 128, 200] {
        for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
            let config = ParseConfig::default()
                .with_mode(mode)
                .with_max_nesting_depth(max_nesting_depth)
                .with_max_vector_length(8)
                .with_max_allocation_bytes(256);
            let _ = read_rds_with_config(&bytes, config);
        }
    }
});
