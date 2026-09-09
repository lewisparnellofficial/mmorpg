use std::time::Instant;

use ui_scripting_spike::{AddonPolicy, AddonRunner, DispatchResult, VisibleState};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--adversarial-gate") {
        adversarial_gate();
        return;
    }

    let source = r#"
        local panel = ui.create_panel("waiting")
        ui.on("frame", function(event)
            ui.set_text(panel, event.visible.player_name)
        end)
    "#;
    let mut runner = AddonRunner::load("demo-addon", source, AddonPolicy::default())
        .expect("demo addon should load");
    let state = VisibleState {
        player_name: "Aria".to_owned(),
        target_name: Some("Field Wolf".to_owned()),
        inventory_slots_used: 2,
        quest_progress: 1,
    };
    let result = runner.deliver_event("frame", &state);
    let snapshot = runner.snapshot();
    println!(
        "ui scripting spike: dispatch={result:?} nodes={} events={} secure_intents={} errors={}",
        snapshot.nodes.len(),
        snapshot.registered_events,
        snapshot.secure_intents.len(),
        snapshot.errors.len()
    );
}

fn adversarial_gate() {
    const ITERATIONS: usize = 100;
    let source = r#"
        local panel = ui.create_panel("initial")
        ui.on("frame", function(event)
            ui.set_text(panel, event.visible.player_name)
        end)
    "#;
    let state = VisibleState::new("Aria", Some("Field Wolf".into()), 2, 1);
    let mut load_ns = Vec::with_capacity(ITERATIONS);
    let mut callback_ns = Vec::with_capacity(ITERATIONS);
    for index in 0..ITERATIONS {
        let start = Instant::now();
        let mut runner = AddonRunner::load(
            format!("benchmark-addon-{index}"),
            source,
            AddonPolicy::default(),
        )
        .expect("bounded benchmark addon must load");
        load_ns.push(start.elapsed().as_nanos());

        let start = Instant::now();
        for _ in 0..10 {
            assert_eq!(
                runner.deliver_event("frame", &state),
                DispatchResult::Applied
            );
        }
        callback_ns.push(start.elapsed().as_nanos() / 10);
    }
    load_ns.sort_unstable();
    callback_ns.sort_unstable();
    println!(
        "ui adversarial gate: iterations={ITERATIONS} load_ns={} callback_ns={} load_p50_ns={} load_p95_ns={} load_max_ns={} callback_p50_ns={} callback_p95_ns={} callback_max_ns={}",
        load_ns.len(),
        callback_ns.len(),
        percentile(&load_ns, 50),
        percentile(&load_ns, 95),
        load_ns[load_ns.len() - 1],
        percentile(&callback_ns, 50),
        percentile(&callback_ns, 95),
        callback_ns[callback_ns.len() - 1],
    );
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
    let index = ((samples.len() - 1) * percentile).div_ceil(100);
    samples[index]
}
