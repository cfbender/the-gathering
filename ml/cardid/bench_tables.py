"""CPU benchmarks for table scenes: raw rendering throughput, and (with `--strategy`) one
detection strategy's per-scene latency against a manifest -- the p50/p95 numbers the plan asks
the strategy comparison to report, without also scoring accuracy (`evaluate_tables.py` does
that, and already reports the same latencies alongside precision/recall/AP)."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import numpy as np

from .image_bank import ArtBank, CardBank
from .table_scenes import SETUPS, render_table_scene


def bench_render(scenes: int, cards_per_scene: int, seed: int) -> dict:
    cards, arts = CardBank(), ArtBank()
    started = time.perf_counter()
    for i in range(scenes):
        render_table_scene(seed + i, cards, arts, SETUPS[i % len(SETUPS)], count=min(cards_per_scene, len(cards)))
    elapsed = time.perf_counter() - started
    return {"scenes": scenes, "cards_per_scene": cards_per_scene, "ms_per_scene": elapsed * 1000 / scenes}


def bench_strategy(manifest: Path, split: str, strategy: str, detector_path: Path | None, limit: int | None, table_model_path: Path | None = None) -> dict:
    from .evaluate_tables import load_scenes, read_scene_image
    from .table_strategies import strategy_a_dense_detector, strategy_b_grid_sweep, strategy_c_hybrid

    rows = load_scenes(manifest, split)
    if limit:
        rows = rows[:limit]
    if not rows:
        raise SystemExit(f"no {split!r} records in {manifest}")
    if strategy == "a":
        if table_model_path is not None:
            import torch

            from .table_detector import TableCenterNet

            model = TableCenterNet(pretrained=False).eval()
            model.load_state_dict(torch.load(table_model_path, map_location="cpu", weights_only=True))
            predict = lambda image: strategy_a_dense_detector(image, model=model)  # noqa: E731
        else:
            predict = strategy_a_dense_detector
    else:
        from .detector import Detector

        if detector_path is None:
            raise SystemExit(f"strategy {strategy!r} needs --detector")
        detector = Detector(detector_path)
        predict = (lambda image: strategy_b_grid_sweep(image, detector)) if strategy == "b" else (lambda image: strategy_c_hybrid(image, detector))
    times_ms = []
    for row in rows:
        image = read_scene_image(row)
        started = time.perf_counter()
        predict(image)
        times_ms.append((time.perf_counter() - started) * 1000)
    times = np.array(times_ms)
    return {
        "strategy": strategy,
        "scenes": len(times),
        "p50_ms": float(np.percentile(times, 50)),
        "p95_ms": float(np.percentile(times, 95)),
        "mean_ms": float(times.mean()),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenes", type=int, default=20, help="rendering mode: scenes to render")
    parser.add_argument("--cards", type=int, default=8, help="rendering mode: cards per scene")
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--strategy", choices=("a", "b", "c"), help="switch to latency mode for this strategy instead of rendering")
    parser.add_argument("--manifest", type=Path, help="strategy mode: table-scene manifest.jsonl")
    parser.add_argument("--split", default="test", help="strategy mode: which split to read from the manifest")
    parser.add_argument("--detector", type=Path, help="strategy mode: CornerNet checkpoint (strategies b and c)")
    parser.add_argument("--table-model", type=Path, help="strategy mode: TableCenterNet checkpoint for the learned strategy a (default: classical fallback)")
    parser.add_argument("--limit", type=int, help="strategy mode: only benchmark the first N scenes")
    args = parser.parse_args()
    if args.strategy:
        if not args.manifest:
            parser.error("--strategy requires --manifest")
        result = bench_strategy(args.manifest, args.split, args.strategy, args.detector, args.limit, args.table_model)
        print(json.dumps(result, indent=2))
    else:
        result = bench_render(args.scenes, args.cards, args.seed)
        print(f"table scenes: {result['scenes']} scenes, {result['cards_per_scene']} cards/scene, {result['ms_per_scene']:.1f} ms/scene")


if __name__ == "__main__":
    main()
