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
    let mut object = RObject::List(values);
    for _ in 0..data.first().copied().unwrap_or(0) % 4 {
        object = RObject::List(vec![object]);
    }
    let bytes = write_rds(&object).expect("bounded object should serialize");
    read_rds_with_config(&bytes, ParseConfig::default()).expect("writer output should parse");
    for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
        let config = ParseConfig::default()
            .with_mode(mode)
            .with_max_nesting_depth(2)
            .with_max_vector_length(8)
            .with_max_allocation_bytes(256);
        let _ = read_rds_with_config(&bytes, config);
    }
});
