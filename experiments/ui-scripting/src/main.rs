use ui_scripting_spike::{AddonPolicy, AddonRunner, VisibleState};

fn main() {
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
