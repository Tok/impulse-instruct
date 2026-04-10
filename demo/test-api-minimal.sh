#!/usr/bin/env bash
# Minimal tests to isolate WHICH operation causes the crash.
# Run the app first, then run this script.
API="http://127.0.0.1:8765"
c() { echo "  → $*"; curl -sf -X POST "$API/$1" -H "Content-Type: application/json" -d "$2" 2>/dev/null; echo ""; }

echo "=== Test A: prompt WITHOUT rack reset (should work like normal) ==="
echo "  Sending prompt..."
c "api/prompt" '{"prompt":"make a simple beat"}'
echo "  Waiting 15s..."
sleep 15
echo "  Did the app survive Test A? (y/n)"
read -r A

echo ""
echo "=== Test B: rack reset + prompt (NO new modules) ==="
c "api/rack/reset" '{}'
sleep 1
echo "  Sending prompt after reset..."
c "api/prompt" '{"prompt":"make a simple beat"}'
echo "  Waiting 15s..."
sleep 15
echo "  Did the app survive Test B? (y/n)"
read -r B

echo ""
echo "=== Test C: rack reset + add modules + prompt (NO agent) ==="
c "api/rack/reset" '{}'
sleep 0.5
c "api/rack/add" '{"kind":"bass"}'
c "api/rack/add" '{"kind":"808"}'
sleep 1
echo "  Sending prompt (no agent added)..."
c "api/prompt" '{"prompt":"make a simple beat"}'
echo "  Waiting 15s..."
sleep 15
echo "  Did the app survive Test C? (y/n)"
read -r C

echo ""
echo "=== Test D: rack reset + add modules + add agent + prompt ==="
c "api/rack/reset" '{}'
sleep 0.5
c "api/rack/add" '{"kind":"bass"}'
c "api/rack/add" '{"kind":"808"}'
c "api/rack/agent" '{"persona":"PULSE","scope":[],"model":"gemma"}'
sleep 1
echo "  Sending prompt..."
c "api/prompt" '{"prompt":"make a simple beat"}'
echo "  Waiting 15s..."
sleep 15
echo "  Did the app survive Test D? (y/n)"
read -r D

echo ""
echo "Results: A=$A  B=$B  C=$C  D=$D"
echo "First 'n' tells us which operation causes the crash."
