use std::time::Instant;

use mmorpg_ui_contract::{AccountId, PackageId, StorageNamespace, StoredValue};
use ui_scripting_spike::{AddonPolicy, AddonRunner, DispatchResult, StorageWorker, VisibleState};

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
    let rss_baseline_kib = process_rss_kib();
    let mut rss_peak_kib = rss_baseline_kib;
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
        if let Some(rss) = process_rss_kib() {
            rss_peak_kib = Some(rss_peak_kib.unwrap_or(rss).max(rss));
        }
    }
    load_ns.sort_unstable();
    callback_ns.sort_unstable();
    let rss_after_load_kib = process_rss_kib();
    let (storage_us, reload_us) = storage_latency_sample();
    let rss_after_storage_kib = process_rss_kib();
    println!(
        "ui adversarial gate: iterations={ITERATIONS} load_ns={} callback_ns={} load_p50_ns={} load_p95_ns={} load_max_ns={} callback_p50_ns={} callback_p95_ns={} callback_max_ns={} rss_baseline_kib={:?} rss_peak_kib={:?} rss_after_load_kib={:?} rss_after_storage_kib={:?} storage_p50_us={} storage_p95_us={} storage_max_us={} storage_reload_us={}",
        load_ns.len(),
        callback_ns.len(),
        percentile(&load_ns, 50),
        percentile(&load_ns, 95),
        load_ns[load_ns.len() - 1],
        percentile(&callback_ns, 50),
        percentile(&callback_ns, 95),
        callback_ns[callback_ns.len() - 1],
        rss_baseline_kib,
        rss_peak_kib,
        rss_after_load_kib,
        rss_after_storage_kib,
        percentile(&storage_us, 50),
        percentile(&storage_us, 95),
        storage_us[storage_us.len() - 1],
        reload_us,
    );
}

fn process_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmRSS:")?.split_whitespace().next()?;
        value.parse().ok()
    })
}

fn storage_latency_sample() -> (Vec<u128>, u128) {
    const COMMITS: u64 = 10;
    let path = std::env::temp_dir().join(format!(
        "mmorpg-ui-storage-benchmark-{}.state",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let namespace = StorageNamespace {
        account_id: AccountId::new(7001).expect("benchmark account must be non-zero"),
        package_id: PackageId::new(7002).expect("benchmark package must be non-zero"),
        schema_version: 1,
    };
    let worker = StorageWorker::open(path.clone(), namespace.clone())
        .expect("storage benchmark worker must open");
    let mut latencies = Vec::with_capacity(COMMITS as usize);
    for request_id in 1..=COMMITS {
        let start = Instant::now();
        worker
            .set(
                request_id,
                format!("benchmark-{request_id}"),
                StoredValue::Integer(request_id as i64),
            )
            .expect("storage benchmark request must be admitted");
        loop {
            if let Some(result) = worker.try_result() {
                assert_eq!(result.request_id, request_id);
                assert!(result.result.is_ok(), "storage benchmark commit failed");
                latencies.push(start.elapsed().as_micros());
                break;
            }
            std::thread::yield_now();
        }
    }
    drop(worker);

    let start = Instant::now();
    let reloaded =
        StorageWorker::open(path.clone(), namespace).expect("storage benchmark restart must open");
    let reload_us = start.elapsed().as_micros();
    drop(reloaded);
    let _ = std::fs::remove_file(path);
    latencies.sort_unstable();
    (latencies, reload_us)
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
    let index = ((samples.len() - 1) * percentile).div_ceil(100);
    samples[index]
}
