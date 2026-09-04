#!/usr/bin/env python3
"""Deterministic, non-networked replication workload model.

This is deliberately a workload model rather than a server implementation. It
counts candidate checks, scheduled updates, dirty deltas, admitted updates,
and payload bytes for a dense 200-player activity. The model uses a stable
integer mixer instead of Python's randomized hash so runs are reproducible.
"""

from __future__ import annotations

import argparse
import json
import math
import time
from dataclasses import asdict, dataclass
from typing import Iterable


@dataclass(frozen=True)
class EntitySet:
    name: str
    count: int
    priority: int
    full_bytes: int
    delta_bytes: int
    dirty_rate: float
    frequency_hz: float


DEFAULT_ENTITY_SETS = (
    EntitySet("other_players", 199, 100, 96, 28, 0.65, 20.0),
    EntitySet("boss", 1, 110, 96, 32, 0.95, 20.0),
    EntitySet("encounter_adds", 40, 90, 80, 24, 0.75, 20.0),
    EntitySet("area_effects", 80, 75, 32, 12, 0.80, 20.0),
    EntitySet("dynamic_objects", 25, 50, 48, 14, 0.50, 10.0),
    EntitySet("ambient_npcs", 20, 20, 64, 18, 0.15, 5.0),
)


def stable_mix(*values: int) -> int:
    """Small deterministic mixer; it is not intended as a cryptographic PRNG."""

    value = 0x9E3779B9
    for item in values:
        value ^= (item + 0x9E3779B9 + ((value << 6) & 0xFFFFFFFF) + (value >> 2))
        value &= 0xFFFFFFFF
    value ^= value >> 16
    value = (value * 0x85EBCA6B) & 0xFFFFFFFF
    value ^= value >> 13
    return value & 0xFFFFFFFF


def is_due(tick: int, tick_hz: int, frequency_hz: float) -> bool:
    """Return whether a frequency has a send opportunity during this tick."""

    if frequency_hz <= 0.0:
        return False
    previous = int((tick * frequency_hz) // tick_hz)
    current = int(((tick + 1) * frequency_hz) // tick_hz)
    return current > previous


def make_entity_sets(args: argparse.Namespace) -> tuple[EntitySet, ...]:
    overrides = {
        "other_players": args.players_hz,
        "boss": args.combat_hz,
        "encounter_adds": args.combat_hz,
        "area_effects": args.effects_hz,
        "dynamic_objects": args.dynamic_hz,
        "ambient_npcs": args.ambient_hz,
    }
    sets = []
    for entity_set in DEFAULT_ENTITY_SETS:
        sets.append(
            EntitySet(
                entity_set.name,
                entity_set.count,
                entity_set.priority,
                entity_set.full_bytes,
                entity_set.delta_bytes,
                entity_set.dirty_rate,
                overrides[entity_set.name],
            )
        )
    return tuple(sets)


def changed_for_tick(
    *,
    entity_sets: Iterable[EntitySet],
    players: int,
    tick: int,
    tick_hz: int,
    mode: str,
    seed: int,
) -> tuple[int, int, dict[str, int], list[dict[str, int]]]:
    candidate_checks = 0
    dirty_checks = 0
    opportunities: dict[str, int] = {entity_set.name: 0 for entity_set in entity_sets}
    changed_by_client: list[dict[str, int]] = [
        {entity_set.name: 0 for entity_set in entity_sets} for _ in range(players)
    ]

    for client_id in range(players):
        for entity_set in entity_sets:
            candidate_checks += entity_set.count
            if not is_due(tick, tick_hz, entity_set.frequency_hz):
                continue
            opportunities[entity_set.name] += entity_set.count
            if mode == "full":
                changed_by_client[client_id][entity_set.name] = entity_set.count
                continue
            dirty_checks += entity_set.count
            # Aggregate dirty sampling keeps this model cheap enough to sweep.
            # The fractional remainder is distributed deterministically across
            # clients and ticks, rather than allocating one object per entity.
            expected = entity_set.count * entity_set.dirty_rate
            base = int(expected)
            remainder = expected - base
            roll = stable_mix(seed, tick, client_id, hash_name(entity_set.name))
            changed_by_client[client_id][entity_set.name] = base + (
                1 if (roll / 0x100000000) < remainder else 0
            )
    return candidate_checks, dirty_checks, opportunities, changed_by_client


def hash_name(value: str) -> int:
    result = 2166136261
    for char in value.encode("ascii"):
        result ^= char
        result = (result * 16777619) & 0xFFFFFFFF
    return result


def run_profile(
    *,
    name: str,
    entity_sets: tuple[EntitySet, ...],
    players: int,
    seconds: int,
    tick_hz: int,
    mode: str,
    budget_kib_per_client_second: float | None,
    seed: int,
) -> dict:
    start = time.perf_counter()
    ticks = seconds * tick_hz
    dirty_checks = 0
    candidate_checks = 0
    admitted = 0
    dropped = 0
    payload_bytes = 0
    changed_updates = 0
    changed_bytes = 0
    priority_sort_work_units = 0.0
    by_set: dict[str, dict[str, int]] = {
        entity_set.name: {
            "opportunities": 0,
            "changed": 0,
            "admitted": 0,
            "dropped": 0,
            "bytes": 0,
        }
        for entity_set in entity_sets
    }
    budget_per_tick = None
    if budget_kib_per_client_second is not None:
        budget_per_tick = int((budget_kib_per_client_second * 1024.0) / tick_hz)

    for tick in range(ticks):
        tick_candidates, tick_dirty_checks, tick_opportunities, changed_by_client = changed_for_tick(
            entity_sets=entity_sets,
            players=players,
            tick=tick,
            tick_hz=tick_hz,
            mode=mode,
            seed=seed,
        )
        candidate_checks += tick_candidates
        dirty_checks += tick_dirty_checks
        for entity_set_name, count in tick_opportunities.items():
            by_set[entity_set_name]["opportunities"] += count
        changed_by_set = {
            entity_set.name: sum(client[entity_set.name] for client in changed_by_client)
            for entity_set in entity_sets
        }
        changed_updates += sum(changed_by_set.values())
        changed_bytes += sum(
            changed_by_set[entity_set.name]
            * (entity_set.full_bytes if mode == "full" else entity_set.delta_bytes)
            for entity_set in entity_sets
        )
        for entity_set_name, count in changed_by_set.items():
            by_set[entity_set_name]["changed"] += count

        if budget_per_tick is None:
            admitted_by_set = changed_by_set
        else:
            admitted_by_set = {entity_set.name: 0 for entity_set in entity_sets}
            # The real implementation may sort individual updates. This model
            # uses priority buckets, which preserves the bandwidth decision
            # without allocating one Python object per entity update.
            priority_sort_work_units += players * len(entity_sets) * math.log2(len(entity_sets))
            for client_changes in changed_by_client:
                remaining = budget_per_tick
                for entity_set in sorted(entity_sets, key=lambda item: -item.priority):
                    changed = client_changes[entity_set.name]
                    item_bytes = entity_set.full_bytes if mode == "full" else entity_set.delta_bytes
                    accepted = min(changed, remaining // item_bytes)
                    admitted_by_set[entity_set.name] += accepted
                    remaining -= accepted * item_bytes

        admitted_this_tick = sum(admitted_by_set.values())
        admitted += admitted_this_tick
        payload_this_tick = sum(
            admitted_by_set[entity_set.name]
            * (entity_set.full_bytes if mode == "full" else entity_set.delta_bytes)
            for entity_set in entity_sets
        )
        payload_bytes += payload_this_tick
        for entity_set in entity_sets:
            entity_set_name = entity_set.name
            by_set[entity_set_name]["admitted"] += admitted_by_set[entity_set_name]
            by_set[entity_set_name]["bytes"] += admitted_by_set[entity_set_name] * (
                entity_set.full_bytes if mode == "full" else entity_set.delta_bytes
            )
            by_set[entity_set_name]["dropped"] += (
                changed_by_set[entity_set_name] - admitted_by_set[entity_set_name]
            )
        dropped += sum(changed_by_set.values()) - admitted_this_tick

    elapsed_ms = (time.perf_counter() - start) * 1000.0
    serialized_work = sum(
        values["admitted"] * 4 + values["bytes"] / 16.0
        for values in by_set.values()
    )
    cpu_work_units = candidate_checks + (2 * dirty_checks) + serialized_work + priority_sort_work_units
    return {
        "name": name,
        "mode": mode,
        "budget_kib_per_client_second": budget_kib_per_client_second,
        "seconds": seconds,
        "tick_hz": tick_hz,
        "ticks": ticks,
        "candidate_checks": candidate_checks,
        "dirty_checks": dirty_checks,
        "changed_updates": changed_updates,
        "changed_mib": changed_bytes / (1024 * 1024),
        "admitted_updates": admitted,
        "dropped_updates": dropped,
        "drop_rate": (dropped / changed_updates) if changed_updates else 0.0,
        "payload_mib": payload_bytes / (1024 * 1024),
        "aggregate_mib_per_second": payload_bytes / seconds / (1024 * 1024),
        "per_client_kib_per_second": payload_bytes / seconds / players / 1024,
        "estimated_cpu_work_units": cpu_work_units,
        "priority_sort_work_units": priority_sort_work_units,
        "model_runtime_ms": elapsed_ms,
        "by_entity_set": by_set,
    }


def print_report(args: argparse.Namespace, entity_sets: tuple[EntitySet, ...], results: list[dict]) -> None:
    print("Replication workload model (not a network implementation)")
    print(f"configuration: players={args.players}, seconds={args.seconds}, tick_hz={args.tick_hz}, seed={args.seed}")
    print(
        "entity sets: "
        + ", ".join(f"{item.name}={item.count}@{item.frequency_hz:g}Hz" for item in entity_sets)
    )
    print("payload sizes: full bytes and changed-delta bytes; dirty rates are deterministic assumptions")
    print()
    print("profile                         changed/s    admitted/s  dropped/s  drop%  client KiB/s  aggregate MiB/s  model ms")
    print("------------------------------  -----------  ----------  ---------  -----  -------------  ----------------  --------")
    for result in results:
        scheduled_per_second = result["changed_updates"] / result["seconds"]
        admitted_per_second = result["admitted_updates"] / result["seconds"]
        dropped_per_second = result["dropped_updates"] / result["seconds"]
        print(
            f"{result['name']:<30} {scheduled_per_second:>11,.0f}  "
            f"{admitted_per_second:>10,.0f}  {dropped_per_second:>9,.0f}  "
            f"{result['drop_rate'] * 100:>5.1f}  {result['per_client_kib_per_second']:>13.1f}  "
            f"{result['aggregate_mib_per_second']:>16.2f}  {result['model_runtime_ms']:>8.1f}"
        )
    print()
    print("profile detail (run totals)")
    for result in results:
        print(f"\n[{result['name']}]")
        print(
            f"candidate_checks={result['candidate_checks']:,} "
            f"dirty_checks={result['dirty_checks']:,} "
            f"changed_bytes={result['changed_mib']:.2f} MiB "
            f"admitted_bytes={result['payload_mib']:.2f} MiB "
            f"estimated_cpu_work_units={result['estimated_cpu_work_units']:,.0f}"
        )
        print("entity_set                 due/s  changed/s  admitted/s  dropped/s  admitted MiB/s")
        for entity_set in entity_sets:
            values = result["by_entity_set"][entity_set.name]
            print(
                f"{entity_set.name:<25} "
                f"{values['opportunities'] / result['seconds']:>6,.0f}  "
                f"{values['changed'] / result['seconds']:>9,.0f}  "
                f"{values['admitted'] / result['seconds']:>10,.0f}  "
                f"{values['dropped'] / result['seconds']:>9,.0f}  "
                f"{values['bytes'] / result['seconds'] / (1024 * 1024):>15.3f}"
            )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--players", type=int, default=200)
    parser.add_argument("--seconds", type=int, default=10)
    parser.add_argument("--tick-hz", type=int, default=20)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--budget-kib", type=float, default=64.0, help="per-client aggregate budget for the prioritized profile")
    parser.add_argument("--players-hz", type=float, default=20.0)
    parser.add_argument("--combat-hz", type=float, default=20.0)
    parser.add_argument("--effects-hz", type=float, default=20.0)
    parser.add_argument("--dynamic-hz", type=float, default=10.0)
    parser.add_argument("--ambient-hz", type=float, default=5.0)
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON instead of the report")
    args = parser.parse_args()
    if args.players <= 0 or args.seconds <= 0 or args.tick_hz <= 0:
        parser.error("players, seconds, and tick-hz must be positive")

    entity_sets = make_entity_sets(args)
    results = [
        run_profile(
            name="full_all_visible_20hz",
            entity_sets=tuple(
                EntitySet(item.name, item.count, item.priority, item.full_bytes, item.delta_bytes, item.dirty_rate, 20.0)
                for item in entity_sets
            ),
            players=args.players,
            seconds=args.seconds,
            tick_hz=args.tick_hz,
            mode="full",
            budget_kib_per_client_second=None,
            seed=args.seed,
        ),
        run_profile(
            name="delta_all_visible_20hz",
            entity_sets=tuple(
                EntitySet(item.name, item.count, item.priority, item.full_bytes, item.delta_bytes, item.dirty_rate, 20.0)
                for item in entity_sets
            ),
            players=args.players,
            seconds=args.seconds,
            tick_hz=args.tick_hz,
            mode="delta",
            budget_kib_per_client_second=None,
            seed=args.seed,
        ),
        run_profile(
            name=f"prioritized_delta_{args.budget_kib:g}KiB_client_s",
            entity_sets=entity_sets,
            players=args.players,
            seconds=args.seconds,
            tick_hz=args.tick_hz,
            mode="delta",
            budget_kib_per_client_second=args.budget_kib,
            seed=args.seed,
        ),
    ]
    if args.json:
        print(json.dumps({"configuration": vars(args), "entity_sets": [asdict(item) for item in entity_sets], "profiles": results}, indent=2))
    else:
        print_report(args, entity_sets, results)


if __name__ == "__main__":
    main()
