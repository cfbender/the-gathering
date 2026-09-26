"""The three multi-card detection strategies from the Super AI mode plan, sharing one output
schema (`evaluate_tables.Proposal`) so `evaluate_strategy` scores them identically.

None of the three identifies cards: the plan's "existing embed/search recognizer" (`index.py`,
`ArtIndex`) needs a trained `Embedder` checkpoint, and training one needs GPU hours this
offline CPU session does not have. All three strategies stop at detection (a quad plus a
confidence score) exactly where the plan's own phase order stops first ("Offline baselines:
Strategy B, then A and C ... export only the winner to ONNX/WASM"); wiring a trained
`ArtIndex` onto the winning strategy's accepted quads is the next phase, not this one.

- **Strategy A** (`strategy_a_dense_detector`) is the plan's trained oriented-box detector:
  `table_detector.TableCenterNet`, a dense CenterNet-style head over the same MobileNetV3-Small
  backbone the click-conditioned localizer uses, trained on the table-scene manifests
  (`train_table_detector.py`). Pass a loaded `TableCenterNet` as `model` to use it; without one,
  the function falls back to `dense_card_quads` -- a classical edge/contour proposal generator
  (`detect.find_card_quad` generalised from "the quad containing this click" to "every quad
  like this anywhere") -- so the strategy still runs with no checkpoint at hand (quick
  iteration, tests, or before training finishes).
- **Strategy B** (`strategy_b_grid_sweep`) runs the existing two-stage click-conditioned
  learned localizer (`detector.Detector`) at deterministic multi-scale grid points, the plan's
  "no-new-detector-training baseline".
- **Strategy C** (`strategy_c_hybrid`) uses a sparse grid of the cheap classical finder only to
  make coarse proposals, clusters them, then runs the learned localizer once per cluster
  centroid instead of at every grid point.
"""

from __future__ import annotations

import cv2
import numpy as np
import torch

from .data import to_tensor
from .detect import CARD_ASPECT, find_card_quad, order_corners, quad_aspect
from .evaluate_tables import Proposal
from .scene_geometry import quad_iou
from .table_detector import TABLE_STRIDE, decode_detections

__all__ = ["dense_card_quads", "grid_points", "model_dense_quads", "nms_quads", "strategy_a_dense_detector", "strategy_b_grid_sweep", "strategy_c_hybrid"]


def nms_quads(proposals: list[Proposal], iou_threshold: float = 0.35) -> list[Proposal]:
    """Greedy suppression by descending score. Two proposals for the same card at different
    table positions have near-zero IoU and both survive; the plan is explicit that this is
    correct (duplicate identities at different locations are valid overlays), so only spatial
    overlap -- never a repeated identity -- is grounds for suppression here."""
    ordered = sorted(proposals, key=lambda p: -p.score)
    kept: list[Proposal] = []
    for candidate in ordered:
        if all(quad_iou(candidate.quad, k.quad) < iou_threshold for k in kept):
            kept.append(candidate)
    return kept


def dense_card_quads(image: np.ndarray, min_side: int = 24) -> list[Proposal]:
    """Every convex, card-aspect quadrilateral anywhere in `image` (RGB uint8), unfiltered by
    any click point. Reuses `detect.find_card_quad`'s edge maps and shape test; score is the
    quad's pixel area, a shape-quality tie-breaker for NMS, not an identification confidence."""
    gray = cv2.cvtColor(image, cv2.COLOR_RGB2GRAY)
    gray = cv2.bilateralFilter(gray, 7, 40, 40)
    median = float(np.median(gray))
    edge_maps = [
        cv2.Canny(gray, max(0, 0.66 * median), min(255, 1.33 * median)),
        cv2.Canny(gray, 30, 90),
        cv2.adaptiveThreshold(gray, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY_INV, 21, 5),
    ]
    kernel = cv2.getStructuringElement(cv2.MORPH_RECT, (3, 3))
    proposals = []
    for edges in edge_maps:
        edges = cv2.dilate(edges, kernel, iterations=1)
        contours, _ = cv2.findContours(edges, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
        for c in contours:
            if cv2.contourArea(c) < min_side * min_side * CARD_ASPECT:
                continue
            approx = cv2.approxPolyDP(c, 0.02 * cv2.arcLength(c, True), True)
            if len(approx) != 4 or not cv2.isContourConvex(approx):
                continue
            quad = order_corners(approx)
            if not 1.15 <= quad_aspect(quad) <= 1.7:
                continue
            proposals.append(Proposal(quad, float(cv2.contourArea(quad)), "strategy-a"))
    return proposals


def model_dense_quads(
    image: np.ndarray, model, device: torch.device | None = None, input_size: int | None = None, score_threshold: float = 0.3
) -> list[Proposal]:
    """`TableCenterNet` proposals for the whole frame in one forward pass. `image` is resized to
    the model's trained input size (its `.input_size` attribute if set, else `TABLE_INPUT`);
    quads are scaled back to `image`'s own pixel space before returning."""
    from .table_detector import TABLE_INPUT

    device = device or next(model.parameters()).device
    size = input_size or getattr(model, "input_size", TABLE_INPUT)
    h, w = image.shape[:2]
    resized = cv2.resize(image, (size, size), interpolation=cv2.INTER_AREA if size < max(h, w) else cv2.INTER_LINEAR)
    x = to_tensor(resized).unsqueeze(0).to(device)
    with torch.no_grad():
        heat, pose, up = model(x)
    detections = decode_detections(heat[0], pose[0], up[0], TABLE_STRIDE, score_threshold)
    scale_x, scale_y = w / size, h / size
    return [Proposal(quad * np.float32([scale_x, scale_y]), score, "strategy-a-learned") for quad, score in detections]


def strategy_a_dense_detector(
    image: np.ndarray, model=None, device: torch.device | None = None, min_side: int = 24, iou_threshold: float = 0.35
) -> list[Proposal]:
    """The real Strategy A: `model_dense_quads` when a trained `TableCenterNet` is given,
    otherwise the classical `dense_card_quads` fallback (see module docstring)."""
    proposals = model_dense_quads(image, model, device) if model is not None else dense_card_quads(image, min_side)
    return nms_quads(proposals, iou_threshold)


def grid_points(width: int, height: int, scales: tuple[float, ...] = (0.12, 0.22)) -> list[tuple[float, float]]:
    """Deterministic multi-scale grid of click points covering the frame: one coarser and one
    finer pass by default, so a card between the coarse grid's points is still caught by the
    finer one without the cost of a dense fine grid everywhere."""
    points: list[tuple[float, float]] = []
    for scale in scales:
        step_x, step_y = width * scale, height * scale
        xs = np.arange(step_x / 2, width, step_x)
        ys = np.arange(step_y / 2, height, step_y)
        points.extend((float(x), float(y)) for y in ys for x in xs)
    return points


def strategy_b_grid_sweep(image: np.ndarray, detector, scales: tuple[float, ...] = (0.12, 0.22), iou_threshold: float = 0.35) -> list[Proposal]:
    """The existing two-stage click-conditioned localizer (`detector.Detector`) run at every
    grid point, clustered by IoU with the highest-confidence proposal per cluster kept.
    Confidence is the localizer's own up-vote length: the fraction of its rotated passes that
    agreed on the card's printed orientation (`Detector.locate_up`)."""
    h, w = image.shape[:2]
    proposals = []
    for x, y in grid_points(w, h, scales):
        quad, up_confidence = detector.locate_up(image, (x, y))
        proposals.append(Proposal(quad, up_confidence, "strategy-b"))
    return nms_quads(proposals, iou_threshold)


def strategy_c_hybrid(image: np.ndarray, detector, scales: tuple[float, ...] = (0.18,), min_side: int = 24, iou_threshold: float = 0.35) -> list[Proposal]:
    """A sparse grid makes coarse proposals with the cheap classical finder (no neural network
    call per point), clustered by IoU; the learned localizer then refines each cluster's
    centroid once. Grid points where the classical finder sees nothing card-shaped are simply
    skipped -- that is the cost this strategy accepts for not calling the neural net at every
    point, and cards it misses this way show up as lower recall in the comparison report."""
    h, w = image.shape[:2]
    coarse = []
    for x, y in grid_points(w, h, scales):
        quad = find_card_quad(image, (x, y), min_side)
        if quad is not None:
            coarse.append(Proposal(quad, float(cv2.contourArea(quad)), "strategy-c-coarse"))
    clustered = nms_quads(coarse, iou_threshold)
    refined = []
    for cluster in clustered:
        cx, cy = cluster.quad.mean(axis=0)
        quad, up_confidence = detector.locate_up(image, (float(cx), float(cy)))
        refined.append(Proposal(quad, up_confidence, "strategy-c"))
    return nms_quads(refined, iou_threshold)
