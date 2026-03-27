#!/bin/bash
# Helper script to run TLC model checker on PipePool specification
#
# Usage:
#   ./run_tlc.sh              # Run with default settings
#   ./run_tlc.sh -workers 4   # Run with custom options

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Default TLC options
WORKERS="${WORKERS:-auto}"
SPEC="PipePool.tla"

echo "========================================="
echo "TLA+ Model Checker (TLC)"
echo "========================================="
echo "Specification: $SPEC"
echo "Configuration: PipePool.cfg"
echo "Workers: $WORKERS"
echo "========================================="
echo ""

# Run TLC with parallel garbage collection for better performance
java -XX:+UseParallelGC -cp tla2tools.jar tlc2.TLC \
    -workers "$WORKERS" \
    -config PipePool.cfg \
    "$@" \
    "$SPEC"

echo ""
echo "========================================="
echo "TLC run completed"
echo "========================================="
