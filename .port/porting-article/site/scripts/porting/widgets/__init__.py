"""Registry of widget data builders: slug -> build(research_dir, mockbuild_dir) -> dict."""
from . import w01_three_reviewers, w02_agents_vs_human, w03_test_contract, w04_cutting_the_cycle, w05_perf_replay, w06_moving_target, w07_translation_lens, w08_defect_map, w09_memory_safety, w10_verification_matrix, w11_cost, w12_every_prompt, static_charts

REGISTRY: dict = {
    "three-reviewers": w01_three_reviewers.build,
    "agents-vs-human": w02_agents_vs_human.build,
    "test-contract": w03_test_contract.build,
    "cutting-the-cycle": w04_cutting_the_cycle.build,
    "perf-replay": w05_perf_replay.build,
    "moving-target": w06_moving_target.build,
    "translation-lens": w07_translation_lens.build,
    "defect-map": w08_defect_map.build,
    "defect-map-details": w08_defect_map.build_details,
    "memory-safety": w09_memory_safety.build,
    "verification-matrix": w10_verification_matrix.build,
    "cost": w11_cost.build,
    "every-prompt": w12_every_prompt.build,
    "stats-card": static_charts.build_stats_card,
    "strategy-timeline": static_charts.build_strategy_timeline,
    "checks-growth": static_charts.build_checks_growth,
    "audit-funnel": static_charts.build_audit_funnel,
    "orchestration": static_charts.build_orchestration,
    "unsafe-density": static_charts.build_unsafe_density,
    "platform-table": static_charts.build_platform_table,
}
