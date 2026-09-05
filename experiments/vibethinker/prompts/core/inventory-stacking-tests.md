Role: Bounded Rust test author.

Repository context:
- Tests are inserted inside the existing #[cfg(test)] mod tests in
  crates/mmorpg-core/src/lib.rs, so private Inventory methods are visible.
- Inventory::new(capacity) creates a slot-limited inventory.
- Inventory::used_slots() returns the number of stacks.
- Inventory::quantity(item_id) returns the total quantity for an item.
- Inventory::can_add(definition, quantity) checks whether quantity fits,
  filling partial matching stacks before empty slots.
- Inventory::add(definition, quantity) fills matching partial stacks and then
  creates stacks no larger than definition.max_stack.
- ItemDefinition fields are id: ItemId, name: &'static str, and max_stack:
  u32.

Scope:
- Return only compilable #[test] functions.
- Do not include use statements or invent fields or methods.
- Do not modify production code.

Task:
Write focused tests covering partial-stack filling, creation of a second
stack when quantity exceeds max_stack, used slot count, and rejection when the
inventory has insufficient capacity.

Acceptance cases:
- Construct ItemDefinition { id: ItemId(7), name: "Test item", max_stack: 10 }.
- Use Inventory::new(2).
- Check the expected total quantity and stack count after adding 15 items.
- Check that an additional quantity that cannot fit is rejected.

Required response:
- Return only test code.
- Do not claim that tests passed; the caller will run them.
