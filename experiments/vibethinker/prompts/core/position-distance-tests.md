Role: Bounded Rust test author.

Repository context:
- The response is inserted inside the existing #[cfg(test)] mod tests in
  crates/mmorpg-core/src/lib.rs, so Position and its private methods are in
  scope.
- Position::new(x: f32, y: f32) constructs a position.
- Position::distance_squared(self, other: Position) -> f32 returns squared
  Euclidean distance.

Scope:
- Return only three #[test] functions.
- Do not include imports, Markdown fences, or explanation.
- Do not invent APIs.
- Do not modify production code.

Task:
Test same-point distance, the 3-4-5 triangle, and symmetry.

Acceptance cases:
- The distance from a point to itself is 0.0.
- The distance from (0.0, 0.0) to (3.0, 4.0) is 25.0.
- Distance is symmetric.
- Use assert!((actual - expected).abs() < f32::EPSILON).

Required response:
- Return only the test functions.
