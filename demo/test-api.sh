#!/usr/bin/env bash
# Quick test: run the exact same API calls as the demo scenario,
# WITHOUT ffmpeg/pw-record/TTS. If the app crashes here too,
# it's a code bug. If it doesn't, it's ffmpeg/pw-record interference.
set -u

API="http://127.0.0.1:8765"
c() { echo "  → $1"; curl -sf -X POST "$API/$1" -H "Content-Type: application/json" -d "$2" 2>/dev/null; echo ""; }

echo "=== API smoke test (no recording, no TTS) ==="
echo "Make sure the app is running with: cargo run -- --skip-wizard --model models/gemma-4-E4B-it-Q4_K_M.gguf"
echo ""

echo "[1] rack reset"
c "api/rack/reset" '{}'
sleep 1

echo "[2] add bass"
c "api/rack/add" '{"kind":"bass"}'
sleep 0.5

echo "[3] add 808"
c "api/rack/add" '{"kind":"808"}'
sleep 0.5

echo "[4] add 909"
c "api/rack/add" '{"kind":"909"}'
sleep 0.5

echo "[5] add agent"
c "api/rack/agent" '{"persona":"PULSE","scope":[],"model":"gemma"}'
sleep 1

echo "[6] scroll to sequencer"
c "api/scroll" '{"target":"sequencer"}'
sleep 0.5

echo "[7] flip back"
c "api/flip" '{"show_back":true}'
sleep 2

echo "[8] flip front"
c "api/flip" '{"show_back":false}'
sleep 1

echo "[9] play"
c "api/sequencer/play" '{}'
sleep 2

echo "[10] scroll to console"
c "api/scroll" '{"target":"console"}'
sleep 0.5

echo "[11] PROMPT (this is where the demo crashes)"
c "api/prompt" '{"prompt":"set up a classic acid house groove with a driving bass line, four on the floor kick, and hi-hats"}'
echo "  Waiting 15s for inference..."
sleep 15

echo "[12] scroll to bass"
c "api/scroll" '{"target":"bass"}'
sleep 2

echo "[13] stop"
c "api/sequencer/stop" '{}'

echo ""
echo "=== Done. Did the app survive? ==="
