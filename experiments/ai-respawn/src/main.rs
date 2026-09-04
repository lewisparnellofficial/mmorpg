use std::time::Duration;

use ai_respawn_experiment::{
    AiEvent, AiWorld, Enemy, EnemyId, PlayerId, PlayerObservation, Position, WorldConfig,
};

fn main() {
    let config = WorldConfig::try_new(10, Duration::from_secs(3)).expect("valid report config");
    let mut world = AiWorld::new(config);
    world.add_enemy(
        Enemy::new(
            EnemyId(7),
            Position::new(0, 0),
            vec![
                Position::new(0, 0),
                Position::new(3, 0),
                Position::new(3, 3),
            ],
            1,
            5,
            10,
            50,
        )
        .expect("valid report enemy"),
    );

    println!(
        "ai/respawn report: tick_rate={}Hz respawn_delay={:?} ({} ticks)",
        config.tick_rate_hz, config.respawn_delay, config.respawn_delay_ticks
    );

    world.observe_player(PlayerObservation {
        id: PlayerId(2),
        position: Position::new(30, 30),
        alive: true,
    });
    advance_and_print(&mut world, 2);

    world.observe_player(PlayerObservation {
        id: PlayerId(2),
        position: Position::new(2, 0),
        alive: true,
    });
    advance_and_print(&mut world, 1);

    world.observe_player(PlayerObservation {
        id: PlayerId(2),
        position: Position::new(20, 20),
        alive: true,
    });
    advance_and_print(&mut world, 1);

    println!("returning from leash:");
    advance_and_print(&mut world, 5);

    println!("applying lethal damage:");
    world.damage_enemy(EnemyId(7), 50);
    print_events(world.drain_events());

    println!("waiting for scheduled respawn:");
    advance_and_print(&mut world, config.respawn_delay_ticks as usize);
}

fn advance_and_print(world: &mut AiWorld, ticks: usize) {
    for _ in 0..ticks {
        world.advance_tick();
        print_events(world.drain_events());
    }
}

fn print_events(events: Vec<AiEvent>) {
    for event in events {
        println!("  {event}");
    }
}
