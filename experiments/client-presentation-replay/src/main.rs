use client_presentation_replay::run_replay;

fn main() {
    let report = run_replay();
    println!("client presentation replay: success");
    println!("  player={} vendor={}", report.player_id, report.vendor_id);
    println!("  applied authoritative events={}", report.applied_events);
    println!(
        "  projected position=({:.1}, {:.1}) area={:?}",
        report.projected_position.0, report.projected_position.1, report.projected_area
    );
    println!(
        "  defeated enemies={} quest={} gold={}",
        report.defeated_enemies, report.completed_quest, report.projected_gold
    );
    println!(
        "  inventory: pelts={} rations={} potions={}",
        report.projected_pelts, report.projected_rations, report.projected_potions
    );
    println!(
        "  vendor potion stock remaining={}",
        report.vendor_potions_remaining
    );
}
