"""Download the pinned minimal PBBS sources and build the reference adapter on Linux."""
import argparse
import io
from pathlib import Path
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PBBS = "3932578095bb73a2b5dbf8102ed5ebe3f5041b19"
PARLAY = "f8e38c71efc209ff60cf31bce3723e74d3730832"
FILES = {
    "common/graph.h": "common/graph.h",
    "common/atomics.h": "common/atomics.h",
    "common/speculative_for.h": "common/speculative_for.h",
    "algorithm/kth_smallest.h": "algorithm/kth_smallest.h",
    "algorithm/union_find.h": "algorithm/union_find.h",
    "benchmarks/minSpanningForest/bench/MST.h": "MST.h",
    "benchmarks/minSpanningForest/parallelFilterKruskal/MST.C": "MST.C",
}


def download(url):
    request = urllib.request.Request(url, headers={"User-Agent": "dlmst-reproduction-benchmark"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, default=ROOT / "target/pbbs-source")
    parser.add_argument("--parlay", type=Path, default=ROOT / "target/pbbs-parlay")
    parser.add_argument("--reuse-sources", action="store_true", help="Use already downloaded files; only for the pinned revisions above")
    args = parser.parse_args()
    if not args.reuse_sources:
        for source, destination in FILES.items():
            path = args.sources / destination
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(download(f"https://raw.githubusercontent.com/cmuparlay/pbbsbench/{PBBS}/{source}"))
        archive = download(f"https://codeload.github.com/cmuparlay/parlaylib/tar.gz/{PARLAY}")
        args.parlay.mkdir(parents=True, exist_ok=True)
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
            for member in tar.getmembers():
                parts = Path(member.name).parts
                if len(parts) < 2:
                    continue
                member.name = str(Path(*parts[1:]))
                tar.extract(member, args.parlay, filter="data")
    for source, before, after in [
        ("MST.C", 'timer t("mst", true)', 'timer t("mst", false)'),
        ("common/speculative_for.h", "long currentRoundSize = maxRoundSize/4;", "long currentRoundSize = std::max(1L, maxRoundSize/4);"),
    ]:
        path = args.sources / source
        text = path.read_text()
        if text.count(before) == 1:
            path.write_text(text.replace(before, after))
        elif text.count(after) != 1:
            raise ValueError(f"Unexpected source version for {source}")
    link = args.sources / "parlay"
    if not link.exists():
        link.symlink_to((args.parlay / "include/parlay").resolve(), target_is_directory=True)
    command = ["g++", "-O3", "-DNDEBUG", "-std=c++17", "-pthread",
               "-I", str(args.sources), "-I", str(args.parlay / "include"),
               str(ROOT / "benchmarks/pbbs_adapter.cpp"), str(args.sources / "MST.C"),
               "-o", str(ROOT / "target/pbbs-filter-kruskal")]
    subprocess.run(command, check=True)
    print(f"Built PBBS {PBBS} with Parlay {PARLAY}; two documented patches applied.")


if __name__ == "__main__":
    main()
