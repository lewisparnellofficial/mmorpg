Role: Bounded Rust test author.

Repository context:
- The response is inserted inside the existing #[cfg(test)] mod tests in
  crates/mmorpg-server/src/main.rs, where super::* is already imported.
- parse_line(line: &str, bound_player: Option<EntityId>) ->
  Result<ParsedLine, String>.
- EntityId is constructed as EntityId(u64).

Scope:
- Return only one #[test] function.
- Do not include imports, Markdown fences, or explanation.
- Do not modify parser behavior, public APIs, or dependencies.

Task:
Write a test named parser_rejects_unknown_and_malformed_commands.

Acceptance cases:
- parse_line("bogus", None) returns Err("unknown command 'bogus'; try help").
- parse_line("move nope 1", Some(EntityId(7))) returns
  Err("entity-id must be an integer").
- parse_line("buy 1 2 0", Some(EntityId(7))) returns
  Err("quantity must be a positive integer").
- Use assert_eq! on the complete Result values.

Required response:
- Return only compilable test code.
- Do not claim that tests passed; the caller will run them.
