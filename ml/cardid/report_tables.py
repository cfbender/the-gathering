"""Run all three Super AI mode detection strategies against the frozen `test` and `challenge`
table-scene splits and write a JSON result plus a visual HTML report: per-strategy metrics,
comparison charts, slice breakdowns, and an overlay gallery of true vs. predicted quads,
including the false positives/misses/bad-geometry examples the plan asks a strategy
comparison to show.

    uv run python -m cardid.report_tables ~/the-gathering-cardid/table-scenes --detector data/runs/table-demo/best.pt

This is offline, CPU, detection-only tooling (see `table_strategies.py`'s module docstring for
what stops at detection and why); the report says so on every page, not just here.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import cv2
import numpy as np
import torch

from .evaluate_tables import Proposal, evaluate_strategy, load_scenes, read_scene_image
from .table_strategies import strategy_a_dense_detector, strategy_b_grid_sweep, strategy_c_hybrid

# A palette wide enough for the classical fallback plus however many trained Strategy-A
# variants (--strategy-a-model) are compared, assigned by sorted key for determinism.
PALETTE_BGR = [(255, 140, 0), (0, 90, 255), (200, 0, 200), (0, 170, 120), (140, 90, 0), (0, 0, 220)]
TRUTH_COLOR_BGR = (0, 200, 0)
EVAL_SPLITS = ("test", "challenge")
DEFAULT_LABELS = {"a-classical": "A: classical (no training)", "b": "B: grid + learned localizer", "c": "C: hybrid coarse sweep + refine"}


def strategy_label(key: str) -> str:
    if key in DEFAULT_LABELS:
        return DEFAULT_LABELS[key]
    if key.startswith("a-"):
        return f"A: learned ({key.removeprefix('a-')})"
    return key


def strategy_colors(keys: list[str]) -> dict[str, tuple[int, int, int]]:
    return {key: PALETTE_BGR[i % len(PALETTE_BGR)] for i, key in enumerate(keys)}


def build_predictors(detector, table_variants: dict[str, object] | None = None) -> dict[str, callable]:
    """`detector` is the CornerNet checkpoint for strategies B and C; `table_variants` maps a
    name (e.g. "pretrained", "scratch") to a loaded `TableCenterNet` for extra Strategy-A rows
    alongside the classical, no-training fallback."""
    predictors: dict[str, callable] = {"a-classical": strategy_a_dense_detector}
    for name, model in (table_variants or {}).items():
        predictors[f"a-{name}"] = lambda image, m=model: strategy_a_dense_detector(image, model=m)
    predictors["b"] = lambda image: strategy_b_grid_sweep(image, detector)
    predictors["c"] = lambda image: strategy_c_hybrid(image, detector)
    return predictors


def draw_overlay(image: np.ndarray, truth: list[np.ndarray], proposals: list[Proposal], color: tuple[int, int, int]) -> np.ndarray:
    canvas = cv2.cvtColor(image, cv2.COLOR_RGB2BGR).copy()
    for quad in truth:
        cv2.polylines(canvas, [quad.astype(np.int32)], True, TRUTH_COLOR_BGR, 2, cv2.LINE_AA)
    for p in proposals:
        cv2.polylines(canvas, [p.quad.astype(np.int32)], True, color, 2, cv2.LINE_AA)
    return canvas


def render_gallery(
    rows: list[dict], predictors: dict[str, callable], colors: dict[str, tuple[int, int, int]], out_dir: Path, split: str, samples: int
) -> list[dict]:
    out_dir.mkdir(parents=True, exist_ok=True)
    gallery = []
    for i, row in enumerate(rows[:samples]):
        image = read_scene_image(row)
        truth = [np.float32(c["quad"]) for c in row["cards"]]
        entry = {
            "scene": row["image"],
            "setup": row.get("setup"),
            "camera_profile": row.get("camera_profile"),
            "density": row.get("density"),
            "truth_cards": len(truth),
            "panels": {},
        }
        for key, predict in predictors.items():
            proposals = predict(image)
            overlay = draw_overlay(image, truth, proposals, colors[key])
            name = f"{split}-{i:03d}-{key}.jpg"
            cv2.imwrite(str(out_dir / name), overlay, [cv2.IMWRITE_JPEG_QUALITY, 90])
            entry["panels"][key] = {"file": name, "found": len(proposals)}
        gallery.append(entry)
    return gallery


def svg_bar_chart(title: str, labels: list[str], values: list[float], colors: list[str], value_fmt: str = "{:.2f}", width: int = 380) -> str:
    bar_h, gap, label_w = 26, 10, 190
    max_v = max([*values, 1e-9])
    height = len(values) * (bar_h + gap) + gap
    bars = []
    for i, (label, value, color) in enumerate(zip(labels, values, colors, strict=True)):
        y = gap + i * (bar_h + gap)
        w = max(2.0, (value / max_v) * (width - label_w - 60))
        bars.append(f'<text x="0" y="{y + bar_h * 0.68:.1f}" font-size="12" fill="currentColor">{label}</text>')
        bars.append(f'<rect x="{label_w}" y="{y}" width="{w:.1f}" height="{bar_h}" fill="{color}" rx="3"></rect>')
        bars.append(f'<text x="{label_w + w + 6:.1f}" y="{y + bar_h * 0.68:.1f}" font-size="12" fill="currentColor">{value_fmt.format(value)}</text>')
    return f'<figure class="chart"><figcaption>{title}</figcaption><svg viewBox="0 0 {width} {height}" width="{width}" height="{height}">{"".join(bars)}</svg></figure>'


def metrics_table(split_results: dict, keys: list[str]) -> str:
    rows = []
    for key in keys:
        m = split_results[key]
        t50, t75 = m["thresholds"][0.5], m["thresholds"][0.75]
        rows.append(
            f"<tr><td>{strategy_label(key)}</td><td>{m['scenes']}</td>"
            f"<td>{t50['precision']:.2f}</td><td>{t50['recall']:.2f}</td><td>{t50['f1']:.2f}</td>"
            f"<td>{t50['average_precision']:.2f}</td><td>{t75['average_precision']:.2f}</td>"
            f"<td>{t50['orientation_accuracy']:.2f}</td><td>{t50['false_overlay_rate']:.2f}</td>"
            f"<td>{m['latency_ms']['p50']:.0f} / {m['latency_ms']['p95']:.0f}</td></tr>"
        )
    return (
        "<table><thead><tr><th>Strategy</th><th>Scenes</th><th>Precision@.5</th><th>Recall@.5</th>"
        "<th>F1@.5</th><th>AP@.5</th><th>AP@.75</th><th>Orientation acc.</th><th>False-overlay rate</th>"
        "<th>Latency p50/p95 (ms)</th></tr></thead><tbody>" + "".join(rows) + "</tbody></table>"
    )


def slice_table(split_results: dict, slice_name: str, keys: list[str]) -> str:
    slice_keys = sorted({k for m in split_results.values() for k in m["slices"][slice_name]})
    header = "".join(f"<th>{k}</th>" for k in slice_keys)
    rows = []
    for key in keys:
        cells = []
        for k in slice_keys:
            entry = split_results[key]["slices"][slice_name].get(k)
            cells.append(f"<td>{entry['recall']:.2f} (n={entry['n']})</td>" if entry and entry["recall"] is not None else "<td>-</td>")
        rows.append(f"<tr><td>{strategy_label(key)}</td>{''.join(cells)}</tr>")
    return f"<table><thead><tr><th>Strategy \\ {slice_name}</th>{header}</tr></thead><tbody>{''.join(rows)}</tbody></table>"


def gallery_html(gallery: list[dict]) -> str:
    if not gallery:
        return "<p>No sample scenes.</p>"
    cards = []
    for entry in gallery:
        panels = "".join(
            f'<figure><img src="samples/{p["file"]}" loading="lazy"><figcaption>{strategy_label(key)} - {p["found"]} found</figcaption></figure>'
            for key, p in entry["panels"].items()
        )
        cards.append(
            f'<div class="scene-row"><h4>{entry["scene"]} - {entry["setup"]}/{entry["camera_profile"]}/{entry["density"]}, '
            f'{entry["truth_cards"]} ground-truth cards</h4><div class="panels">{panels}</div></div>'
        )
    return "".join(cards)


def write_html_report(out: Path, header: dict, results: dict[str, dict], galleries: dict[str, list[dict]], colors: dict[str, tuple[int, int, int]]) -> None:
    keys = list(colors)
    css_colors = [f"rgb({r},{g},{b})" for b, g, r in (colors[k] for k in keys)]  # BGR -> RGB for CSS
    labels = [strategy_label(k) for k in keys]
    sections = []
    for split in EVAL_SPLITS:
        split_results = results[split]
        recall = [split_results[k]["thresholds"][0.5]["recall"] for k in keys]
        ap50 = [split_results[k]["thresholds"][0.5]["average_precision"] for k in keys]
        p50_latency = [split_results[k]["latency_ms"]["p50"] for k in keys]
        charts = (
            svg_bar_chart(f"{split}: recall @ IoU 0.50", labels, recall, css_colors)
            + svg_bar_chart(f"{split}: average precision @ IoU 0.50", labels, ap50, css_colors)
            + svg_bar_chart(f"{split}: latency p50 (ms, lower is better)", labels, p50_latency, css_colors, value_fmt="{:.0f}")
        )
        sections.append(
            f"<section><h2>{split} split ({split_results[keys[0]]['scenes']} scenes)</h2>"
            f"<div class='charts'>{charts}</div>"
            f"<h3>Metrics</h3>{metrics_table(split_results, keys)}"
            f"<h3>Recall by occlusion</h3>{slice_table(split_results, 'occlusion', keys)}"
            f"<h3>Recall by rotation</h3>{slice_table(split_results, 'orientation', keys)}"
            f"<h3>Recall by arrangement (setup)</h3>{slice_table(split_results, 'setup', keys)}"
            f"<h3>Recall by camera profile</h3>{slice_table(split_results, 'camera_profile', keys)}"
            f"<h3>Sample overlays ({len(galleries[split])} of {split_results[keys[0]]['scenes']} scenes shown; "
            f"green = ground truth, colour = strategy proposals)</h3>"
            f"<div class='gallery'>{gallery_html(galleries[split])}</div></section>"
        )
    dataset_counts = "".join(
        f"<li><strong>{name}</strong>: {stats['scenes']} scenes, {stats['cards']} cards ({stats['identifiable_cards']} identifiable)</li>"
        for name, stats in header.get("splits", {}).items()
    )
    html = f"""<!doctype html>
<html><head><meta charset="utf-8"><title>Table-scene strategy comparison</title>
<style>
:root {{ color-scheme: light dark; }}
body {{ font: 14px/1.5 system-ui, sans-serif; margin: 2rem auto; max-width: 1100px; padding: 0 1rem; }}
h1, h2, h3 {{ font-weight: 600; }}
table {{ border-collapse: collapse; width: 100%; margin: 0.5rem 0 1.5rem; font-size: 13px; }}
th, td {{ border: 1px solid #8884; padding: 4px 8px; text-align: left; }}
.charts {{ display: flex; flex-wrap: wrap; gap: 1.5rem; margin-bottom: 1rem; }}
.chart svg {{ overflow: visible; }}
.gallery .scene-row {{ margin-bottom: 1.5rem; }}
.panels {{ display: flex; flex-wrap: wrap; gap: 0.75rem; }}
.panels figure {{ margin: 0; }}
.panels img {{ max-width: 260px; display: block; border: 1px solid #8884; border-radius: 4px; }}
.panels figcaption {{ font-size: 12px; text-align: center; }}
.caveats {{ background: #8882; border-radius: 8px; padding: 1rem 1.25rem; margin: 1rem 0 2rem; }}
.caveats li {{ margin-bottom: 0.4rem; }}
code {{ font-size: 0.9em; }}
</style></head>
<body>
<h1>Table-scene multi-card detection: strategy comparison</h1>
<p>Renderer <code>{header.get("renderer_version")}</code>, catalog fingerprint <code>{header.get("catalog_fingerprint")}</code>,
generated {header.get("generated_at")}, {header.get("cards_available")} distinct cards available.</p>
<h2>Dataset</h2>
<ul>{dataset_counts}</ul>
<div class="caveats">
<strong>Scope of this report</strong>
<ul>
<li>This is a scaled-down validation run of the tooling (hundreds of scenes per split), not the plan's full-scale
corpus (150k/20k/20k/5k train/val/test/challenge). The generator (<code>table_scenes.py</code>) accepts
<code>--train/--val/--test/--challenge</code> to scale up on a machine with a full Scryfall art/card download.</li>
<li><strong>Strategy A (classical)</strong> is the no-training fallback: a classical edge/contour multi-quad
detector. Any other "A: learned (...)" row is <code>table_detector.TableCenterNet</code>, a dense
CenterNet-style head over the same MobileNetV3-Small backbone the click-conditioned localizer uses, trained
on the table-scene manifests (see <code>train_table_detector.py</code> for the pretrained-backbone vs.
from-scratch comparison); how long each variant trained is recorded in its run's <code>history.json</code>.</li>
<li><strong>Strategies B and C</strong> use a <code>CornerNet</code> checkpoint that may itself be a short CPU
run rather than a fully converged one; absolute numbers will improve with real training but the relative
comparison structure (grid cost vs. hybrid cost vs. dense-detector recall) is what this report is validating.</li>
<li>None of the three strategies identifies cards here: that needs a trained embedding model
(<code>ArtIndex</code>) this session does not have. Metrics below are detection-only (precision/recall/AP/geometry),
exactly where the plan's own phase order stops before "export only the winner to ONNX/WASM".</li>
</ul>
</div>
{"".join(sections)}
</body></html>"""
    out.write_text(html, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dataset", type=Path, help="table-scenes output directory (contains dataset.json, test/, challenge/)")
    parser.add_argument("--detector", type=Path, required=True, help="CornerNet checkpoint for strategies B and C")
    parser.add_argument(
        "--strategy-a-model",
        nargs=2,
        metavar=("NAME", "CHECKPOINT"),
        action="append",
        default=[],
        help="a trained TableCenterNet checkpoint to add as an extra Strategy-A row, e.g. --strategy-a-model pretrained data/runs/table-a-pretrained/best.pt; repeatable",
    )
    parser.add_argument("--out", type=Path, default=None, help="report output directory (default: <dataset>/report)")
    parser.add_argument("--limit", type=int, default=None, help="only evaluate the first N scenes per split (default: all)")
    parser.add_argument("--samples", type=int, default=6, help="sample scenes to render overlay galleries for, per split")
    args = parser.parse_args()
    from .detector import Detector
    from .table_detector import TableCenterNet

    out_dir = args.out or (args.dataset / "report")
    out_dir.mkdir(parents=True, exist_ok=True)
    header = json.loads((args.dataset / "dataset.json").read_text())
    detector = Detector(args.detector)
    table_variants = {}
    for name, checkpoint in args.strategy_a_model:
        model = TableCenterNet(pretrained=False).eval()
        model.load_state_dict(torch.load(checkpoint, map_location="cpu", weights_only=True))
        table_variants[name] = model
    predictors = build_predictors(detector, table_variants)
    colors = strategy_colors(list(predictors))
    print(f"strategies: {', '.join(strategy_label(k) for k in predictors)}")

    results, galleries = {}, {}
    for split in EVAL_SPLITS:
        manifest = args.dataset / split / "manifest.jsonl"
        rows = load_scenes(manifest, split)
        if args.limit:
            rows = rows[: args.limit]
        print(f"{split}: scoring {len(rows)} scenes x {len(predictors)} strategies ...")
        results[split] = {key: evaluate_strategy(rows, predict) for key, predict in predictors.items()}
        galleries[split] = render_gallery(rows, predictors, colors, out_dir / "samples", split, args.samples)

    (out_dir / "report.json").write_text(json.dumps({"header": header, "results": results}, indent=2, default=float))
    write_html_report(out_dir / "report.html", header, results, galleries, colors)
    total_samples = sum(len(g) for g in galleries.values()) * len(predictors)
    print(f"wrote {out_dir / 'report.json'} and {out_dir / 'report.html'} ({total_samples} sample overlay images)")


if __name__ == "__main__":
    main()
