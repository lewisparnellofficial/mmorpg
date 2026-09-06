use mmorpg_core::{Command, EntityId, Event, ItemId, QuestId, Role, World};
use mmorpg_wire::{
    ClientCommand as WireCommand, DecodeError as WireDecodeError, Envelope, MessageKind, decode_one,
};
use std::collections::VecDeque;
use std::env;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_ADDRESS: &str = "127.0.0.1:4000";
const DEFAULT_TICK_HZ: u64 = 20;
const MAX_WIRE_INPUT_BYTES: usize = mmorpg_wire::MAX_FRAME_SIZE * 2;
const MAX_WIRE_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug)]
struct Client {
    id: u64,
    stream: TcpStream,
    input: String,
    output: VecDeque<u8>,
    player_id: Option<EntityId>,
    closed: bool,
}

impl Client {
    fn new(id: u64, stream: TcpStream) -> Self {
        Self {
            id,
            stream,
            input: String::new(),
            output: VecDeque::new(),
            player_id: None,
            closed: false,
        }
    }

    fn queue_line(&mut self, line: impl AsRef<str>) {
        self.output.extend(line.as_ref().as_bytes());
        self.output.push_back(b'\n');
    }
}

#[derive(Debug)]
struct WireClient {
    id: u64,
    stream: TcpStream,
    input: Vec<u8>,
    output: VecDeque<u8>,
    player_id: Option<EntityId>,
    closed: bool,
}

impl WireClient {
    fn new(id: u64, stream: TcpStream) -> Self {
        Self {
            id,
            stream,
            input: Vec::new(),
            output: VecDeque::new(),
            player_id: None,
            closed: false,
        }
    }

    fn queue_event_payload(&mut self, payload: &[u8]) {
        let Ok(envelope) = Envelope::new(MessageKind::Event, payload.to_vec()) else {
            self.closed = true;
            return;
        };
        let Ok(frame) = envelope.encode() else {
            self.closed = true;
            return;
        };
        if self.output.len().saturating_add(frame.len()) > MAX_WIRE_OUTPUT_BYTES {
            self.closed = true;
            return;
        }
        self.output.extend(frame);
    }

    fn queue_event_text(&mut self, text: impl AsRef<str>) {
        self.queue_event_payload(text.as_ref().as_bytes());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClientOrigin {
    Line(u64),
    Wire(u64),
}

struct PendingCommand {
    origin: ClientOrigin,
    command: Command,
}

struct Server {
    world: World,
    clients: Vec<Client>,
    wire_clients: Vec<WireClient>,
    commands: VecDeque<PendingCommand>,
    next_client_id: u64,
    tick_interval: Duration,
    next_tick: Instant,
}

impl Server {
    fn new(tick_hz: u64) -> Self {
        let tick_hz = tick_hz.max(1);
        let tick_interval = Duration::from_secs_f64(1.0 / tick_hz as f64);
        Self {
            world: World::new_starter_zone(),
            clients: Vec::new(),
            wire_clients: Vec::new(),
            commands: VecDeque::new(),
            next_client_id: 1,
            tick_interval,
            next_tick: Instant::now() + tick_interval,
        }
    }

    fn add_client(&mut self, stream: TcpStream) {
        let id = self.next_client_id;
        self.next_client_id = self.next_client_id.saturating_add(1);
        let mut client = Client::new(id, stream);
        client.queue_line("WELCOME mmorpg-server");
        client.queue_line("TYPE help FOR COMMANDS");
        self.clients.push(client);
        println!("client_connected id={id}");
    }

    fn add_wire_client(&mut self, stream: TcpStream) {
        let id = self.next_client_id;
        self.next_client_id = self.next_client_id.saturating_add(1);
        let mut client = WireClient::new(id, stream);
        client.queue_event_text("WELCOME mmorpg-server");
        self.wire_clients.push(client);
        println!("wire_client_connected id={id}");
    }

    fn read_clients(&mut self) {
        let mut parsed = Vec::new();
        for client in &mut self.clients {
            if client.closed {
                continue;
            }
            let mut buffer = [0_u8; 4096];
            loop {
                match client.stream.read(&mut buffer) {
                    Ok(0) => {
                        client.closed = true;
                        break;
                    }
                    Ok(bytes_read) => {
                        client
                            .input
                            .push_str(&String::from_utf8_lossy(&buffer[..bytes_read]));
                        while let Some(newline) = client.input.find('\n') {
                            let line = client.input[..newline].trim_end_matches('\r').to_owned();
                            client.input.drain(..=newline);
                            parsed.push((client.id, line));
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("client_read_error id={} error={error}", client.id);
                        client.closed = true;
                        break;
                    }
                }
            }
        }

        for (client_id, line) in parsed {
            self.handle_line(client_id, &line);
        }
    }

    fn read_wire_clients(&mut self) {
        let mut commands = Vec::new();
        let mut errors = Vec::new();
        for client in &mut self.wire_clients {
            if client.closed {
                continue;
            }
            let mut buffer = [0_u8; 4096];
            loop {
                match client.stream.read(&mut buffer) {
                    Ok(0) => {
                        client.closed = true;
                        break;
                    }
                    Ok(bytes_read) => {
                        client.input.extend_from_slice(&buffer[..bytes_read]);
                        if client.input.len() > MAX_WIRE_INPUT_BYTES {
                            errors.push((
                                client.id,
                                "wire input buffer exceeded its limit".to_owned(),
                            ));
                            client.input.clear();
                            client.closed = true;
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("wire_client_read_error id={} error={error}", client.id);
                        client.closed = true;
                        break;
                    }
                }
            }

            loop {
                let decoded = match decode_one(&client.input) {
                    Ok(decoded) => decoded,
                    Err(WireDecodeError::Truncated { .. }) => break,
                    Err(error) => {
                        errors.push((client.id, format!("invalid wire frame: {error}")));
                        client.input.clear();
                        client.closed = true;
                        break;
                    }
                };
                let consumed = decoded.consumed;
                let kind = decoded.envelope.kind;
                let payload = decoded.envelope.payload;
                client.input.drain(..consumed);
                if kind != MessageKind::Command {
                    errors.push((
                        client.id,
                        format!("expected command envelope, received {kind:?}"),
                    ));
                    continue;
                }
                match WireCommand::decode_payload(&payload) {
                    Ok(command) => commands.push((client.id, command)),
                    Err(error) => errors.push((client.id, format!("invalid command: {error}"))),
                }
            }
        }

        for (client_id, error) in errors {
            self.queue_wire_error(client_id, error);
        }
        for (client_id, command) in commands {
            self.handle_wire_command(client_id, command);
        }
    }

    fn handle_wire_command(&mut self, client_id: u64, command: WireCommand) {
        let Some(client_index) = self
            .wire_clients
            .iter()
            .position(|client| client.id == client_id)
        else {
            return;
        };
        let bound_player = self.wire_clients[client_index].player_id;
        if let WireCommand::Join { name, role } = command {
            let already_pending = self.commands.iter().any(|pending| {
                pending.origin == ClientOrigin::Wire(client_id)
                    && matches!(pending.command, Command::JoinPlayer { .. })
            });
            if bound_player.is_some() || already_pending {
                self.queue_wire_error(client_id, "already connected".to_owned());
                return;
            }
            let role = match role {
                mmorpg_wire::RoleCode::Tank => Role::Tank,
                mmorpg_wire::RoleCode::Healer => Role::Healer,
                mmorpg_wire::RoleCode::DamageDealer => Role::DamageDealer,
            };
            self.commands.push_back(PendingCommand {
                origin: ClientOrigin::Wire(client_id),
                command: Command::JoinPlayer { name, role },
            });
            return;
        }
        if matches!(command, WireCommand::Snapshot) {
            self.send_wire_machine_snapshot(client_id);
            return;
        }
        let Some(player_id) = bound_player else {
            self.queue_wire_error(client_id, "connect first".to_owned());
            return;
        };
        let command = match wire_command_to_core(command, player_id) {
            Ok(command) => command,
            Err(error) => {
                self.queue_wire_error(client_id, error);
                return;
            }
        };
        self.commands.push_back(PendingCommand {
            origin: ClientOrigin::Wire(client_id),
            command,
        });
    }

    fn send_wire_machine_snapshot(&mut self, client_id: u64) {
        let lines = format_machine_snapshot(&self.world);
        if let Some(client) = self
            .wire_clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            for line in lines {
                client.queue_event_text(line);
            }
        }
    }

    fn queue_wire_error(&mut self, client_id: u64, error: String) {
        if let Some(client) = self
            .wire_clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            client.queue_event_text(format!("ERR {error}"));
        }
    }

    fn handle_line(&mut self, client_id: u64, line: &str) {
        let Some(client_index) = self
            .clients
            .iter()
            .position(|client| client.id == client_id)
        else {
            return;
        };
        match parse_line(line, self.clients[client_index].player_id) {
            Ok(ParsedLine::Command(command)) => {
                if matches!(command, Command::JoinPlayer { .. }) {
                    let already_pending = self.commands.iter().any(|pending| {
                        pending.origin == ClientOrigin::Line(client_id)
                            && matches!(pending.command, Command::JoinPlayer { .. })
                    });
                    if self.clients[client_index].player_id.is_some() || already_pending {
                        self.clients[client_index].queue_line("ERR already connected");
                        return;
                    }
                }
                self.commands.push_back(PendingCommand {
                    origin: ClientOrigin::Line(client_id),
                    command,
                });
            }
            Ok(ParsedLine::State) => self.send_state(client_id),
            Ok(ParsedLine::Snapshot) => self.send_machine_snapshot(client_id),
            Ok(ParsedLine::Inventory) => self.send_inventory(client_id),
            Ok(ParsedLine::Help) => self.send_help(client_id),
            Ok(ParsedLine::Quit) => {
                self.clients[client_index].closed = true;
                self.clients[client_index].queue_line("BYE");
            }
            Err(error) => self.clients[client_index].queue_line(format!("ERR {error}")),
        }
    }

    fn send_help(&mut self, client_id: u64) {
        let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        else {
            return;
        };
        client.queue_line("HELP connect <name> <tank|healer|damage>");
        client.queue_line("HELP move <dx> <dy>");
        client.queue_line("HELP target <entity-id>");
        client.queue_line("HELP attack");
        client.queue_line("HELP vendor <vendor-id>");
        client.queue_line("HELP buy <vendor-id> <item-id> <quantity>");
        client.queue_line("HELP loot <enemy-id>");
        client.queue_line("HELP inventory");
        client.queue_line("HELP quest-offers <npc-id>");
        client.queue_line("HELP accept-quest <npc-id> <quest-id>");
        client.queue_line("HELP turn-in-quest <npc-id> <quest-id>");
        client.queue_line("HELP state");
        client.queue_line("HELP snapshot");
        client.queue_line("HELP quit");
    }

    fn send_state(&mut self, client_id: u64) {
        if !self.clients.iter().any(|client| client.id == client_id) {
            return;
        }
        let mut lines = Vec::new();
        let summary = self.world.summary();
        lines.push(format!(
            "WORLD tick={} players={} npcs={} enemies={} vendors={}",
            summary.tick,
            summary.player_count,
            summary.npc_count,
            summary.enemy_count,
            summary.vendor_count
        ));
        for player in self.world.players() {
            lines.push(format!(
                "PLAYER id={} name={} role={} pos={:.2},{:.2} hp={}/{} gold={} target={}",
                player.id,
                player.name,
                player.role.as_str(),
                player.position.x,
                player.position.y,
                player.health,
                player.max_health,
                player.gold,
                player
                    .target
                    .map_or_else(|| "none".to_owned(), |target| target.to_string())
            ));
            for stack in player.inventory.stacks() {
                lines.push(format!(
                    "ITEM player={} item={} quantity={}",
                    player.id, stack.item_id, stack.quantity
                ));
            }
            for quest in &player.quests {
                lines.push(format!(
                    "QUEST player={} quest={} progress={}/{} status={:?}",
                    player.id, quest.quest_id, quest.progress, quest.required_count, quest.status
                ));
            }
        }
        for npc in self.world.npcs() {
            lines.push(format!(
                "NPC id={} name={} kind={:?} pos={:.2},{:.2} hp={}/{}",
                npc.id,
                npc.name,
                npc.kind,
                npc.position.x,
                npc.position.y,
                npc.health,
                npc.max_health
            ));
        }
        if let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            for line in lines {
                client.queue_line(line);
            }
        }
    }

    fn send_inventory(&mut self, client_id: u64) {
        let Some(player_id) = self
            .clients
            .iter()
            .find(|client| client.id == client_id)
            .and_then(|client| client.player_id)
        else {
            if let Some(client) = self
                .clients
                .iter_mut()
                .find(|client| client.id == client_id)
            {
                client.queue_line("ERR connect first");
            }
            return;
        };

        let Some(player) = self.world.players().find(|player| player.id == player_id) else {
            return;
        };
        let lines: Vec<_> = std::iter::once(format!(
            "INVENTORY player={} gold={} slots={}/{}",
            player.id,
            player.gold,
            player.inventory.used_slots(),
            player.inventory.capacity()
        ))
        .chain(player.inventory.stacks().map(|stack| {
            format!(
                "ITEM player={} item={} quantity={}",
                player.id, stack.item_id, stack.quantity
            )
        }))
        .collect();
        if let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            for line in lines {
                client.queue_line(line);
            }
        }
    }

    /// Sends the temporary machine-readable bootstrap snapshot. This is kept
    /// separate from `state` so existing terminal users retain the current
    /// human-readable output while the graphical client has a deterministic
    /// response to parse.
    fn send_machine_snapshot(&mut self, client_id: u64) {
        let lines = format_machine_snapshot(&self.world);
        if let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            for line in lines {
                client.queue_line(line);
            }
        }
    }

    fn advance_if_due(&mut self) {
        if Instant::now() < self.next_tick {
            return;
        }

        let pending: Vec<_> = self.commands.drain(..).collect();
        let mut join_origins: VecDeque<ClientOrigin> = pending
            .iter()
            .filter_map(|pending| {
                matches!(pending.command, Command::JoinPlayer { .. }).then_some(pending.origin)
            })
            .collect();
        let events = self
            .world
            .step(pending.into_iter().map(|pending| pending.command));

        // The development protocol only accepts valid join commands, so join
        // events correspond in order to the pending join origins.
        for event in &events {
            if let Event::PlayerJoined { player } = event
                && let Some(origin) = join_origins.pop_front()
            {
                match origin {
                    ClientOrigin::Line(origin) => {
                        if let Some(client) =
                            self.clients.iter_mut().find(|client| client.id == origin)
                        {
                            client.player_id = Some(player.id);
                            client.queue_line(format!(
                                "CONNECTED player_id={} role={}",
                                player.id,
                                player.role.as_str()
                            ));
                        }
                    }
                    ClientOrigin::Wire(origin) => {
                        if let Some(client) = self
                            .wire_clients
                            .iter_mut()
                            .find(|client| client.id == origin)
                        {
                            client.player_id = Some(player.id);
                            client.queue_event_text(format!(
                                "CONNECTED player_id={} role={}",
                                player.id,
                                player.role.as_str()
                            ));
                        }
                    }
                }
            }
        }

        for event in events {
            self.broadcast(format_event(&event));
        }

        self.next_tick += self.tick_interval;
        if self.next_tick <= Instant::now() {
            self.next_tick = Instant::now() + self.tick_interval;
        }
    }

    fn queue_disconnects(&mut self) {
        for client in &mut self.clients {
            if client.closed
                && let Some(player_id) = client.player_id.take()
            {
                self.commands.push_back(PendingCommand {
                    origin: ClientOrigin::Line(client.id),
                    command: Command::LeavePlayer { player_id },
                });
            }
        }
        for client in &mut self.wire_clients {
            if client.closed
                && let Some(player_id) = client.player_id.take()
            {
                self.commands.push_back(PendingCommand {
                    origin: ClientOrigin::Wire(client.id),
                    command: Command::LeavePlayer { player_id },
                });
            }
        }
    }

    fn flush_clients(&mut self) {
        for client in &mut self.clients {
            while !client.output.is_empty() {
                let chunk: Vec<u8> = client.output.iter().copied().take(8192).collect();
                match client.stream.write(&chunk) {
                    Ok(0) => {
                        client.closed = true;
                        break;
                    }
                    Ok(bytes_written) => {
                        client.output.drain(..bytes_written);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("client_write_error id={} error={error}", client.id);
                        client.closed = true;
                        break;
                    }
                }
            }
        }
        for client in &mut self.wire_clients {
            while !client.output.is_empty() {
                let chunk: Vec<u8> = client.output.iter().copied().take(8192).collect();
                match client.stream.write(&chunk) {
                    Ok(0) => {
                        client.closed = true;
                        break;
                    }
                    Ok(bytes_written) => {
                        client.output.drain(..bytes_written);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("wire_client_write_error id={} error={error}", client.id);
                        client.closed = true;
                        break;
                    }
                }
            }
        }
    }

    fn remove_closed(&mut self) {
        self.clients
            .retain(|client| !client.closed || !client.output.is_empty());
        self.wire_clients
            .retain(|client| !client.closed || !client.output.is_empty());
    }

    fn broadcast(&mut self, line: String) {
        for client in &mut self.clients {
            if !client.closed {
                client.queue_line(&line);
            }
        }
        for client in &mut self.wire_clients {
            if !client.closed {
                client.queue_event_text(&line);
            }
        }
    }
}

fn format_event(event: &Event) -> String {
    match event {
        Event::PlayerJoined { player } => format!(
            "EVENT player_joined id={} name={} role={} pos={:.2},{:.2}",
            player.id,
            player.name,
            player.role.as_str(),
            player.position.x,
            player.position.y
        ),
        Event::PlayerLeft { player_id } => format!("EVENT player_left id={player_id}"),
        Event::PlayerMoved {
            player_id,
            position,
            area,
        } => format!(
            "EVENT player_moved id={} pos={:.2},{:.2} area={area:?}",
            player_id, position.x, position.y
        ),
        Event::TargetSelected {
            player_id,
            target_id,
        } => {
            format!("EVENT target_selected player={player_id} target={target_id}")
        }
        Event::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        } => format!(
            "EVENT attack player={} target={} damage={} target_hp={}",
            player_id, target_id, damage, target_health
        ),
        Event::EnemyDefeated { enemy_id } => format!("EVENT enemy_defeated id={enemy_id}"),
        Event::VendorListed {
            player_id,
            vendor_id,
            listings,
        } => {
            let listing_text = listings
                .iter()
                .map(|listing| {
                    format!(
                        "item={} name={} price={} stock={} max_stack={}",
                        listing.item_id,
                        listing.name.replace(' ', "_"),
                        listing.unit_price,
                        listing.remaining_quantity,
                        listing.max_stack
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            format!(
                "EVENT vendor_listed player={} vendor={} listings={listing_text}",
                player_id, vendor_id
            )
        }
        Event::ItemPurchased {
            player_id,
            vendor_id,
            item_id,
            quantity,
            total_price,
            gold_remaining,
        } => format!(
            "EVENT item_purchased player={} vendor={} item={} quantity={} total_price={} gold={}",
            player_id, vendor_id, item_id, quantity, total_price, gold_remaining
        ),
        Event::LootRewarded {
            player_id,
            enemy_id,
            item_id,
            quantity,
        } => format!(
            "EVENT loot_rewarded player={} enemy={} item={} quantity={}",
            player_id, enemy_id, item_id, quantity
        ),
        Event::TransactionRejected { player_id, reason } => format!(
            "EVENT transaction_rejected player={} reason={reason}",
            player_id
        ),
        Event::QuestOffersListed {
            player_id,
            npc_id,
            quests,
        } => {
            let quest_text = quests
                .iter()
                .map(|quest| {
                    format!(
                        "id={} name={}",
                        quest.quest_id,
                        quest.name.replace(' ', "_")
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            format!(
                "EVENT quest_offers player={} npc={} quests={quest_text}",
                player_id, npc_id
            )
        }
        Event::QuestAccepted {
            player_id,
            npc_id,
            quest_id,
        } => format!(
            "EVENT quest_accepted player={} npc={} quest={}",
            player_id, npc_id, quest_id
        ),
        Event::QuestProgressed {
            player_id,
            quest_id,
            progress,
            required_count,
        } => format!(
            "EVENT quest_progressed player={} quest={} progress={}/{}",
            player_id, quest_id, progress, required_count
        ),
        Event::QuestCompleted {
            player_id,
            quest_id,
        } => format!(
            "EVENT quest_completed player={} quest={}",
            player_id, quest_id
        ),
        Event::QuestRewarded {
            player_id,
            quest_id,
            gold,
            item_id,
            item_quantity,
            gold_remaining,
        } => format!(
            "EVENT quest_rewarded player={} quest={} gold={} item={} quantity={} gold_remaining={}",
            player_id,
            quest_id,
            gold,
            item_id.map_or_else(|| "none".to_owned(), |item| item.to_string()),
            item_quantity,
            gold_remaining
        ),
        Event::QuestRejected { player_id, reason } => {
            format!("EVENT quest_rejected player={} reason={reason}", player_id)
        }
        Event::CommandRejected { reason } => format!("EVENT rejected reason={reason}"),
    }
}

/// Formats the temporary graphical-client bootstrap response.
///
/// The `TEMP_SNAPSHOT` prefix is deliberately not part of the future wire
/// protocol. Records are whitespace-delimited key/value pairs, and text
/// values use percent encoding so names containing spaces cannot change the
/// record shape. Positions use Rust's shortest round-trippable float format.
fn format_machine_snapshot(world: &World) -> Vec<String> {
    let summary = world.summary();
    let mut lines = vec!["TEMP_SNAPSHOT_BEGIN version=1".to_owned()];
    lines.push(format!(
        "TEMP_SNAPSHOT WORLD tick={} players={} npcs={} enemies={} vendors={}",
        summary.tick,
        summary.player_count,
        summary.npc_count,
        summary.enemy_count,
        summary.vendor_count
    ));
    for player in world.players() {
        lines.push(format!(
            "TEMP_SNAPSHOT PLAYER id={} name={} role={} position={:?},{:?} health={} max_health={} gold={} target={}",
            player.id,
            encode_snapshot_text(&player.name),
            player.role.as_str(),
            player.position.x,
            player.position.y,
            player.health,
            player.max_health,
            player.gold,
            player
                .target
                .map_or_else(|| "none".to_owned(), |target| target.to_string())
        ));
        for stack in player.inventory.stacks() {
            lines.push(format!(
                "TEMP_SNAPSHOT ITEM player={} item={} quantity={}",
                player.id, stack.item_id, stack.quantity
            ));
        }
        for quest in &player.quests {
            lines.push(format!(
                "TEMP_SNAPSHOT QUEST player={} quest={} progress={}/{} status={:?}",
                player.id, quest.quest_id, quest.progress, quest.required_count, quest.status
            ));
        }
    }
    for npc in world.npcs() {
        lines.push(format!(
            "TEMP_SNAPSHOT NPC id={} template_id={} name={} kind={} position={:?},{:?} health={} max_health={}",
            npc.id,
            npc.template_id,
            encode_snapshot_text(&npc.name),
            machine_npc_kind(npc.kind),
            npc.position.x,
            npc.position.y,
            npc.health,
            npc.max_health,
        ));
    }
    lines.push("TEMP_SNAPSHOT_END".to_owned());
    lines
}

fn machine_npc_kind(kind: mmorpg_core::NpcKind) -> &'static str {
    match kind {
        mmorpg_core::NpcKind::Vendor => "vendor",
        mmorpg_core::NpcKind::Enemy => "enemy",
    }
}

fn encode_snapshot_text(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

enum ParsedLine {
    Command(Command),
    State,
    Snapshot,
    Inventory,
    Help,
    Quit,
}

fn parse_line(line: &str, bound_player: Option<EntityId>) -> Result<ParsedLine, String> {
    let tokens: Vec<_> = line.split_whitespace().collect();
    let Some(command) = tokens.first().copied() else {
        return Err("empty command".to_owned());
    };
    match command.to_ascii_lowercase().as_str() {
        "connect" => {
            if tokens.len() != 3 {
                return Err("usage: connect <name> <tank|healer|damage>".to_owned());
            }
            let role = tokens[2]
                .parse::<Role>()
                .map_err(|error| error.to_string())?;
            Ok(ParsedLine::Command(Command::JoinPlayer {
                name: tokens[1].to_owned(),
                role,
            }))
        }
        "move" => {
            let (dx, dy) = match tokens.len() {
                3 => (tokens[1], tokens[2]),
                4 => {
                    ensure_bound_id(tokens[1], bound_player)?;
                    (tokens[2], tokens[3])
                }
                _ => return Err("usage: move <dx> <dy>".to_owned()),
            };
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::Move {
                player_id,
                dx: dx.parse().map_err(|_| "dx must be a number".to_owned())?,
                dy: dy.parse().map_err(|_| "dy must be a number".to_owned())?,
            }))
        }
        "target" => {
            if tokens.len() != 2 {
                return Err("usage: target <entity-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::SelectTarget {
                player_id,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "attack" => {
            if tokens.len() != 1 {
                return Err("usage: attack".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::BasicAttack { player_id }))
        }
        "vendor" | "list-vendor" => {
            if tokens.len() != 2 {
                return Err("usage: vendor <vendor-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::ListVendor {
                player_id,
                vendor_id: parse_entity_id(tokens[1])?,
            }))
        }
        "buy" => {
            if tokens.len() != 4 {
                return Err("usage: buy <vendor-id> <item-id> <quantity>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::BuyItem {
                player_id,
                vendor_id: parse_entity_id(tokens[1])?,
                item_id: parse_item_id(tokens[2])?,
                quantity: parse_positive_quantity(tokens[3])?,
            }))
        }
        "loot" => {
            if tokens.len() != 2 {
                return Err("usage: loot <enemy-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::LootEnemy {
                player_id,
                enemy_id: parse_entity_id(tokens[1])?,
            }))
        }
        "inventory" => {
            if tokens.len() != 1 {
                return Err("usage: inventory".to_owned());
            }
            Ok(ParsedLine::Inventory)
        }
        "quest-offers" | "quests" => {
            if tokens.len() != 2 {
                return Err("usage: quest-offers <npc-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::ListQuestOffers {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
            }))
        }
        "accept-quest" => {
            if tokens.len() != 3 {
                return Err("usage: accept-quest <npc-id> <quest-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::AcceptQuest {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
                quest_id: parse_quest_id(tokens[2])?,
            }))
        }
        "turn-in-quest" => {
            if tokens.len() != 3 {
                return Err("usage: turn-in-quest <npc-id> <quest-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::TurnInQuest {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
                quest_id: parse_quest_id(tokens[2])?,
            }))
        }
        "state" => Ok(ParsedLine::State),
        "snapshot" => Ok(ParsedLine::Snapshot),
        "help" => Ok(ParsedLine::Help),
        "quit" | "exit" => Ok(ParsedLine::Quit),
        _ => Err(format!("unknown command '{command}'; try help")),
    }
}

fn ensure_bound_id(value: &str, bound_player: Option<EntityId>) -> Result<(), String> {
    let requested = parse_entity_id(value)?;
    if Some(requested) != bound_player {
        return Err("a client may only control its own player".to_owned());
    }
    Ok(())
}

fn parse_entity_id(value: &str) -> Result<EntityId, String> {
    value
        .parse::<u64>()
        .map(EntityId)
        .map_err(|_| "entity-id must be an integer".to_owned())
}

fn parse_item_id(value: &str) -> Result<ItemId, String> {
    value
        .parse::<u32>()
        .map(ItemId)
        .map_err(|_| "item-id must be an integer".to_owned())
}

fn parse_quest_id(value: &str) -> Result<QuestId, String> {
    value
        .parse::<u32>()
        .map(QuestId)
        .map_err(|_| "quest-id must be an integer".to_owned())
}

fn parse_positive_quantity(value: &str) -> Result<u32, String> {
    let quantity = value
        .parse::<u32>()
        .map_err(|_| "quantity must be a positive integer".to_owned())?;
    if quantity == 0 {
        return Err("quantity must be a positive integer".to_owned());
    }
    Ok(quantity)
}

fn wire_command_to_core(command: WireCommand, player_id: EntityId) -> Result<Command, String> {
    Ok(match command {
        WireCommand::Move { dx, dy } => Command::Move { player_id, dx, dy },
        WireCommand::SelectTarget { target_id } => Command::SelectTarget {
            player_id,
            target_id: EntityId(target_id),
        },
        WireCommand::BasicAttack => Command::BasicAttack { player_id },
        WireCommand::ListVendor { vendor_id } => Command::ListVendor {
            player_id,
            vendor_id: EntityId(vendor_id),
        },
        WireCommand::BuyItem {
            vendor_id,
            item_id,
            quantity,
        } => Command::BuyItem {
            player_id,
            vendor_id: EntityId(vendor_id),
            item_id: ItemId(item_id),
            quantity,
        },
        WireCommand::LootEnemy { enemy_id } => Command::LootEnemy {
            player_id,
            enemy_id: EntityId(enemy_id),
        },
        WireCommand::ListQuestOffers { npc_id } => Command::ListQuestOffers {
            player_id,
            npc_id: EntityId(npc_id),
        },
        WireCommand::AcceptQuest { npc_id, quest_id } => Command::AcceptQuest {
            player_id,
            npc_id: EntityId(npc_id),
            quest_id: QuestId(quest_id),
        },
        WireCommand::TurnInQuest { npc_id, quest_id } => Command::TurnInQuest {
            player_id,
            npc_id: EntityId(npc_id),
            quest_id: QuestId(quest_id),
        },
        WireCommand::Join { .. } | WireCommand::Snapshot => {
            return Err("command is not valid in a bound session".to_owned());
        }
    })
}

fn parse_server_addresses() -> Result<(String, Option<String>), String> {
    let mut arguments = env::args().skip(1);
    let address = arguments
        .next()
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let mut wire_address = None;
    while let Some(argument) = arguments.next() {
        if argument != "--wire-address" {
            return Err(format!("unknown argument '{argument}'"));
        }
        if wire_address.is_some() {
            return Err("--wire-address may only be specified once".to_owned());
        }
        wire_address = Some(
            arguments
                .next()
                .ok_or_else(|| "--wire-address requires an address".to_owned())?,
        );
    }
    Ok((address, wire_address))
}

fn main() -> io::Result<()> {
    let (address, wire_address) = parse_server_addresses()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let listener = TcpListener::bind(&address)?;
    listener.set_nonblocking(true)?;
    let wire_listener = wire_address.as_deref().map(TcpListener::bind).transpose()?;
    if let Some(listener) = &wire_listener {
        listener.set_nonblocking(true)?;
    }
    let mut server = Server::new(DEFAULT_TICK_HZ);
    println!("server_listening address={address} tick_hz={DEFAULT_TICK_HZ}");
    if let Some(address) = wire_address {
        println!("wire_server_listening address={address}");
    }

    loop {
        loop {
            match listener.accept() {
                Ok((stream, peer)) => {
                    stream.set_nonblocking(true)?;
                    println!("accepted peer={peer}");
                    server.add_client(stream);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        if let Some(listener) = &wire_listener {
            loop {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        stream.set_nonblocking(true)?;
                        println!("accepted_wire_peer={peer}");
                        server.add_wire_client(stream);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error),
                }
            }
        }

        server.read_clients();
        server.read_wire_clients();
        server.queue_disconnects();
        server.advance_if_due();
        server.flush_clients();
        server.remove_closed();
        thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_protocol_requires_a_bound_player_for_gameplay() {
        assert!(parse_line("move 1 0", None).is_err());
        assert!(parse_line("attack", None).is_err());
        assert!(parse_line("connect Alice tank", None).is_ok());
    }

    #[test]
    fn development_protocol_only_allows_a_client_to_move_its_own_player() {
        let own = Some(EntityId(7));
        assert!(parse_line("move 7 1 0", own).is_ok());
        assert!(parse_line("move 8 1 0", own).is_err());
    }

    #[test]
    fn development_protocol_parses_economy_commands_for_bound_player() {
        let own = Some(EntityId(7));
        assert_eq!(
            parse_line("vendor 1", own).unwrap_command(),
            Command::ListVendor {
                player_id: EntityId(7),
                vendor_id: EntityId(1),
            }
        );
        assert_eq!(
            parse_line("buy 1 2 3", own).unwrap_command(),
            Command::BuyItem {
                player_id: EntityId(7),
                vendor_id: EntityId(1),
                item_id: ItemId(2),
                quantity: 3,
            }
        );
        assert_eq!(
            parse_line("loot 12", own).unwrap_command(),
            Command::LootEnemy {
                player_id: EntityId(7),
                enemy_id: EntityId(12),
            }
        );
        assert!(parse_line("buy 1 2 0", own).is_err());
        assert!(parse_line("loot 12", None).is_err());
    }

    #[test]
    fn development_protocol_parses_quest_commands_for_bound_player() {
        let own = Some(EntityId(7));
        assert_eq!(
            parse_line("quest-offers 1", own).unwrap_command(),
            Command::ListQuestOffers {
                player_id: EntityId(7),
                npc_id: EntityId(1),
            }
        );
        assert_eq!(
            parse_line("accept-quest 1 1", own).unwrap_command(),
            Command::AcceptQuest {
                player_id: EntityId(7),
                npc_id: EntityId(1),
                quest_id: QuestId(1),
            }
        );
        assert_eq!(
            parse_line("turn-in-quest 1 1", own).unwrap_command(),
            Command::TurnInQuest {
                player_id: EntityId(7),
                npc_id: EntityId(1),
                quest_id: QuestId(1),
            }
        );
        assert!(parse_line("accept-quest 1 nope", own).is_err());
    }

    trait ParsedLineExt {
        fn unwrap_command(self) -> Command;
    }

    impl ParsedLineExt for Result<ParsedLine, String> {
        fn unwrap_command(self) -> Command {
            match self.expect("expected a parsed command") {
                ParsedLine::Command(command) => command,
                ParsedLine::State | ParsedLine::Inventory | ParsedLine::Help | ParsedLine::Quit => {
                    panic!("expected a command")
                }
                ParsedLine::Snapshot => panic!("expected a command"),
            }
        }
    }

    #[test]
    fn event_format_is_human_readable() {
        let event = Event::EnemyDefeated {
            enemy_id: EntityId(9),
        };
        assert_eq!(format_event(&event), "EVENT enemy_defeated id=9");
    }

    #[test]
    fn snapshot_command_is_available_without_a_bound_player() {
        assert!(matches!(
            parse_line("snapshot", None),
            Ok(ParsedLine::Snapshot)
        ));
    }

    #[test]
    fn machine_snapshot_has_stable_bootstrap_records() {
        let mut world = World::new_starter_zone();
        world.step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }]);

        assert_eq!(
            format_machine_snapshot(&world),
            vec![
                "TEMP_SNAPSHOT_BEGIN version=1",
                "TEMP_SNAPSHOT WORLD tick=1 players=1 npcs=4 enemies=3 vendors=1",
                "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=0.0,0.0 health=100 max_health=100 gold=20 target=none",
                "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira%20the%20Merchant kind=vendor position=0.0,0.0 health=1 max_health=1",
                "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Field%20Wolf kind=enemy position=24.0,0.0 health=100 max_health=100",
                "TEMP_SNAPSHOT NPC id=3 template_id=2 name=Field%20Wolf kind=enemy position=30.0,6.0 health=100 max_health=100",
                "TEMP_SNAPSHOT NPC id=4 template_id=2 name=Field%20Wolf kind=enemy position=30.0,-6.0 health=100 max_health=100",
                "TEMP_SNAPSHOT_END",
            ]
        );
    }

    #[test]
    fn machine_snapshot_includes_player_inventory_and_quest_state() {
        let mut world = World::new_starter_zone();
        world.step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }]);
        let player_id = world.players().next().expect("player joined").id;
        let vendor_id = world
            .npcs()
            .find(|npc| npc.kind == mmorpg_core::NpcKind::Vendor)
            .expect("starter vendor exists")
            .id;

        world.step([Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId::TOWN_RATION,
            quantity: 2,
        }]);
        world.step([Command::AcceptQuest {
            player_id,
            npc_id: vendor_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);

        let lines = format_machine_snapshot(&world);
        assert!(
            lines
                .iter()
                .any(|line| { line == "TEMP_SNAPSHOT ITEM player=5 item=2 quantity=2" })
        );
        assert!(lines.iter().any(|line| {
            line == "TEMP_SNAPSHOT QUEST player=5 quest=1 progress=0/3 status=Accepted"
        }));
    }

    #[test]
    fn machine_snapshot_text_encoding_preserves_record_boundaries() {
        assert_eq!(
            encode_snapshot_text("Name with\tcontrols%"),
            "Name%20with%09controls%25"
        );
    }
}
