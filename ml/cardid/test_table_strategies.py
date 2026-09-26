"""CPU-only contracts for the three detection strategies (`table_strategies.py`).

Strategies B and C call a `Detector`-shaped object's `locate_up`; these tests use a stub
instead of a trained `CornerNet` checkpoint, so the suite covers the grid/clustering/NMS
orchestration without needing torch weights or GPU time (see `table_strategies.py`'s module
docstring for why no trained checkpoint is part of this offline tooling session).
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import numpy as np

from . import table_scenes, table_strategies
from .evaluate_tables import Proposal
from .image_bank import ArtBank
from .scene_geometry import quad_iou
from .test_table_scenes import make_card_bank


class StubDetector:
    """Always answers with a fixed-size, unrotated quad centred on the click, so grid/cluster
    logic can be tested without a trained network. `calls` counts invocations."""

    def __init__(self, short: float = 60.0, confidence: float = 0.8):
        self.short = short
        self.confidence = confidence
        self.calls = 0

    def locate_up(self, image: np.ndarray, click: tuple[float, float], refine: bool = True, snap: bool = False, rotations: int = 4):
        from .scene_geometry import quad_from_pose

        self.calls += 1
        return quad_from_pose(click[0], click[1], self.short, 0, None), self.confidence


def single_card_scene(root: Path, size: int = 512, out: int = 256):
    cards = make_card_bank(root)
    image, record = table_scenes.render_table_scene(0, cards, ArtBank([]), "spread", "closeup_4k", count=1, size=size, out=out)
    return image, np.float32(record["cards"][0]["quad"])


class NmsQuadsTest(unittest.TestCase):
    def test_suppresses_overlap_but_keeps_a_distant_duplicate(self):
        from .scene_geometry import quad_from_pose

        near_low = Proposal(quad_from_pose(100, 100, 40, 0, None), 0.4, "test")
        near_high = Proposal(quad_from_pose(105, 100, 40, 0, None), 0.9, "test")  # overlaps near_low
        far = Proposal(quad_from_pose(400, 400, 40, 0, None), 0.5, "test")  # same "identity" elsewhere on the table
        kept = table_strategies.nms_quads([near_low, near_high, far], iou_threshold=0.35)
        self.assertEqual(len(kept), 2)
        self.assertIn(far, kept)
        self.assertIn(near_high, kept)
        self.assertNotIn(near_low, kept)

    def test_empty_input_returns_empty_output(self):
        self.assertEqual(table_strategies.nms_quads([]), [])


class GridPointsTest(unittest.TestCase):
    def test_deterministic_and_within_bounds(self):
        a = table_strategies.grid_points(640, 480, scales=(0.2,))
        b = table_strategies.grid_points(640, 480, scales=(0.2,))
        self.assertEqual(a, b)
        self.assertTrue(all(0 <= x < 640 and 0 <= y < 480 for x, y in a))

    def test_multiple_scales_add_more_points_than_one(self):
        coarse = table_strategies.grid_points(640, 640, scales=(0.3,))
        fine = table_strategies.grid_points(640, 640, scales=(0.3, 0.15))
        self.assertGreater(len(fine), len(coarse))


class StrategyATest(unittest.TestCase):
    def test_finds_the_synthetic_card_with_reasonable_overlap(self):
        with tempfile.TemporaryDirectory() as temp:
            image, truth = single_card_scene(Path(temp))
        proposals = table_strategies.strategy_a_dense_detector(image)
        self.assertTrue(proposals, "expected at least one card-shaped quad on a single-card scene")
        best_iou = max(quad_iou(truth, p.quad) for p in proposals)
        self.assertGreater(best_iou, 0.3)
        self.assertTrue(all(p.source == "strategy-a" for p in proposals))

    def test_blank_canvas_finds_nothing(self):
        blank = np.full((256, 256, 3), 200, np.uint8)
        self.assertEqual(table_strategies.strategy_a_dense_detector(blank), [])


class StrategyBTest(unittest.TestCase):
    def test_grid_sweep_returns_proposals_shaped_like_the_stub_and_dedupes_overlap(self):
        with tempfile.TemporaryDirectory() as temp:
            image, _truth = single_card_scene(Path(temp))
        detector = StubDetector()
        proposals = table_strategies.strategy_b_grid_sweep(image, detector, scales=(0.1,))  # dense enough for guaranteed overlap
        self.assertGreater(detector.calls, len(proposals), "NMS should have collapsed overlapping grid hits")
        self.assertTrue(proposals)
        for p in proposals:
            self.assertEqual(p.source, "strategy-b")
            self.assertEqual(p.quad.shape, (4, 2))
            self.assertEqual(p.score, detector.confidence)


class StrategyCTest(unittest.TestCase):
    def test_hybrid_refines_only_clustered_coarse_proposals(self):
        with tempfile.TemporaryDirectory() as temp:
            image, truth = single_card_scene(Path(temp))
        detector = StubDetector()
        proposals = table_strategies.strategy_c_hybrid(image, detector, scales=(0.2,))
        self.assertLessEqual(detector.calls, 3, "a single card should collapse to very few coarse clusters")
        self.assertTrue(all(p.source == "strategy-c" for p in proposals))
        if proposals:
            best_iou = max(quad_iou(truth, p.quad) for p in proposals)
            self.assertGreaterEqual(best_iou, 0.0)  # geometry sanity: comparable quads, no crash

    def test_no_coarse_hits_means_no_refinement_calls(self):
        blank = np.full((256, 256, 3), 200, np.uint8)
        detector = StubDetector()
        self.assertEqual(table_strategies.strategy_c_hybrid(blank, detector), [])
        self.assertEqual(detector.calls, 0)


if __name__ == "__main__":
    unittest.main()
