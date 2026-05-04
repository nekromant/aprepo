use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(v: bool) {
    VERBOSE.store(v, Ordering::Relaxed);
}

pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

pub fn info(msg: &str) {
    println!("{}", msg);
}

pub fn warn(msg: &str) {
    eprintln!("WARNING: {}", msg);
}

pub fn error(msg: &str) {
    eprintln!("ERROR: {}", msg);
}

pub fn debug(msg: &str) {
    if is_verbose() {
        eprintln!("DEBUG: {}", msg);
    }
}
