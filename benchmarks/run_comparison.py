"""Run matched-input CPU comparisons. Python stdlib only; requires both adapters."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PBBS_COMMIT = "3932578095bb73a2b5dbf8102ed5ebe3f5041b19"
PARLAY_COMMIT = "f8e38c71efc209ff60cf31bce3723e74d3730832"


def graph(n, density, seed, family):
    rng = random.Random(seed)
    edges = []
    # A spanning tree guarantees that petgraph's single-component Prim is valid.
    for v in range(1, n):
        u = rng.randrange(v) if family == "uniform" else v - 1
        edges.append({"u": u, "v": v, "weight": rng.randint(-1000, 1000)})
    while len(edges) < density * n:
        u = rng.randrange(n)
        if family == "uniform":
            v = rng.randrange(n - 1)
            v += v >= u
        else:
            v = (u + rng.choice([-1, 1]) * rng.randint(1, min(16, n - 1))) % n
        edges.append({"u": u, "v": v, "weight": rng.randint(-1000, 1000)})
    rng.shuffle(edges)
    return {"vertices": n, "edges": edges}


def call(command, **kwargs):
    result = subprocess.run(command, capture_output=True, text=True, timeout=180, **kwargs)
    if result.returncode:
        raise RuntimeError(f"Adapter failed ({result.returncode}): {command}\n{result.stderr}\n{result.stdout}")
    return json.loads(result.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", type=Path, default=ROOT / "target/release/compare")
    parser.add_argument("--pbbs", type=Path, default=ROOT / "target/pbbs-filter-kruskal")
    parser.add_argument("--sizes", default="128,512,2048,8192")
    parser.add_argument("--densities", default="8,32")
    parser.add_argument("--seeds", default="42,43,44")
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--output", type=Path, default=ROOT / "benchmarks/results/comparison.json")
    args = parser.parse_args()
    sizes = [int(x) for x in args.sizes.split(",")]
    densities = [int(x) for x in args.densities.split(",")]
    seeds = [int(x) for x in args.seeds.split(",")]
    if min(sizes) < 2 or min(densities) < 1 or args.repetitions < 1 or args.warmups < 0:
        parser.error("invalid sizes, densities or repetition counts")
    data = {
        "complete": False,
        "metadata": {
            "date": "2026-11-24", "platform": platform.platform(),
            "cpu": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "unknown"),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "g++": subprocess.check_output(["g++", "--version"], text=True).splitlines()[0],
            "pbbs_commit": PBBS_COMMIT, "parlay_commit": PARLAY_COMMIT,
            "pbbs_patches": ["Disable diagnostic timer", "Clamp initial speculative round size to at least one (upstream nine-item batch otherwise stalls)"],
            "sizes": sizes, "edges_per_vertex": densities, "seeds": seeds,
            "repetitions": args.repetitions, "warmups": args.warmups,
            "timing": "Prepared graph to sorted MST edge IDs; excludes IO, parsing, input generation, and representation construction.",
            "instrumentation": "Prototype comparison counters enabled; reference algorithms uninstrumented.",
            "weights": "Integers in [-1000,1000], exact in both Rust i64 and PBBS float32; ties broken by original edge ID.",
            "scope": "Synthetic connected multigraphs; PBBS Filter-Kruskal is a research benchmark reference, not a claim of globally fastest MST.",
        },
        "records": [],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    total = len(sizes) * len(densities) * len(seeds) * 2
    with tempfile.TemporaryDirectory(prefix="dlmst-comparison-") as tmp:
        tmp = Path(tmp)
        for family in ["uniform", "local"]:
            for density in densities:
                for n in sizes:
                    for seed in seeds:
                        g = graph(n, density, seed, family)
                        raw = json.dumps(g, separators=(",", ":"))
                        json_path, text_path = tmp / "graph.json", tmp / "graph.txt"
                        json_path.write_text(raw)
                        with text_path.open("w") as stream:
                            stream.write(f"{n} {len(g['edges'])}\n")
                            stream.writelines(f"{e['u']} {e['v']} {e['weight']}\n" for e in g["edges"])
                        rust = call([str(args.rust), str(json_path), "--repetitions", str(args.repetitions), "--warmups", str(args.warmups)])
                        for threads in ([1, 8] if seed % 2 == 0 else [8, 1]):
                            environment = dict(os.environ, PARLAY_NUM_THREADS=str(threads))
                            pbbs = call([str(args.pbbs), str(text_path), str(args.repetitions), str(args.warmups)], env=environment)
                            if pbbs["edge_ids"] != rust["edge_ids"]:
                                raise RuntimeError(f"PBBS mismatch: {family=} {density=} {n=} {seed=} {threads=}")
                            rust["timings_ms"][f"pbbs_filter_kruskal_{threads}t"] = pbbs["timings_ms"]
                        rust.pop("edge_ids")
                        rust.update(family=family, edges_per_vertex=density, seed=seed,
                                    input_sha256=hashlib.sha256(raw.encode()).hexdigest())
                        data["records"].append(rust)
                        args.output.write_text(json.dumps(data, indent=2) + "\n")
                        print(f"{len(data['records'])}/{total}: {family}, n={n}, m={density*n}, seed={seed}: all five implementations matched", flush=True)
    data["complete"] = True
    args.output.write_text(json.dumps(data, indent=2) + "\n")


if __name__ == "__main__":
    main()
