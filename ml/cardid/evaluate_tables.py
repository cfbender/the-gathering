"""Score one strategy's multi-card proposals against a table-scene JSONL manifest.

`evaluate_strategy` is the shared harness the three strategies in `table_strategies.py` (and
`bench_tables.py`'s timing pass) are all scored through, so every number in the comparison
report comes from identical matching, thresholds, and slices. It measures the plan's
detection-stage gates: precision/recall/F1 and AP at IoU 0.50/0.75, orientation accuracy
(the printed-order corner distance test), a false/duplicate-overlay rate, and slices by
occlusion, rotation, and the identifiable-card flag. It does not score card identity: that
needs a trained embedding model this offline session does not have (see `table_strategies.py`
and `ml/README.md`).
"""

from __future__ import annotations

import argparse
import json
import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path

import cv2
import numpy as np

from .scene_geometry import quad_iou, quad_short

OCCLUSION_BUCKETS = (("none", 0.0, 0.05), ("light", 0.05, 0.3), ("heavy", 0.3, 1.01))


@dataclass(eq=False)
class Proposal:
    """One strategy's candidate card: a printed-order-ish 4x2 quad and a confidence score
    used only for ranking (AP, NMS), never compared across strategies as a probability.
    `eq=False` keeps identity-based equality/hashing; the default dataclass `__eq__` would
    compare the `quad` ndarray with `==` and raise on the resulting elementwise array."""

    quad: np.ndarray
    score: float
    source: str


def load_scenes(manifest: Path, split: str | None = None) -> list[dict]:
    """Manifest rows (optionally filtered to one split), each with an added absolute
    `_image_path`. Pixels are loaded lazily by the caller, not here, so a frozen test
    manifest with thousands of scenes does not have to fit in memory at once."""
    rows = []
    for line in manifest.read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        if split is not None and row.get("split") != split:
            continue
        row["_image_path"] = manifest.parent / row["image"]
        rows.append(row)
    return rows


def read_scene_image(row: dict) -> np.ndarray:
    image = cv2.imread(str(row["_image_path"]))
    if image is None:
        raise FileNotFoundError(row["_image_path"])
    return cv2.cvtColor(image, cv2.COLOR_BGR2RGB)


def evaluate_quads(truth: list[np.ndarray], found: list[np.ndarray], threshold: float = 0.5) -> dict:
    """One-to-one greedy IoU matching and printed-corner orientation accuracy for one scene."""
    pairs = sorted(((quad_iou(t, f), i, j) for i, t in enumerate(truth) for j, f in enumerate(found)), reverse=True)
    used_truth, used_found, oriented = set(), set(), 0
    for score, i, j in pairs:
        if score < threshold or i in used_truth or j in used_found:
            continue
        used_truth.add(i)
        used_found.add(j)
        oriented += int(np.linalg.norm(found[j] - truth[i], axis=1).mean() <= quad_short(truth[i]) * 0.15)
    precision = len(used_found) / len(found) if found else 0.0
    recall = len(used_truth) / len(truth) if truth else 0.0
    return {
        "truth": len(truth),
        "found": len(found),
        "matched": len(used_truth),
        "precision": precision,
        "recall": recall,
        "f1": 2 * precision * recall / (precision + recall) if precision + recall else 0.0,
        "orientation_accuracy": oriented / len(used_truth) if used_truth else 0.0,
    }


def per_card_hits(truth: list[np.ndarray], found: list[np.ndarray], threshold: float = 0.5) -> list[bool]:
    """Whether each truth card (by index) was claimed by some found quad, greedy by IoU."""
    pairs = sorted(((quad_iou(t, f), i, j) for i, t in enumerate(truth) for j, f in enumerate(found)), reverse=True)
    used_truth, used_found, hit = set(), set(), [False] * len(truth)
    for score, i, j in pairs:
        if score < threshold or i in used_truth or j in used_found:
            continue
        used_truth.add(i)
        used_found.add(j)
        hit[i] = True
    return hit


def ranked_hits(truth: list[np.ndarray], proposals: list[Proposal], threshold: float) -> list[bool]:
    """One hit/miss flag per proposal, in the caller's order (callers rank by descending
    score before concatenating across scenes for `average_precision`); a proposal counts once
    it claims an unclaimed truth quad at or above `threshold`, so a second box on the same
    card is a miss here even if its IoU is high (the duplicate/false-overlay rate this
    penalises is exactly what the plan asks evaluation to catch)."""
    used = set()
    hits = []
    for p in proposals:
        best_iou, best_i = 0.0, -1
        for i, t in enumerate(truth):
            if i in used:
                continue
            iou = quad_iou(p.quad, t)
            if iou > best_iou:
                best_iou, best_i = iou, i
        if best_iou >= threshold:
            used.add(best_i)
            hits.append(True)
        else:
            hits.append(False)
    return hits


def average_precision(total_truth: int, hits_by_score: list[bool]) -> float:
    """All-points interpolated AP (COCO/VOC-style) from hit/miss flags already ranked by
    descending confidence across the whole split. `total_truth` is every ground-truth
    instance, matched or not, so a strategy cannot inflate AP by only running on easy scenes."""
    if not hits_by_score or not total_truth:
        return 0.0
    hits = np.asarray(hits_by_score, dtype=np.float64)
    tp, fp = np.cumsum(hits), np.cumsum(1 - hits)
    recall = tp / total_truth
    precision = tp / np.maximum(tp + fp, 1e-9)
    for i in range(len(precision) - 2, -1, -1):
        precision[i] = max(precision[i], precision[i + 1])
    recall = np.concatenate([[0.0], recall])
    precision = np.concatenate([[precision[0]], precision])
    return float(np.sum((recall[1:] - recall[:-1]) * precision[1:]))


def _occlusion_bucket(fraction: float) -> str:
    for name, lo, hi in OCCLUSION_BUCKETS:
        if lo <= fraction < hi:
            return name
    return OCCLUSION_BUCKETS[-1][0]


def evaluate_strategy(
    rows: Iterable[dict],
    predict: Callable[[np.ndarray], list[Proposal]],
    thresholds: tuple[float, ...] = (0.5, 0.75),
    only_identifiable: bool = False,
) -> dict:
    """Run `predict` (a `table_strategies` strategy, already bound to its detector) over every
    scene in `rows` and score it against the manifest's ground truth. Returns per-threshold
    precision/recall/F1/AP/orientation, timing percentiles, and occlusion/rotation/
    identifiable/scene-attribute slices, all from one pass over the images so bench and
    report scripts do not re-decode them per metric."""
    per_threshold = {t: {"used_truth": 0, "used_found": 0, "truth": 0, "found": 0, "oriented": 0, "hits": []} for t in thresholds}
    latencies_ms: list[float] = []
    occlusion_hits = {name: [0, 0] for name, _, _ in OCCLUSION_BUCKETS}
    orientation_hits = {"axis_aligned": [0, 0], "rotated": [0, 0]}
    identifiable_hits = [0, 0]
    by_setup: dict[str, list[int]] = {}
    by_camera_profile: dict[str, list[int]] = {}
    by_density: dict[str, list[int]] = {}
    scenes_scored = 0
    for row in rows:
        image = read_scene_image(row)
        cards = [c for c in row["cards"] if not only_identifiable or c["identifiable"]]
        truth = [np.float32(c["quad"]) for c in cards]
        started = time.perf_counter()
        proposals = predict(image)
        latencies_ms.append((time.perf_counter() - started) * 1000)
        scenes_scored += 1
        ranked = sorted(proposals, key=lambda p: -p.score)
        primary = thresholds[0]
        hits = per_card_hits(truth, [p.quad for p in ranked], primary)
        for card, hit in zip(cards, hits, strict=True):
            bucket = occlusion_hits[_occlusion_bucket(card["occluded_fraction"])]
            bucket[0] += hit
            bucket[1] += 1
            axis = orientation_hits["axis_aligned" if min(card["orientation"] % 90, 90 - card["orientation"] % 90) <= 5 else "rotated"]
            axis[0] += hit
            axis[1] += 1
            if card["identifiable"]:
                identifiable_hits[0] += hit
                identifiable_hits[1] += 1
        group_hit = [sum(hits), len(hits)]
        for grouping, key in ((by_setup, row.get("setup")), (by_camera_profile, row.get("camera_profile")), (by_density, row.get("density"))):
            slot = grouping.setdefault(key, [0, 0])
            slot[0] += group_hit[0]
            slot[1] += group_hit[1]
        for threshold in thresholds:
            found = [p.quad for p in ranked]
            metrics = evaluate_quads(truth, found, threshold)
            acc = per_threshold[threshold]
            acc["used_truth"] += metrics["matched"]
            acc["used_found"] += round(metrics["precision"] * metrics["found"]) if metrics["found"] else 0
            acc["truth"] += metrics["truth"]
            acc["found"] += metrics["found"]
            acc["oriented"] += round(metrics["orientation_accuracy"] * metrics["matched"])
            acc["hits"].extend(ranked_hits(truth, ranked, threshold))

    def ratio(pair: list[int]) -> float | None:
        return pair[0] / pair[1] if pair[1] else None

    thresholds_out = {}
    for threshold, acc in per_threshold.items():
        precision = acc["used_found"] / acc["found"] if acc["found"] else 0.0
        recall = acc["used_truth"] / acc["truth"] if acc["truth"] else 0.0
        false_overlay = 1 - precision if acc["found"] else 0.0
        thresholds_out[threshold] = {
            "truth": acc["truth"],
            "found": acc["found"],
            "matched": acc["used_truth"],
            "precision": precision,
            "recall": recall,
            "f1": 2 * precision * recall / (precision + recall) if precision + recall else 0.0,
            "orientation_accuracy": acc["oriented"] / acc["used_truth"] if acc["used_truth"] else 0.0,
            "average_precision": average_precision(acc["truth"], acc["hits"]),
            "false_overlay_rate": false_overlay,
        }
    latencies = np.array(latencies_ms) if latencies_ms else np.zeros(1)
    return {
        "scenes": scenes_scored,
        "thresholds": thresholds_out,
        "latency_ms": {"p50": float(np.percentile(latencies, 50)), "p95": float(np.percentile(latencies, 95)), "mean": float(latencies.mean())},
        "slices": {
            "occlusion": {name: {"recall": ratio(v), "n": v[1]} for name, v in occlusion_hits.items()},
            "orientation": {name: {"recall": ratio(v), "n": v[1]} for name, v in orientation_hits.items()},
            "identifiable": {"recall": ratio(identifiable_hits), "n": identifiable_hits[1]},
            "setup": {str(k): {"recall": ratio(v), "n": v[1]} for k, v in by_setup.items()},
            "camera_profile": {str(k): {"recall": ratio(v), "n": v[1]} for k, v in by_camera_profile.items()},
            "density": {str(k): {"recall": ratio(v), "n": v[1]} for k, v in by_density.items()},
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--detector", type=Path, required=True)
    parser.add_argument("--split", default="test")
    parser.add_argument("--strategy", choices=("a", "b", "c"), default="b")
    args = parser.parse_args()
    from .detector import Detector
    from .table_strategies import strategy_a_dense_detector, strategy_b_grid_sweep, strategy_c_hybrid

    detector = Detector(args.detector)
    predict = {
        "a": strategy_a_dense_detector,
        "b": lambda image: strategy_b_grid_sweep(image, detector),
        "c": lambda image: strategy_c_hybrid(image, detector),
    }[args.strategy]
    rows = load_scenes(args.manifest, args.split)
    if not rows:
        parser.error(f"no {args.split!r} records in {args.manifest}")
    print(json.dumps(evaluate_strategy(rows, predict), indent=2, default=float))


if __name__ == "__main__":
    main()
