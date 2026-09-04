use mmorpg_core::{Command, EntityId, Event, Role, World};
use std::collections::VecDeque;
use std::env;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_ADDRESS: &str = "127.0.0.1:4000";
const DEFAULT_TICK_HZ: u64 = 20;

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

struct PendingCommand {
    origin: u64,
    command: Command,
}

struct Server {
    world: World,
    clients: Vec<Client>,
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
                        pending.origin == client_id
                            && matches!(pending.command, Command::JoinPlayer { .. })
                    });
                    if self.clients[client_index].player_id.is_some() || already_pending {
                        self.clients[client_index].queue_line("ERR already connected");
                        return;
                    }
                }
                self.commands.push_back(PendingCommand {
                    origin: client_id,
                    command,
                });
            }
            Ok(ParsedLine::State) => self.send_state(client_id),
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
        client.queue_line("HELP state");
        client.queue_line("HELP quit");
    }

    fn send_state(&mut self, client_id: u64) {
        let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        else {
            return;
        };
        let summary = self.world.summary();
        client.queue_line(format!(
            "WORLD tick={} players={} npcs={} enemies={} vendors={}",
            summary.tick,
            summary.player_count,
            summary.npc_count,
            summary.enemy_count,
            summary.vendor_count
        ));
        for player in self.world.players() {
            client.queue_line(format!(
                "PLAYER id={} name={} role={} pos={:.2},{:.2} hp={}/{} target={}",
                player.id,
                player.name,
                player.role.as_str(),
                player.position.x,
                player.position.y,
                player.health,
                player.max_health,
                player
                    .target
                    .map_or_else(|| "none".to_owned(), |target| target.to_string())
            ));
        }
        for npc in self.world.npcs() {
            client.queue_line(format!(
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
    }

    fn advance_if_due(&mut self) {
        if Instant::now() < self.next_tick {
            return;
        }

        let pending: Vec<_> = self.commands.drain(..).collect();
        let mut join_origins: VecDeque<u64> = pending
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
                && let Some(client) = self.clients.iter_mut().find(|client| client.id == origin)
            {
                client.player_id = Some(player.id);
                client.queue_line(format!(
                    "CONNECTED player_id={} role={}",
                    player.id,
                    player.role.as_str()
                ));
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
                    origin: client.id,
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
    }

    fn remove_closed(&mut self) {
        self.clients
            .retain(|client| !client.closed || !client.output.is_empty());
    }

    fn broadcast(&mut self, line: String) {
        for client in &mut self.clients {
            if !client.closed {
                client.queue_line(&line);
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
        Event::CommandRejected { reason } => format!("EVENT rejected reason={reason}"),
    }
}

enum ParsedLine {
    Command(Command),
    State,
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
        "state" => Ok(ParsedLine::State),
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

fn main() -> io::Result<()> {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let listener = TcpListener::bind(&address)?;
    listener.set_nonblocking(true)?;
    let mut server = Server::new(DEFAULT_TICK_HZ);
    println!("server_listening address={address} tick_hz={DEFAULT_TICK_HZ}");

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

        server.read_clients();
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
    fn event_format_is_human_readable() {
        let event = Event::EnemyDefeated {
            enemy_id: EntityId(9),
        };
        assert_eq!(format_event(&event), "EVENT enemy_defeated id=9");
    }
}
