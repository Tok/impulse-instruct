#!/usr/bin/env bash
# Convenience wrapper — delegates to download-models.sh tts
exec "$(dirname "$0")/download-models.sh" tts
