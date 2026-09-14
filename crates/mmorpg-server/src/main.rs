//! Binary entrypoint for the authoritative mmorpg-server headless development process.

use mmorpg_server::{DEFAULT_ADDRESS, DEFAULT_TICK_HZ, Server};
use std::env;
use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn parse_args() -> Result<(String, Option<String>, Option<PathBuf>, Option<u64>), String> {
    let mut args = env::args().skip(1);
    let addr = args.next().unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let (mut wire, mut store, mut ticks) = (None, None, None);
    while let Some(arg) = args.next() {
        let flag = arg.as_str();
        let mut val = |r| args.next().ok_or_else(|| format!("{flag} requires {r}"));
        match flag {
            "--wire-address" if wire.is_none() => wire = Some(val("an address")?),
            "--character-store" if store.is_none() => {
                store = Some(PathBuf::from(val("a file path")?))
            }
            "--shutdown-after-ticks" if ticks.is_none() => {
                let text = val("a tick count")?;
                ticks = Some(
                    text.parse()
                        .map_err(|_| "--shutdown-after-ticks requires an integer".to_owned())?,
                );
            }
            "--wire-address" | "--character-store" | "--shutdown-after-ticks" => {
                return Err(format!("{flag} may only be specified once"));
            }
            _ => return Err(format!("unknown argument '{flag}'")),
        }
    }
    Ok((addr, wire, store, ticks))
}

fn main() -> io::Result<()> {
    let (addr, wire_addr, store, stop_ticks) =
        parse_args().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let listener = TcpListener::bind(&addr)?;
    listener.set_nonblocking(true)?;
    let extra = wire_addr.as_deref().map(TcpListener::bind).transpose()?;
    if let Some(l) = &extra {
        l.set_nonblocking(true)?;
    }
    let dev_auth = listener.local_addr().is_ok_and(|a| a.ip().is_loopback());
    let mut server = Server::new(DEFAULT_TICK_HZ, dev_auth, store.clone());
    println!(
        "typed_server_listening address={addr} tick_hz={DEFAULT_TICK_HZ}\ntyped_dev_auth_enabled={dev_auth}"
    );
    if let Some(a) = &wire_addr {
        println!("typed_server_additional_listener address={a}");
    }
    if let Some(p) = &store {
        println!("character_checkpoint_store={}", p.display());
    }
    if let Some(t) = stop_ticks {
        println!("shutdown_after_ticks={t}");
    }

    loop {
        if stop_ticks.is_none_or(|limit| server.world.tick() < limit) {
            server.accept_from(&listener, "accepted_typed_peer")?;
            if let Some(l) = &extra {
                server.accept_from(l, "accepted_typed_additional_peer")?;
            }
        }
        if stop_ticks.is_some_and(|limit| server.world.tick() >= limit) {
            println!("graceful_shutdown_begin tick={}", server.world.tick());
            server.shutdown();
            server.flush_clients();
            let st = server.timing_stats;
            println!(
                "simulation_timing ticks={} deadline_misses={} max_duration_ms={:.3}\ngraceful_shutdown_complete tick={}",
                st.ticks,
                st.deadline_misses,
                st.max_duration.as_secs_f64() * 1000.0,
                server.world.tick()
            );
            return Ok(());
        }
        server.step_frame();
        thread::sleep(Duration::from_millis(1));
    }
}
