#[cfg(not(target_arch = "wasm32"))]
use std::env;

#[cfg(not(target_arch = "wasm32"))]
fn print_usage() {
    eprintln!("Usage: rds-read <input.rds> [--trusted]");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let mut args = env::args().skip(1);
    let Some(input) = args.next() else {
        print_usage();
        std::process::exit(2);
    };
    let mut trusted = false;
    if let Some(flag) = args.next() {
        if flag == "--trusted" {
            trusted = true;
        } else {
            print_usage();
            std::process::exit(2);
        }
        if args.next().is_some() {
            print_usage();
            std::process::exit(2);
        }
    }

    let data = std::fs::read(&input).expect("read input");
    let _obj = if trusted {
        rds2rust::read_rds_with_config(&data, rds2rust::ParseConfig::for_trusted_large_file())
            .expect("parse rds")
    } else {
        rds2rust::read_rds(&data).expect("parse rds")
    };
}

#[cfg(target_arch = "wasm32")]
fn main() {}
