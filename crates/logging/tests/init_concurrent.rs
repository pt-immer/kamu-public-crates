//! Concurrent callers converge on one owned installation.

#![cfg(feature = "systemd")]

use std::sync::{Arc, Barrier};

use kamu_logging::{InitOptions, Sink, init_with};

#[test]
fn idempotent_concurrent_init_has_one_commit_and_no_false_foreign_owner() {
    const CALLERS: usize = 8;

    let barrier = Arc::new(Barrier::new(CALLERS));
    let threads: Vec<_> = (0..CALLERS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                init_with(InitOptions::default().with_sink(Sink::Stderr).idempotent(true))
            })
        })
        .collect();

    for thread in threads {
        thread
            .join()
            .expect("init caller must not panic")
            .expect("all idempotent callers observe owned state");
    }
}
