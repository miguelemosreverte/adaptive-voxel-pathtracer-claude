#!/bin/bash

# Test performance at different camera positions
echo "Testing performance at different camera positions..."

# Test 1: Outside the box (should be fast)
echo "Test 1: Camera outside the box"
~/.cargo/bin/cargo run -- --screenshot --cam-x 0 --cam-y 1 --cam-z=-2 --look-x 0 --look-y 1 --look-z 0 --duration 3 2>/dev/null &

# Test 2: At entrance (medium performance)
echo "Test 2: Camera at entrance"
~/.cargo/bin/cargo run -- --screenshot --cam-x 0 --cam-y 1 --cam-z=0 --look-x 0 --look-y 1 --look-z 1 --duration 3 2>/dev/null &

# Test 3: Deep inside (should be slow)
echo "Test 3: Camera deep inside"
~/.cargo/bin/cargo run -- --screenshot --cam-x 0 --cam-y 1 --cam-z=1 --look-x 0 --look-y 1 --look-z 1.5 --duration 3 2>/dev/null &

wait

echo "Performance tests complete. Check performance_report_*.md files for results."