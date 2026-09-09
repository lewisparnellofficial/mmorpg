#![no_main]

use libfuzzer_sys::fuzz_target;
use mmorpg_ui_contract::{
    API_VERSION, AccountId, Generation, Handle, Manifest, PackageId, Property, StoredValue,
    UiLimits, UiOperation, validate_manifest, validate_operations,
};
use std::collections::{BTreeMap, BTreeSet};

struct Cursor<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, index: 0 }
    }

    fn byte(&mut self) -> u8 {
        let value = self.bytes.get(self.index).copied().unwrap_or(0);
        self.index = self.index.saturating_add(1);
        value
    }

    fn bounded_len(&mut self, maximum: usize) -> usize {
        usize::from(self.byte()) % (maximum.saturating_add(1))
    }

    fn bounded_string(&mut self, maximum: usize) -> String {
        let length = self.bounded_len(maximum);
        (0..length)
            .map(|_| {
                let value = self.byte();
                match value % 4 {
                    0 => '\0',
                    1 => '/',
                    2 => '.',
                    _ => char::from(b'a' + value % 26),
                }
            })
            .collect()
    }
}

fn id(value: u8) -> u64 {
    u64::from(value).saturating_add(1)
}

fn handle(cursor: &mut Cursor<'_>, package: PackageId, generation: Generation) -> Handle {
    Handle {
        node: mmorpg_ui_contract::NodeId::new(id(cursor.byte())).expect("non-zero node"),
        owner: if cursor.byte() & 1 == 0 {
            package
        } else {
            PackageId::new(id(cursor.byte())).expect("non-zero package")
        },
        generation: if cursor.byte() & 1 == 0 {
            generation
        } else {
            Generation::new(id(cursor.byte())).expect("non-zero generation")
        },
    }
}

fn stored_value(cursor: &mut Cursor<'_>, depth: usize) -> StoredValue {
    if depth >= 18 {
        return StoredValue::String(cursor.bounded_string(256));
    }
    match cursor.byte() % 7 {
        0 => StoredValue::Null,
        1 => StoredValue::Bool(cursor.byte() & 1 == 0),
        2 => StoredValue::Integer(i64::from(cursor.byte())),
        3 => StoredValue::Number(if cursor.byte() & 1 == 0 {
            f64::from(cursor.byte())
        } else {
            f64::NAN
        }),
        4 => StoredValue::String(cursor.bounded_string(512)),
        5 => {
            let count = cursor.bounded_len(8);
            StoredValue::List((0..count).map(|_| stored_value(cursor, depth + 1)).collect())
        }
        _ => {
            let count = cursor.bounded_len(8);
            let mut values = BTreeMap::new();
            for _ in 0..count {
                values.insert(cursor.bounded_string(32), stored_value(cursor, depth + 1));
            }
            StoredValue::Record(values)
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let mut cursor = Cursor::new(data);
    let package = PackageId::new(id(cursor.byte())).expect("non-zero package");
    let generation = Generation::new(id(cursor.byte())).expect("non-zero generation");
    let account = AccountId::new(id(cursor.byte())).expect("non-zero account");

    let mut package_ids = BTreeSet::new();
    package_ids.insert(package);
    let mut capabilities = BTreeSet::new();
    if cursor.byte() & 1 == 0 {
        capabilities.insert("ui.panel".to_owned());
    }
    let manifest = Manifest {
        package_id: package,
        name: cursor.bounded_string(256),
        version: cursor.bounded_string(256),
        manifest_schema: u32::from(cursor.byte()),
        api_range: cursor.bounded_string(256),
        runtime_range: cursor.bounded_string(256),
        entry: cursor.bounded_string(256),
        load_order: i32::from(cursor.byte()),
        dependencies: (0..cursor.bounded_len(8))
            .map(|_| PackageId::new(id(cursor.byte())).expect("non-zero dependency"))
            .collect(),
        capabilities: (0..cursor.bounded_len(8))
            .map(|_| cursor.bounded_string(32))
            .collect(),
        saved_data: cursor.byte() & 1 == 0,
        asset_ids: (0..cursor.bounded_len(8))
            .map(|_| cursor.bounded_string(64))
            .collect(),
        integrity_sha256: cursor.bounded_string(128),
    };
    let _ = validate_manifest(&manifest, &capabilities, &package_ids);

    let operation_count = cursor.bounded_len(32);
    let operations = (0..operation_count)
        .map(|_| match cursor.byte() % 7 {
            0 => UiOperation::CreatePanel {
                handle: handle(&mut cursor, package, generation),
                text: cursor.bounded_string(512),
            },
            1 => UiOperation::SetProperty {
                handle: handle(&mut cursor, package, generation),
                property: Property::Text(cursor.bounded_string(512)),
            },
            2 => UiOperation::Destroy {
                handle: handle(&mut cursor, package, generation),
            },
            3 => UiOperation::Subscribe {
                event: cursor.bounded_string(128),
            },
            4 => UiOperation::Unsubscribe {
                event: cursor.bounded_string(128),
            },
            5 => UiOperation::CreateTimer {
                timer: mmorpg_ui_contract::TimerId::new(id(cursor.byte())).expect("non-zero timer"),
                interval_ms: u32::from(cursor.byte()),
            },
            _ => UiOperation::PresentSecureAction {
                handle: handle(&mut cursor, package, generation),
                action: cursor.bounded_string(128),
            },
        })
        .collect::<Vec<_>>();
    let _ = validate_operations(&operations, package, generation, &UiLimits::default());

    let namespace = mmorpg_ui_contract::StorageNamespace {
        account_id: account,
        package_id: package,
        schema_version: u32::from(cursor.byte()),
    };
    let mut storage = mmorpg_ui_contract::Storage::default();
    let key = cursor.bounded_string(128);
    let _ = storage.set(namespace, key, stored_value(&mut cursor, 0));
    let _ = API_VERSION;
});
