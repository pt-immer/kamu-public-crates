#!/usr/bin/env bash
# How much machine ONE YugabyteDB node is allowed to take. Sourced, never executed.
#
#   source kamu-money-pg/yb/node-limits.sh
#   docker run --memory "$YB_NODE_MEM" --memory-swap "$YB_NODE_MEM" ... \
#       bin/yugabyted start --tserver_flags="memory_limit_hard_bytes=$YB_TSERVER_MEM_BYTES" \
#                           --master_flags="memory_limit_hard_bytes=$YB_MASTER_MEM_BYTES"
#
# A CONTAINER SEES THE HOST'S RAM, NOT ITS OWN LIMIT, and that is the whole reason this file
# exists. yugabyted sizes each tserver as a share of the memory it can see, so three nodes on one
# host each size themselves for the WHOLE host and together promise several times what exists.
# Nothing capped anything before this. It survived only because every machine that had ever run
# these suites was large enough to hide the arithmetic -- on a 16 GB runner it is an OOM kill of
# whichever node the kernel picks, and it surfaces as a cluster that "failed to come up" rather
# than as anything mentioning memory.
#
# THESE ARE CORRECTNESS SUITES, NOT BENCHMARKS. The numbers are chosen to make an RF=3 cluster plus
# read replicas fit a 16 GB CI runner, not to make any of it fast: three primaries at 2 GB plus two
# replicas is 10 GB, which leaves the daemon and the runner itself comfortable headroom. A
# benchmark wants entirely different values, which is exactly why these are variables and not
# literals -- `YB_NODE_MEM=8g just bench-yb` and the caps move with you.
#
# --memory-swap EQUAL TO --memory disables swap for the container. A node that swaps instead of
# failing converts a memory problem into a timing problem, and these harnesses read timing as
# evidence: a soak that got slower because it was paging would be reported as a soak that got
# slower. Failing is the honest outcome.
#
# ONE DEFINITION, NOT ONE PER HARNESS. cluster.sh and run-yb.sh both start nodes and both source
# this. The retry classifier in cluster.sh records what happens otherwise: it lived in two scripts,
# drifted, and the forgotten copy was the one deciding whether a real failure got retried into
# silence. Two copies of a resource policy would drift the same way, and the symptom would be one
# harness OOMing on a machine where the other passes.

# Docker's hard ceiling per node container.
YB_NODE_MEM="${YB_NODE_MEM:-2g}"
# What yugabyted is TOLD it may use, kept under the container ceiling so the process refuses
# cleanly inside its own limit rather than being killed from outside it.
YB_TSERVER_MEM_BYTES="${YB_TSERVER_MEM_BYTES:-1073741824}"   # 1 GiB
YB_MASTER_MEM_BYTES="${YB_MASTER_MEM_BYTES:-536870912}"      # 512 MiB
