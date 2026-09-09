#![no_main]

use libfuzzer_sys::fuzz_target;
use ui_scripting_spike::{AddonPolicy, AddonRunner, VisibleState};

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let source = String::from_utf8_lossy(data);
    let Ok(mut runner) = AddonRunner::load("fuzz-addon", &source, AddonPolicy::default()) else {
        return;
    };
    let _ = runner.deliver_event(
        "ui.ready",
        &VisibleState::new("fuzz-player", Some("fuzz-target".to_owned()), 0, 0),
    );
    let _ = runner.dispatch_queued();
});
