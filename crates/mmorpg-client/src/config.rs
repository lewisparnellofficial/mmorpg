use super::*;

pub(crate) struct ClientConfig {
    pub(crate) typed_address: String,
    pub(crate) auth_token: String,
    pub(crate) preferred_character_id: Option<u64>,
    pub(crate) addon_root: Option<String>,
    pub(crate) addon_process_host: Option<String>,
    pub(crate) addon_process_package_root: Option<String>,
    pub(crate) acceptance_smoke: bool,
    pub(crate) frame_time_stats: bool,
    pub(crate) render_backend: RenderBackendChoice,
}

impl ClientConfig {
    pub(crate) fn parse() -> Option<Self> {
        let mut arguments = std::env::args().skip(1);
        let server_address = arguments
            .next()
            .unwrap_or_else(|| DEFAULT_SERVER_ADDRESS.to_owned());
        let mut wire_address = None;
        let mut auth_token = DEV_AUTH_TOKEN.to_owned();
        let mut auth_token_set = false;
        let mut preferred_character_id = None;
        let mut addon_root = None;
        let mut addon_process_host = None;
        let mut addon_process_package_root = None;
        let mut acceptance_smoke = false;
        let mut frame_time_stats = false;
        let mut render_backend = RenderBackendChoice::Automatic;
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--token" => {
                    if auth_token_set {
                        eprintln!("--token may only be specified once");
                        return None;
                    }
                    let Some(value) = arguments.next() else {
                        eprintln!("--token requires a value");
                        return None;
                    };
                    if value.is_empty() {
                        eprintln!("--token requires a non-empty value");
                        return None;
                    }
                    auth_token = value;
                    auth_token_set = true;
                }
                "--wire-address" => {
                    if wire_address.is_some() {
                        eprintln!("--wire-address may only be specified once");
                        return None;
                    }
                    wire_address = arguments.next();
                    if wire_address.is_none() {
                        eprintln!("--wire-address requires an address");
                        return None;
                    }
                }
                "--character-id" => {
                    if preferred_character_id.is_some() {
                        eprintln!("--character-id may only be specified once");
                        return None;
                    }
                    let Some(value) = arguments.next() else {
                        eprintln!("--character-id requires a numeric character ID");
                        return None;
                    };
                    match value.parse::<u64>() {
                        Ok(0) | Err(_) => {
                            eprintln!("--character-id must be a non-zero numeric character ID");
                            return None;
                        }
                        Ok(character_id) => preferred_character_id = Some(character_id),
                    }
                }
                "--acceptance-smoke" => acceptance_smoke = true,
                "--frame-time-stats" => frame_time_stats = true,
                "--addon-root" => {
                    if addon_root.is_some() {
                        eprintln!("--addon-root may only be specified once");
                        return None;
                    }
                    addon_root = arguments.next();
                    if addon_root.is_none() {
                        eprintln!("--addon-root requires a package repository path");
                        return None;
                    }
                }
                "--addon-process-host" => {
                    if addon_process_host.is_some() {
                        eprintln!("--addon-process-host may only be specified once");
                        return None;
                    }
                    addon_process_host = arguments.next();
                    if addon_process_host.is_none() {
                        eprintln!("--addon-process-host requires an executable path");
                        return None;
                    }
                }
                "--addon-process-package-root" => {
                    if addon_process_package_root.is_some() {
                        eprintln!("--addon-process-package-root may only be specified once");
                        return None;
                    }
                    addon_process_package_root = arguments.next();
                    if addon_process_package_root.is_none() {
                        eprintln!("--addon-process-package-root requires a package directory");
                        return None;
                    }
                }
                "--render-backend" => {
                    let Some(value) = arguments.next() else {
                        eprintln!("--render-backend requires auto, vulkan, or gl");
                        return None;
                    };
                    let Some(choice) = RenderBackendChoice::parse(&value) else {
                        eprintln!("--render-backend must be auto, vulkan, or gl (got {value})");
                        return None;
                    };
                    render_backend = choice;
                }
                _ => {
                    eprintln!("unknown argument '{argument}'");
                    return None;
                }
            }
        }
        if addon_root.is_some() && addon_process_host.is_some() {
            eprintln!("--addon-root and --addon-process-host are mutually exclusive");
            return None;
        }
        if addon_process_package_root.is_some() && addon_process_host.is_none() {
            eprintln!("--addon-process-package-root requires --addon-process-host");
            return None;
        }
        let typed_address = wire_address.unwrap_or_else(|| server_address.clone());
        println!("render_backend_request={}", render_backend.label());
        Some(Self {
            typed_address,
            auth_token,
            preferred_character_id,
            addon_root,
            addon_process_host,
            addon_process_package_root,
            acceptance_smoke,
            frame_time_stats,
            render_backend,
        })
    }
}
