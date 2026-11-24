"""Render measured MST comparisons to PNG/SVG. Requires matplotlib 3.10.8."""
import argparse
import csv
import json
from pathlib import Path
from statistics import median

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
ALGORITHMS = {
    "dlmst_prototype": ("Our DLMST prototype · 1 thread", "#AD3863", "o"),
    "petgraph_kruskal": ("petgraph Kruskal · 1 thread", "#3166AF", "s"),
    "petgraph_prim": ("petgraph Prim · 1 thread", "#168978", "^"),
    "pbbs_filter_kruskal_1t": ("PBBS Filter-Kruskal* · 1 thread", "#C08020", "D"),
    "pbbs_filter_kruskal_8t": ("PBBS Filter-Kruskal* · 8 workers", "#62616B", "v"),
}


def style(ax):
    ax.set_facecolor("#FFFFFF")
    ax.spines[["top", "right"]].set_visible(False)
    ax.spines[["left", "bottom"]].set_color("#C6CBD2")
    ax.grid(axis="y", color="#DEE3EA", linewidth=0.7, alpha=0.8)
    ax.set_axisbelow(True)
    ax.tick_params(length=3, color="#ABB3BF")


def save(fig, directory, name):
    for suffix in ["png", "svg"]:
        fig.savefig(directory / f"{name}.{suffix}", dpi=190, facecolor=fig.get_facecolor())
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=ROOT / "benchmarks/results/comparison.json")
    parser.add_argument("--output", type=Path, default=ROOT / "benchmarks/figures")
    args = parser.parse_args()
    data = json.loads(args.input.read_text())
    if not data["complete"] or not all(r["all_match"] for r in data["records"]):
        raise ValueError("Refusing to chart an incomplete or incorrect comparison")
    args.output.mkdir(parents=True, exist_ok=True)
    rows, meta = data["records"], data["metadata"]
    sizes = meta["sizes"]
    scenarios = [(f, d) for f in ["uniform", "local"] for d in meta["edges_per_vertex"]]
    plt.rcParams.update({
        "font.family": "DejaVu Sans", "font.size": 10,
        "axes.labelcolor": "#394556", "text.color": "#1D2939",
        "xtick.color": "#394556", "ytick.color": "#394556",
        "axes.titlesize": 12, "axes.titleweight": "bold", "svg.fonttype": "none",
    })
    summary = []
    for family, density in scenarios:
        for n in sizes:
            group = [r for r in rows if (r["family"], r["edges_per_vertex"], r["vertices"]) == (family, density, n)]
            for algorithm in ALGORITHMS:
                per_graph = [median(r["timings_ms"][algorithm]) for r in group]
                summary.append(dict(family=family, edges_per_vertex=density, vertices=n,
                                    edges=n*density, algorithm=algorithm, median_ms=median(per_graph),
                                    min_seed_median_ms=min(per_graph), max_seed_median_ms=max(per_graph)))
    with (args.output / "summary.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(summary[0]))
        writer.writeheader(); writer.writerows(summary)

    fig, axes = plt.subplots(2, 2, figsize=(13.6, 9.8), facecolor="#F7F9FC")
    fig.subplots_adjust(left=0.08, right=0.96, top=0.78, bottom=0.14, hspace=0.52, wspace=0.22)
    fig.text(0.065, 0.955, "MST runtime: how the prototype compares", fontsize=22, weight="bold")
    fig.text(0.065, 0.918, "Identical connected graphs, checked edge-for-edge. Lower is better.", fontsize=12, color="#536176")
    for ax, (family, density) in zip(axes.flat, scenarios):
        style(ax)
        for algorithm, (label, color, marker) in ALGORITHMS.items():
            points = [r for r in summary if r["family"] == family and r["edges_per_vertex"] == density and r["algorithm"] == algorithm]
            ax.plot(sizes, [p["median_ms"] for p in points], label=label, color=color,
                    marker=marker, markersize=5, linewidth=2.3 if algorithm == "dlmst_prototype" else 1.7)
            ax.fill_between(sizes, [p["min_seed_median_ms"] for p in points], [p["max_seed_median_ms"] for p in points], color=color, alpha=0.10)
        ax.set_xscale("log", base=2); ax.set_yscale("log")
        ax.set_xticks(sizes, [f"{n:,}" for n in sizes])
        ax.set_title(f"{'Uniform' if family == 'uniform' else 'Local'} connectivity  |  m = {density}n", loc="left", pad=13)
        ax.set_xlabel("Vertices (n)"); ax.set_ylabel("Elapsed time (ms, log scale)")
    handles, labels = axes[0, 0].get_legend_handles_labels()
    fig.legend(handles, labels, loc="upper left", bbox_to_anchor=(0.062, 0.888), ncol=2,
               frameon=False, columnspacing=2.5, handlelength=2.8, fontsize=10)
    fig.text(0.065, 0.075, f"Each point: median of {len(meta['seeds'])} graph medians, {meta['repetitions']} timed runs per graph after {meta['warmups']} warmup. Bands: range across graph medians.", fontsize=9, color="#536176")
    fig.text(0.065, 0.052, "Intel Core i7-12700F · WSL · release builds · parsing and graph preparation excluded · prototype counters enabled", fontsize=9, color="#536176")
    fig.text(0.065, 0.029, "*PBBS: small-batch guard + logging disabled. Research reference, not a global SOTA claim. Published O(m) DLMST is not timed.", fontsize=9, color="#536176")
    save(fig, args.output, "mst_runtime_comparison")

    fig, axes = plt.subplots(1, 2, figsize=(13.6, 6.9), facecolor="#F7F9FC")
    fig.subplots_adjust(left=0.08, right=0.96, top=0.77, bottom=0.30, wspace=0.27)
    fig.text(0.065, 0.935, "The prototype's performance gap", fontsize=22, weight="bold")
    fig.text(0.065, 0.885, "Elapsed time and weight-comparison work answer different questions.", fontsize=12, color="#536176")
    ax = axes[0]; style(ax)
    colors = ["#3166AF", "#AD3863", "#168978", "#C08020"]
    single_thread = ["petgraph_kruskal", "petgraph_prim", "pbbs_filter_kruskal_1t"]
    ratios = []
    for (family, density), color in zip(scenarios, colors):
        med, lo, hi = [], [], []
        for n in sizes:
            group = [r for r in rows if (r["family"], r["edges_per_vertex"], r["vertices"]) == (family, density, n)]
            paired = [median(r["timings_ms"]["dlmst_prototype"]) / min(median(r["timings_ms"][a]) for a in single_thread) for r in group]
            med.append(median(paired)); lo.append(min(paired)); hi.append(max(paired))
            ratios.append({"family": family, "density": density, "vertices": n, "slowdown": median(paired)})
        ax.plot(sizes, med, marker="o", color=color, label=f"{family.title()}, m = {density}n", linewidth=2)
        ax.fill_between(sizes, lo, hi, color=color, alpha=.10)
    ax.set_xscale("log", base=2)
    ax.set_ylim(bottom=0)
    ax.set_xticks(sizes, [f"{n:,}" for n in sizes])
    ax.yaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}×"))
    ax.set_xlabel("Vertices (n)"); ax.set_ylabel("Prototype / fastest measured 1-thread baseline")
    ax.set_title("Runtime gap on the same graph", loc="left", pad=14)
    ax.legend(frameon=False, fontsize=9, loc="best")

    ax = axes[1]; style(ax)
    categories = [("exact_msf", "Exact MSF (Kruskal)", "#AD3863"),
                  ("path", "Path queries", "#3166AF"), ("selection", "Batch selection", "#168978"),
                  ("boruvka", "Boruvka", "#C08020"), ("admissibility", "Admissibility", "#62616B"),
                  ("spoke", "Spokes", "#A4B1C3")]
    left = np.zeros(len(scenarios))
    for key, label, color in categories:
        values = []
        for family, density in scenarios:
            group = [r for r in rows if (r["family"], r["edges_per_vertex"], r["vertices"]) == (family, density, max(sizes))]
            values.append(sum(r["prototype_stats"]["comparisons"][key] for r in group) / sum(r["input_edges"] for r in group))
        ax.barh(range(len(scenarios)), values, left=left, label=label, color=color, height=.62)
        left += values
    ax.set_yticks(range(len(scenarios)), [f"{f.title()}\nm = {d}n" for f,d in scenarios]); ax.invert_yaxis()
    ax.set_xlim(0, max(left)*1.16)
    for i, value in enumerate(left): ax.text(value + max(left)*.02, i, f"{value:.1f}", va="center", fontsize=10, weight="bold")
    ax.set_xlabel("Weight comparisons per input edge (prototype only)")
    ax.set_title(f"Where comparisons go at n = {max(sizes):,}", loc="left", pad=14)
    ax.legend(loc="upper center", bbox_to_anchor=(.5, -.25), ncol=3, fontsize=8.5, frameon=False)
    fig.text(.065, .09, "Left: paired ratios per graph, then median across seeds. Right: summed comparison counts divided by summed input edges.", fontsize=9, color="#536176")
    fig.text(.065, .055, "Comparison shares are not runtime shares. Neither these measurements nor this prototype establish the paper's linear-time result.", fontsize=9, color="#536176")
    save(fig, args.output, "mst_cost_analysis")
    (args.output / "ratios.json").write_text(json.dumps(ratios, indent=2)+"\n")
    print(json.dumps({"figures": str(args.output), "largest_graph_ratios": [r for r in ratios if r["vertices"] == max(sizes)]}, indent=2))


if __name__ == "__main__":
    main()
