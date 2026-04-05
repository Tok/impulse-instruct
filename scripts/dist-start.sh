#!/usr/bin/env bash
# Impulse Instruct launcher
# Place this next to impulse-instruct-linux-x86_64 and run it instead of the
# binary directly. It opens a terminal window so log output is visible, which
# is especially useful for diagnosing startup errors (missing model, no audio
# device, etc.).

DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$DIR/impulse-instruct-linux-x86_64"

if [ ! -x "$BIN" ]; then
    echo "ERROR: Cannot find $BIN" >&2
    echo "Place start.sh next to impulse-instruct-linux-x86_64 and try again." >&2
    exit 1
fi

# If we're already attached to a terminal, just run directly.
if [ -t 0 ] && [ -t 1 ]; then
    exec "$BIN" "$@"
fi

# Not in a terminal (launched from file manager or desktop shortcut).
# Try to open a terminal emulator so log output is visible.
CMD="'$BIN' $(printf '%q ' "$@"); echo; echo '--- Press Enter to close ---'; read _"

if command -v gnome-terminal &>/dev/null; then
    exec gnome-terminal -- bash -c "$CMD"
elif command -v konsole &>/dev/null; then
    exec konsole -e bash -c "$CMD"
elif command -v xfce4-terminal &>/dev/null; then
    exec xfce4-terminal -e "bash -c \"$CMD\""
elif command -v xterm &>/dev/null; then
    exec xterm -e bash -c "$CMD"
elif command -v x-terminal-emulator &>/dev/null; then
    exec x-terminal-emulator -e bash -c "$CMD"
else
    # No terminal found — run directly.
    # Errors will be written to impulse-instruct-error.log next to the binary.
    exec "$BIN" "$@"
fi
