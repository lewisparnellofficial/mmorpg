use mmorpg_core::{Command, EntityId, Event, ItemId, Role};
use mmorpg_server::{
    AccountCharacterRepository, AuthenticatedSession, ClientOrigin, DEFAULT_TICK_HZ,
    DevelopmentAccountRepository, MAX_COMPLETED_OPERATIONS, OperationJournalJob,
    OperationJournalWorker, OperationKey, PendingCommand, Server, WireClient,
};
use mmorpg_wire::ClientCommand as WireCommand;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn operation_outcome_retention_is_bounded_across_success_and_failure() {
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    for operation_id in 1..=(MAX_COMPLETED_OPERATIONS as u64 + 32) {
        server.failed_operations.insert(
            OperationKey {
                account_id: 1,
                character_id: 1,
                operation_id,
            },
            "rejected".to_owned(),
        );
    }
    server.trim_operation_results();
    assert_eq!(
        server.completed_operations.len() + server.failed_operations.len(),
        MAX_COMPLETED_OPERATIONS
    );
    assert!(!server.failed_operations.contains_key(&OperationKey {
        account_id: 1,
        character_id: 1,
        operation_id: 1,
    }));
}

#[test]
fn interrupted_prepared_operation_is_failed_before_retry_after_restart() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let checkpoint_path =
        std::env::temp_dir().join(format!("mmorpg-interrupted-operation-{unique}.state"));
    let journal_path = checkpoint_path.with_extension("operations");
    let key = OperationKey {
        account_id: 1,
        character_id: 1,
        operation_id: 102,
    };
    let (worker, _) =
        OperationJournalWorker::new(journal_path.clone()).expect("journal should start");
    worker
        .try_enqueue(OperationJournalJob::Prepare {
            key,
            command_payload: vec![0x01],
        })
        .expect("prepared operation should enter bounded queue");
    let mut prepared = false;
    for _ in 0..100 {
        if worker.drain_results().any(|result| result.result.is_ok()) {
            prepared = true;
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(prepared, "journal should report prepared record");
    drop(worker);

    let server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    assert_eq!(
        server.failed_operations.get(&key),
        Some(&"operation was interrupted before durable completion".to_owned())
    );
    drop(server);

    let restarted = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    assert_eq!(
        restarted.failed_operations.get(&key),
        Some(&"operation was interrupted before durable completion".to_owned())
    );
    drop(restarted);
    let _ = std::fs::remove_file(checkpoint_path);
    let _ = std::fs::remove_file(journal_path);
}

#[test]
fn retryable_durable_command_returns_cached_result_without_reapplying() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    let player_id = server
        .world
        .step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }])
        .into_iter()
        .find_map(|event| match event {
            Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("test player should join");
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 1,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    client.player_id = Some(player_id);
    server.wire_clients.push(client);
    let key = OperationKey {
        account_id: 1,
        character_id: 1,
        operation_id: 77,
    };
    server.commands.push_back(PendingCommand {
        origin: ClientOrigin::Wire(1),
        command: Command::BuyItem {
            player_id,
            vendor_id: EntityId(1),
            item_id: ItemId::TOWN_RATION,
            quantity: 1,
        },
        operation: Some(key),
    });
    server.next_tick = Instant::now() - Duration::from_millis(1);
    server.advance_if_due();
    let gold_after_first_apply = server.world.player(player_id).unwrap().gold;
    assert!(server.completed_operations.contains_key(&key));

    server.handle_wire_command(
        1,
        WireCommand::Retryable {
            operation_id: 77,
            command: Box::new(WireCommand::BuyItem {
                vendor_id: 1,
                item_id: ItemId::TOWN_RATION.0,
                quantity: 1,
            }),
        },
    );

    assert_eq!(
        server.world.player(player_id).unwrap().gold,
        gold_after_first_apply
    );
    assert!(server.commands.is_empty());
    assert!(!server.wire_clients[0].output.is_empty());
}

#[test]
fn journaled_operation_stages_world_before_commit_and_reloads_after_restart() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let checkpoint_path = std::env::temp_dir().join(format!("mmorpg-staged-{unique}.state"));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    let player_id = server
        .world
        .step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }])
        .into_iter()
        .find_map(|event| match event {
            Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("test player should join");
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 1,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    client.player_id = Some(player_id);
    server.wire_clients.push(client);
    let initial_gold = server.world.player(player_id).unwrap().gold;
    let key = OperationKey {
        account_id: 1,
        character_id: 1,
        operation_id: 700,
    };

    server.handle_wire_command(
        1,
        WireCommand::Retryable {
            operation_id: key.operation_id,
            command: Box::new(WireCommand::BuyItem {
                vendor_id: 1,
                item_id: ItemId::TOWN_RATION.0,
                quantity: 1,
            }),
        },
    );

    let mut applied = false;
    for _ in 0..200 {
        server.next_tick = Instant::now() - Duration::from_millis(1);
        server.advance_if_due();
        if server.completed_operations.contains_key(&key) {
            applied = true;
            break;
        }
        assert_eq!(server.world.player(player_id).unwrap().gold, initial_gold);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(applied, "journaled operation should eventually commit");
    assert!(server.world.player(player_id).unwrap().gold < initial_gold);
    drop(server);

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind restart listener");
    let address = listener.local_addr().expect("restart listener address");
    let _peer = TcpStream::connect(address).expect("connect restart peer");
    let (stream, _) = listener.accept().expect("accept restart peer");
    let mut restarted = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    assert!(restarted.completed_operations.contains_key(&key));
    assert!(restarted.recovery_operations.contains_key(&key));
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 2,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    restarted.wire_clients.push(client);
    restarted.handle_wire_command(1, WireCommand::EnterWorld);
    assert!(restarted.pending_recovery_operations.contains_key(&(1, 1)));
    assert!(!restarted.commands.is_empty());
    restarted.next_tick = Instant::now() - Duration::from_millis(1);
    restarted.advance_if_due();
    let restarted_player = restarted.wire_clients[0]
        .player_id
        .expect("restart enter-world should bind a player");
    assert!(restarted.world.player(restarted_player).is_some());
    assert!(!restarted.commands.is_empty());
    for _ in 0..200 {
        restarted.next_tick = Instant::now() - Duration::from_millis(1);
        restarted.advance_if_due();
        if restarted
            .world
            .player(restarted_player)
            .is_some_and(|player| player.gold < initial_gold)
        {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        restarted
            .world
            .player(restarted_player)
            .is_some_and(|player| player.gold < initial_gold),
        "an older checkpoint should trigger one operation replay"
    );
    drop(restarted);
    let _ = std::fs::remove_file(checkpoint_path);
    let _ = std::fs::remove_file(
        std::env::temp_dir().join(format!("mmorpg-staged-{unique}.operations")),
    );
}

#[test]
fn rejected_retryable_wrapper_is_journaled_and_replayed_after_restart() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let checkpoint_path =
        std::env::temp_dir().join(format!("mmorpg-failed-operation-{unique}.state"));
    let journal_path = checkpoint_path.with_extension("operations");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    let player_id = server
        .world
        .step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }])
        .into_iter()
        .find_map(|event| match event {
            Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("test player should join");
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 1,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    client.player_id = Some(player_id);
    server.wire_clients.push(client);
    let key = OperationKey {
        account_id: 1,
        character_id: 1,
        operation_id: 808,
    };

    server.handle_wire_command(
        1,
        WireCommand::Retryable {
            operation_id: key.operation_id,
            command: Box::new(WireCommand::Move { dx: 1.0, dy: 0.0 }),
        },
    );

    for _ in 0..200 {
        server.next_tick = Instant::now() - Duration::from_millis(1);
        server.advance_if_due();
        if server.failed_operations.contains_key(&key) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    let reason = server
        .failed_operations
        .get(&key)
        .cloned()
        .expect("rejected retryable operation should be journaled");
    assert_eq!(
        reason,
        "operation wrapper is only valid for durable commands"
    );
    drop(server);

    let restarted = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    assert_eq!(restarted.failed_operations.get(&key), Some(&reason));
    let _ = std::fs::remove_file(checkpoint_path);
    let _ = std::fs::remove_file(journal_path);
}

#[test]
fn journal_completion_failure_discards_staged_world_without_publishing_success() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let checkpoint_path =
        std::env::temp_dir().join(format!("mmorpg-journal-failure-{unique}.state"));
    let journal_path = checkpoint_path.with_extension("operations");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    let player_id = server
        .world
        .step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }])
        .into_iter()
        .find_map(|event| match event {
            Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("test player should join");
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 1,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    client.player_id = Some(player_id);
    server.wire_clients.push(client);
    let initial_gold = server.world.player(player_id).unwrap().gold;
    let key = OperationKey {
        account_id: 1,
        character_id: 1,
        operation_id: 701,
    };

    std::fs::create_dir(&journal_path).expect("journal failure directory should be created");
    server.handle_wire_command(
        1,
        WireCommand::Retryable {
            operation_id: key.operation_id,
            command: Box::new(WireCommand::BuyItem {
                vendor_id: 1,
                item_id: ItemId::TOWN_RATION.0,
                quantity: 1,
            }),
        },
    );

    for _ in 0..200 {
        server.next_tick = Instant::now() - Duration::from_millis(1);
        server.advance_if_due();
        if server.staged_operation_batch.is_none()
            && !server.prepared_operations.contains_key(&key)
            && server.commands.is_empty()
        {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    assert!(server.staged_operation_batch.is_none());
    assert!(!server.completed_operations.contains_key(&key));
    assert_eq!(server.world.player(player_id).unwrap().gold, initial_gold);
    drop(server);
    let _ = std::fs::remove_file(checkpoint_path);
    std::fs::remove_dir(journal_path).expect("journal failure directory should be removed");
}

#[test]
fn shutdown_drains_commands_and_checkpoints_before_releasing_players() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let checkpoint_path = std::env::temp_dir().join(format!("mmorpg-shutdown-{unique}.state"));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
    let player_id = server
        .world
        .step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }])
        .into_iter()
        .find_map(|event| match event {
            Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("test player should join");
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 1,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    client.player_id = Some(player_id);
    server.wire_clients.push(client);
    server.commands.push_back(PendingCommand {
        origin: ClientOrigin::Wire(1),
        command: Command::Move {
            player_id,
            dx: 2.0,
            dy: 0.0,
        },
        operation: None,
    });
    let failed_key = OperationKey {
        account_id: 1,
        character_id: 1,
        operation_id: 900,
    };
    let failed_reason = "shutdown drain test".to_owned();
    server
        .pending_failed_operations
        .insert(failed_key, (1, failed_reason.clone()));
    server
        .operation_journal_worker
        .try_enqueue(OperationJournalJob::Failed {
            key: failed_key,
            reason: failed_reason,
        })
        .expect("failed operation should be queued for shutdown drain");

    server.shutdown();
    assert!(server.commands.is_empty());
    assert!(server.pending_failed_operations.is_empty());
    assert!(server.failed_operations.contains_key(&failed_key));
    assert!(server.world.player(player_id).is_none());
    drop(server);

    let repository =
        DevelopmentAccountRepository::with_checkpoint_store(true, checkpoint_path.clone());
    assert!(
        repository
            .load_checkpoint(1, 1)
            .expect("shutdown checkpoint should load")
            .is_some()
    );
    let _ = std::fs::remove_file(checkpoint_path);
    let _ = std::fs::remove_file(
        std::env::temp_dir().join(format!("mmorpg-shutdown-{unique}.operations")),
    );
}
