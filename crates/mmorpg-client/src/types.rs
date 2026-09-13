use super::*;

pub(crate) const FIELD_SIZE: Vec2 = Vec2::new(40.0, 30.0);
pub(crate) const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:4000";
pub(crate) const DEV_AUTH_TOKEN: &str = "dev-local";
pub(crate) const MOVEMENT_STEP: f32 = 0.35;
pub(crate) const MOVEMENT_REPEAT_SECONDS: f32 = 0.05;
pub(crate) const MAX_LOG_LINES: usize = 6;
pub(crate) const COMMAND_QUEUE_CAPACITY: usize = 64;
pub(crate) const MAX_DEFERRED_COMMANDS: usize = 32;
pub(crate) const MAX_OUTGOING_LINES: usize = 64;
pub(crate) const MAX_WIRE_INPUT_BYTES: usize = mmorpg_wire::MAX_FRAME_SIZE * 2;
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const RECONNECT_DELAY: Duration = Duration::from_millis(500);
pub(crate) const FRAME_TIME_SAMPLE_CAPACITY: usize = 6000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenderBackendChoice {
    Automatic,
    Vulkan,
    Gl,
}

impl RenderBackendChoice {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Automatic),
            "vulkan" | "vk" => Some(Self::Vulkan),
            "gl" | "opengl" | "gles" => Some(Self::Gl),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Automatic => "auto",
            Self::Vulkan => "vulkan",
            Self::Gl => "gl",
        }
    }

    pub(crate) fn backends(self) -> Option<Backends> {
        match self {
            Self::Automatic => None,
            Self::Vulkan => Some(Backends::VULKAN),
            Self::Gl => Some(Backends::GL),
        }
    }
}

pub(crate) fn render_plugin(choice: RenderBackendChoice) -> RenderPlugin {
    let Some(backends) = choice.backends() else {
        return RenderPlugin::default();
    };
    RenderPlugin {
        render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
            backends: Some(backends),
            ..default()
        })),
        ..default()
    }
}

#[derive(Component)]
pub(crate) struct StarterNpc {
    pub(crate) id: EntityId,
}

#[derive(Component)]
pub(crate) struct PlayerMarker;

#[derive(Component)]
pub(crate) struct StatusText;

#[derive(Resource, Clone)]
pub(crate) struct NpcPresentationAssets {
    pub(crate) mesh: Handle<Mesh>,
    pub(crate) vendor_material: Handle<StandardMaterial>,
    pub(crate) enemy_material: Handle<StandardMaterial>,
}

#[derive(Resource, Debug)]
pub(crate) struct ClientState {
    pub(crate) server_address: String,
    pub(crate) connection_status: String,
    pub(crate) player_id: Option<EntityId>,
    pub(crate) connected_role: Option<RoleCode>,
    pub(crate) available_characters: Vec<CharacterSummary>,
    pub(crate) selected_character_id: Option<u64>,
    pub(crate) target_cursor: usize,
    pub(crate) logs: VecDeque<String>,
    pub(crate) snapshot: SnapshotAssembler,
    pub(crate) presentation: ClientWorld,
}

impl ClientState {
    pub(crate) fn new(server_address: String) -> Self {
        Self {
            server_address,
            connection_status: "connecting".to_owned(),
            player_id: None,
            connected_role: None,
            available_characters: Vec::new(),
            selected_character_id: None,
            target_cursor: 0,
            logs: VecDeque::from(["WASD move  Tab target  Space attack".to_owned()]),
            snapshot: SnapshotAssembler::new(),
            presentation: ClientWorld::default(),
        }
    }

    pub(crate) fn log(&mut self, message: impl Into<String>) {
        if self.logs.len() == MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(message.into());
    }
}

#[derive(Debug)]
pub(crate) enum ClientCommand {
    SelectCharacter {
        character_id: u64,
    },
    Move {
        dx: f32,
        dy: f32,
    },
    Target(EntityId),
    Attack,
    Taunt,
    Heal(EntityId),
    ListVendor(EntityId),
    ListQuestOffers(EntityId),
    BuyItem {
        vendor_id: EntityId,
        item_id: ItemId,
        quantity: u32,
    },
    AcceptQuest {
        npc_id: EntityId,
        quest_id: QuestId,
    },
    TurnInQuest {
        npc_id: EntityId,
        quest_id: QuestId,
    },
    Loot(EntityId),
    InvitePartyMember(EntityId),
    AcceptPartyInvite(u64),
}

#[derive(Debug)]
pub(crate) enum NetworkEvent {
    Status(String),
    ServerMessage(ServerMessage),
}

#[derive(Resource)]
pub(crate) struct NetworkBridge {
    pub(crate) command_tx: SyncSender<ClientCommand>,
    pub(crate) event_rx: Arc<Mutex<Receiver<NetworkEvent>>>,
}

#[derive(Resource)]
pub(crate) struct MovementRepeat(pub(crate) Timer);

#[derive(Clone, Resource)]
pub(crate) struct ScriptedUiPresentation {
    pub(crate) default_label: String,
    pub(crate) addon_label: String,
    pub(crate) default_node_id: u64,
    pub(crate) addon_node_id: u64,
}

pub(crate) struct AddonProcessHandle {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
}

pub(crate) struct ChildGuard(pub(crate) Option<Child>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let Some(mut child) = self.0.take() else {
            return;
        };
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[derive(Resource, Clone)]
pub(crate) struct AddonProcessSupervisor(pub(crate) Arc<Mutex<Option<AddonProcessHandle>>>);

impl Drop for AddonProcessSupervisor {
    fn drop(&mut self) {
        let Ok(mut handle) = self.0.lock() else {
            return;
        };
        let Some(mut handle) = handle.take() else {
            return;
        };
        let _ = writeln!(handle.stdin, "shutdown");
        let _ = handle.stdin.flush();
        let _ = handle.child.wait();
    }
}

#[derive(Resource)]
pub(crate) struct SecureInputState {
    pub(crate) registry: SecureInputRegistry,
    pub(crate) default_addon: AddonId,
    pub(crate) default_node: NodeId,
    pub(crate) node_generation: Generation,
    pub(crate) attack_action: ActionId,
    pub(crate) next_physical_event_id: u64,
    pub(crate) secure_attack_min: Vec2,
    pub(crate) secure_attack_max: Vec2,
}

impl SecureInputState {
    pub(crate) fn new(scripted_ui: &ScriptedUiPresentation) -> Self {
        let default_addon = AddonId::new(1).expect("default addon ID cannot be zero");
        let default_node = NodeId::new(scripted_ui.default_node_id)
            .expect("scripted default action node cannot be zero");
        let addon = AddonId::new(2).expect("addon ID cannot be zero");
        let addon_node = NodeId::new(scripted_ui.addon_node_id)
            .expect("scripted addon action node cannot be zero");
        let node_generation = Generation::new(1).expect("default node generation cannot be zero");
        let attack_action = ActionId::new(1).expect("attack action ID cannot be zero");
        let mut registry = SecureInputRegistry::new();
        registry
            .register(default_addon, default_node, node_generation, attack_action)
            .expect("default secure attack binding must be valid");
        registry
            .register(addon, addon_node, node_generation, attack_action)
            .expect("addon secure attack binding must be valid");
        Self {
            registry,
            default_addon,
            default_node,
            node_generation,
            attack_action,
            next_physical_event_id: 0,
            secure_attack_min: Vec2::new(12.0, 12.0),
            secure_attack_max: Vec2::new(300.0, 68.0),
        }
    }

    pub(crate) fn next_physical_event_id(&mut self) -> u64 {
        self.next_physical_event_id = self.next_physical_event_id.saturating_add(1).max(1);
        self.next_physical_event_id
    }

    pub(crate) fn refresh_default_binding(&mut self) {
        self.registry.unload_addon(self.default_addon);
        self.registry
            .register(
                self.default_addon,
                self.default_node,
                self.node_generation,
                self.attack_action,
            )
            .expect("default secure attack binding must be valid after focus change");
    }
}
