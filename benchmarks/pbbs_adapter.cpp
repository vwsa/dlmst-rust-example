// Adapter only. The actual Filter-Kruskal implementation is compiled unchanged
// from pinned PBBS sources, with its diagnostic timer disabled and a documented
// one-line guard against zero-sized speculative rounds on tiny batches.
#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <vector>
#include "MST.h"

int main(int argc, char** argv) {
  if (argc != 4) { std::cerr << "usage: pbbs_adapter graph.txt repetitions warmups\n"; return 1; }
  std::ifstream input(argv[1]);
  size_t n, m;
  if (!(input >> n >> m) || n == 0 || m == 0) return 2;
  parlay::sequence<wghEdge<vertexId, edgeWeight>> edges(m);
  for (auto& edge : edges) {
    if (!(input >> edge.u >> edge.v >> edge.weight)) return 3;
  }
  wghEdgeArray<vertexId, edgeWeight> graph(std::move(edges), n);
  const int repeats = std::stoi(argv[2]), warmups = std::stoi(argv[3]);
  if (repeats <= 0 || warmups < 0) return 4;
  std::vector<double> times;
  std::vector<edgeId> expected;
  for (int r = 0; r < repeats + warmups; ++r) {
    const auto start = std::chrono::steady_clock::now();
    auto selected = mst(graph);
    std::sort(selected.begin(), selected.end());
    const auto finish = std::chrono::steady_clock::now();
    if (r == 0) expected.assign(selected.begin(), selected.end());
    else if (!std::equal(expected.begin(), expected.end(), selected.begin(), selected.end())) return 5;
    if (r >= warmups) times.push_back(std::chrono::duration<double, std::milli>(finish-start).count());
  }
  std::cout << std::setprecision(12) << "{\"timings_ms\":[";
  for (size_t i=0; i<times.size(); ++i) { if(i) std::cout << ','; std::cout << times[i]; }
  std::cout << "],\"edge_ids\":[";
  for (size_t i=0; i<expected.size(); ++i) { if(i) std::cout << ','; std::cout << expected[i]; }
  std::cout << "]}\n";
}
